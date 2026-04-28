use futures_util::stream::Stream;
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::time::{Duration, sleep};

use crate::config::{
    ClientOptions, ConnectionConfig, prepare_connection_config,
};
use crate::error::RlStatsError;
use crate::events::{StatsEvent, parse_stats_event_value};

pub struct RocketLeagueStatsClient {
    reader: OwnedReadHalf,
    writer: OwnedWriteHalf,
    read_buffer: Vec<u8>,
    connection: ConnectionConfig,
}

impl RocketLeagueStatsClient {
    pub async fn connect(options: ClientOptions) -> Result<Self, RlStatsError> {
        let connection = prepare_connection_config(&options)?;
        let (reader, writer) = connect_socket(&connection).await?;

        Ok(Self {
            reader,
            writer,
            read_buffer: Vec::new(),
            connection,
        })
    }

    pub fn connection(&self) -> &ConnectionConfig {
        &self.connection
    }

    pub async fn connect_with_retry(
        options: ClientOptions,
        max_attempts: usize,
        retry_delay: Duration,
    ) -> Result<Self, RlStatsError> {
        let attempts = max_attempts.max(1);

        for attempt in 0..attempts {
            match Self::connect(options.clone()).await {
                Ok(client) => return Ok(client),
                Err(error) => {
                    if attempt + 1 == attempts {
                        return Err(error);
                    }

                    sleep(retry_delay).await;
                }
            }
        }

        unreachable!("attempts is always >= 1")
    }

    pub async fn reconnect(&mut self) -> Result<(), RlStatsError> {
        let (reader, writer) = connect_socket(&self.connection).await?;
        self.reader = reader;
        self.writer = writer;
        self.read_buffer.clear();
        Ok(())
    }

    pub async fn next_event(
        &mut self,
    ) -> Result<Option<StatsEvent>, RlStatsError> {
        loop {
            if let Some(event) = self.try_parse_event_from_buffer()? {
                return Ok(Some(event));
            }

            let bytes_read = self.reader.read_buf(&mut self.read_buffer).await?;
            if bytes_read == 0 {
                return self.try_parse_event_from_buffer();
            }
        }
    }

    fn try_parse_event_from_buffer(
        &mut self,
    ) -> Result<Option<StatsEvent>, RlStatsError> {
        discard_leading_whitespace(&mut self.read_buffer);

        if self.read_buffer.is_empty() {
            return Ok(None);
        }

        let mut stream = serde_json::Deserializer::from_slice(&self.read_buffer)
            .into_iter::<Value>();

        match stream.next() {
            Some(Ok(value)) => {
                let consumed = stream.byte_offset();
                self.read_buffer.drain(0..consumed);
                Ok(Some(parse_stats_event_value(value)?))
            }
            Some(Err(error)) if error.is_eof() => Ok(None),
            Some(Err(error)) => Err(RlStatsError::Json(error)),
            None => Ok(None),
        }
    }

    pub fn into_event_stream(
        self,
    ) -> impl Stream<Item = Result<StatsEvent, RlStatsError>> {
        futures_util::stream::unfold(self, |mut client| async move {
            match client.next_event().await {
                Ok(Some(event)) => Some((Ok(event), client)),
                Ok(None) => None,
                Err(error) => Some((Err(error), client)),
            }
        })
    }

    pub async fn close(mut self) -> Result<(), RlStatsError> {
        self.writer.shutdown().await?;
        Ok(())
    }
}

async fn connect_socket(
    connection: &ConnectionConfig,
) -> Result<(OwnedReadHalf, OwnedWriteHalf), RlStatsError> {
    let stream = TcpStream::connect(connection.socket_address()).await?;
    let (reader, writer) = stream.into_split();

    Ok((reader, writer))
}

fn discard_leading_whitespace(buffer: &mut Vec<u8>) {
    let to_drop = buffer
        .iter()
        .take_while(|byte| byte.is_ascii_whitespace())
        .count();

    if to_drop > 0 {
        buffer.drain(0..to_drop);
    }
}
