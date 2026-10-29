use rlstatsapi::{
    ClientOptions, Perspective, PovFocus, RlStatsError,
    RocketLeagueStatsClient, StatsCommand,
};
use serde_json::{Value, json};
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;

fn wire(command: &StatsCommand) -> Value {
    command.to_value().expect("command should encode")
}

#[test]
fn encodes_every_command_in_wire_shape() {
    let cases = [
        (
            StatsCommand::ChangePov {
                focus: Some(PovFocus::Player(1)),
                perspective: Some(Perspective::PlayerView),
            },
            json!({"Command": "ChangePOV", "Data": {"Focus": "1", "Perspective": "PlayerView"}}),
        ),
        (
            StatsCommand::ChangePov {
                focus: Some(PovFocus::Ball),
                perspective: Some(Perspective::CameraDirector),
            },
            json!({"Command": "ChangePOV", "Data": {"Focus": "Ball", "Perspective": "Camera_Director"}}),
        ),
        (
            StatsCommand::LoadReplay {
                file_name: Some("Stadium_P_2026-06-05_18-42".to_string()),
                path: None,
            },
            json!({"Command": "LoadReplay", "Data": {"FileName": "Stadium_P_2026-06-05_18-42"}}),
        ),
        (
            StatsCommand::SeekReplay {
                frame: None,
                time_seconds: Some(120.5),
            },
            json!({"Command": "SeekReplay", "Data": {"TimeSeconds": 120.5}}),
        ),
        (
            StatsCommand::SetGameSpeed { speed: 0.5 },
            json!({"Command": "SetGameSpeed", "Data": {"Speed": 0.5}}),
        ),
        (
            StatsCommand::SetHudVisibility { visible: false },
            json!({"Command": "SetHUDVisibility", "Data": {"bVisible": false}}),
        ),
        (
            StatsCommand::SetMatchPaused { paused: true },
            json!({"Command": "SetMatchPaused", "Data": {"bPaused": true}}),
        ),
    ];

    for (command, expected) in cases {
        let encoded = wire(&command);
        assert_eq!(encoded, expected);
        assert_eq!(
            StatsCommand::from_value(&encoded).expect("round trip"),
            command
        );
    }
}

#[test]
fn rejects_invalid_commands_before_sending() {
    let invalid = [
        StatsCommand::ChangePov {
            focus: None,
            perspective: None,
        },
        StatsCommand::LoadReplay {
            file_name: Some(String::new()),
            path: None,
        },
        StatsCommand::SeekReplay {
            frame: None,
            time_seconds: None,
        },
        StatsCommand::SeekReplay {
            frame: None,
            time_seconds: Some(-1.0),
        },
        StatsCommand::SetGameSpeed { speed: f64::NAN },
        StatsCommand::SetGameSpeed { speed: -0.5 },
    ];

    for command in invalid {
        assert!(
            matches!(
                command.to_json(),
                Err(RlStatsError::InvalidCommand { .. })
            ),
            "expected {command:?} to be rejected"
        );
    }
}

#[test]
fn from_value_rejects_bad_wire_payloads() {
    let invalid = [
        json!({"Data": {}}),
        json!({"Command": "SelfDestruct", "Data": {}}),
        json!({"Command": "ChangePOV", "Data": {"Focus": "player1"}}),
        json!({"Command": "ChangePOV", "Data": {"Perspective": "TopDown"}}),
        json!({"Command": "SeekReplay", "Data": {"Frame": -3}}),
        json!({"Command": "SetGameSpeed", "Data": {}}),
        json!({"Command": "SetMatchPaused", "Data": {"bPaused": "yes"}}),
    ];

    for payload in invalid {
        assert!(
            matches!(
                StatsCommand::from_value(&payload),
                Err(RlStatsError::InvalidCommand { .. })
            ),
            "expected {payload} to be rejected"
        );
    }
}

#[tokio::test]
async fn send_command_writes_json_to_socket() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test tcp listener");
    let port = listener.local_addr().expect("local addr").port();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept client");
        let mut received = String::new();
        stream
            .read_to_string(&mut received)
            .await
            .expect("read command");
        received
    });

    let mut client = RocketLeagueStatsClient::connect(ClientOptions {
        auto_enable_packet_rate: false,
        port_override: Some(port),
        ..ClientOptions::default()
    })
    .await
    .expect("connect client");

    client
        .send_command(&StatsCommand::SetMatchPaused { paused: true })
        .await
        .expect("send command");
    client.close().await.expect("close client");

    let received = server.await.expect("server task");
    let parsed: Value =
        serde_json::from_str(&received).expect("server received JSON");
    assert_eq!(
        parsed,
        json!({"Command": "SetMatchPaused", "Data": {"bPaused": true}})
    );
}
