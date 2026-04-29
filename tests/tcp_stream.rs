use futures_util::{StreamExt, pin_mut};
use rlstatsapi::{ClientOptions, RocketLeagueStatsClient, StatsEvent};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

async fn spawn_server(events: Vec<&'static str>) -> (u16, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test tcp listener");
    let port = listener
        .local_addr()
        .expect("read listener local addr")
        .port();

    let handle = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept client");
        let mut stream = stream;

        for event in events {
            stream
                .write_all(event.as_bytes())
                .await
                .expect("send event payload");
            stream.write_all(b"\n").await.expect("send event separator");
        }
    });

    (port, handle)
}

fn test_options_for_port(port: u16) -> ClientOptions {
    ClientOptions {
        auto_enable_packet_rate: false,
        host: "127.0.0.1".to_string(),
        port_override: Some(port),
        ..ClientOptions::default()
    }
}

#[tokio::test]
async fn next_event_reads_text_frames_and_ends_on_close() {
    let payload = r#"{"Event":"ClockUpdatedSeconds","Data":{"TimeSeconds":321,"bOvertime":false}}"#;
    let (port, server_handle) = spawn_server(vec![payload]).await;

    let mut client =
        RocketLeagueStatsClient::connect(test_options_for_port(port))
            .await
            .expect("connect client");

    assert_eq!(
        client.connection().socket_address(),
        format!("127.0.0.1:{port}")
    );

    let first = client
        .next_event()
        .await
        .expect("read first event")
        .expect("expected first event");

    match first {
        StatsEvent::ClockUpdatedSeconds(data) => {
            assert_eq!(data.time_seconds, 321);
            assert!(!data.b_overtime);
        }
        other => panic!("unexpected event: {other:?}"),
    }

    let end = client.next_event().await.expect("read close marker");
    assert!(end.is_none());

    server_handle.await.expect("server task join");
}

#[tokio::test]
async fn next_event_reads_concatenated_json_without_newlines() {
    let first = r#"{"Event":"MatchPaused","Data":{"MatchGuid":"M1"}}"#;
    let second = r#"{"Event":"MatchUnpaused","Data":{"MatchGuid":"M1"}}"#;

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test tcp listener");
    let port = listener
        .local_addr()
        .expect("read listener local addr")
        .port();

    let server_handle = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept client");
        let payload = format!("{first}{second}");
        stream
            .write_all(payload.as_bytes())
            .await
            .expect("send concatenated payload");
    });

    let mut client =
        RocketLeagueStatsClient::connect(test_options_for_port(port))
            .await
            .expect("connect client");

    let e1 = client
        .next_event()
        .await
        .expect("read first event")
        .expect("expected first event");
    let e2 = client
        .next_event()
        .await
        .expect("read second event")
        .expect("expected second event");

    assert!(matches!(e1, StatsEvent::MatchPaused(_)));
    assert!(matches!(e2, StatsEvent::MatchUnpaused(_)));

    server_handle.await.expect("server task join");
}

#[tokio::test]
async fn next_event_reads_binary_frames() {
    let payload = r#"{"Event":"MatchEnded","Data":{"WinnerTeamNum":1}}"#;
    let (port, server_handle) = spawn_server(vec![payload]).await;

    let mut client =
        RocketLeagueStatsClient::connect(test_options_for_port(port))
            .await
            .expect("connect client");

    let event = client
        .next_event()
        .await
        .expect("read binary event")
        .expect("expected event");

    match event {
        StatsEvent::MatchEnded(data) => {
            assert_eq!(data.winner_team_num, 1);
        }
        other => panic!("unexpected event: {other:?}"),
    }

    server_handle.await.expect("server task join");
}

#[tokio::test]
async fn into_event_stream_yields_in_order_and_stops() {
    let first = r#"{"Event":"RoundStarted","Data":{"MatchGuid":"M1"}}"#;
    let second = r#"{"Event":"MatchPaused","Data":{"MatchGuid":"M1"}}"#;

    let (port, server_handle) = spawn_server(vec![first, second]).await;

    let client = RocketLeagueStatsClient::connect(test_options_for_port(port))
        .await
        .expect("connect client");

    let stream = client.into_event_stream();
    pin_mut!(stream);

    let e1 = stream
        .next()
        .await
        .expect("first stream item")
        .expect("first event ok");
    let e2 = stream
        .next()
        .await
        .expect("second stream item")
        .expect("second event ok");
    let end = stream.next().await;

    assert!(matches!(e1, StatsEvent::RoundStarted(_)));
    assert!(matches!(e2, StatsEvent::MatchPaused(_)));
    assert!(end.is_none());

    server_handle.await.expect("server task join");
}
