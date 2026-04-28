use rlstatsapi::{ClientOptions, RocketLeagueStatsClient, StatsEvent};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio::time::{Duration, sleep};

async fn reserve_free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let port = listener
        .local_addr()
        .expect("read listener address")
        .port();
    drop(listener);
    port
}

async fn spawn_server_on_port(
    port: u16,
    events: Vec<&'static str>,
) -> JoinHandle<()> {
    let listener = TcpListener::bind(("127.0.0.1", port))
        .await
        .expect("bind server port");

    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept client");
        let mut stream = stream;

        for event in events {
            stream
                .write_all(event.as_bytes())
                .await
                .expect("send event payload");
            stream
                .write_all(b"\n")
                .await
                .expect("send event separator");
        }
    })
}

fn options_for(port: u16) -> ClientOptions {
    ClientOptions {
        host: "127.0.0.1".to_string(),
        port_override: Some(port),
        auto_enable_packet_rate: false,
        ..ClientOptions::default()
    }
}

#[tokio::test]
async fn connect_with_retry_succeeds_after_server_starts() {
    let port = reserve_free_port().await;

    let delayed_server = tokio::spawn(async move {
        sleep(Duration::from_millis(200)).await;
        let events = vec![r#"{"Event":"RoundStarted","Data":{"MatchGuid":"R1"}}"#];
        let server = spawn_server_on_port(port, events).await;
        server.await.expect("server join");
    });

    let mut client = RocketLeagueStatsClient::connect_with_retry(
        options_for(port),
        6,
        Duration::from_millis(75),
    )
    .await
    .expect("connect with retry");

    let event = client
        .next_event()
        .await
        .expect("read event")
        .expect("expected event");

    assert!(matches!(event, StatsEvent::RoundStarted(_)));

    delayed_server.await.expect("delayed server join");
}

#[tokio::test]
async fn reconnect_after_disconnect_receives_new_event() {
    let port = reserve_free_port().await;

    let first_server = spawn_server_on_port(
        port,
        vec![r#"{"Event":"MatchPaused","Data":{"MatchGuid":"M1"}}"#],
    )
    .await;

    let mut client = RocketLeagueStatsClient::connect(options_for(port))
        .await
        .expect("connect client");

    let first_event = client
        .next_event()
        .await
        .expect("read first event")
        .expect("expected first event");
    assert!(matches!(first_event, StatsEvent::MatchPaused(_)));

    let end = client.next_event().await.expect("read close marker");
    assert!(end.is_none());

    first_server.await.expect("first server join");

    let second_server = spawn_server_on_port(
        port,
        vec![r#"{"Event":"MatchUnpaused","Data":{"MatchGuid":"M1"}}"#],
    )
    .await;

    client.reconnect().await.expect("reconnect client");

    let second_event = client
        .next_event()
        .await
        .expect("read second event")
        .expect("expected second event");
    assert!(matches!(second_event, StatsEvent::MatchUnpaused(_)));

    second_server.await.expect("second server join");
}
