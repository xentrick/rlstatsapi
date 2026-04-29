use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use rlstatsapi::{ClientOptions, DEFAULT_PORT, prepare_connection_config};

fn temp_ini_path(test_name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();

    std::env::temp_dir().join(format!(
        "rlstatsapi_{test_name}_{}_{}.ini",
        std::process::id(),
        nanos
    ))
}

#[test]
fn sets_packet_send_rate_when_zero() {
    let path = temp_ini_path("set_rate_when_zero");
    fs::write(&path, "PacketSendRate=0\nPort=49150\n").expect("write test ini");

    let options = ClientOptions {
        stats_api_ini_path: Some(path.clone()),
        ..ClientOptions::default()
    };

    let result =
        prepare_connection_config(&options).expect("prepare connection config");

    assert_eq!(result.port, 49150);
    assert_eq!(result.packet_send_rate, 60.0);
    assert!(result.ini_mutated);

    let updated = fs::read_to_string(&path).expect("read updated ini");
    assert!(updated.contains("PacketSendRate=60"));

    let _ = fs::remove_file(path);
}

#[test]
fn preserves_non_zero_rate_when_policy_only_if_zero() {
    let path = temp_ini_path("preserve_non_zero");
    fs::write(&path, "PacketSendRate=30\nPort=49123\n")
        .expect("write test ini");

    let options = ClientOptions {
        stats_api_ini_path: Some(path.clone()),
        packet_send_rate: 120.0,
        set_packet_rate_only_when_zero: true,
        ..ClientOptions::default()
    };

    let result =
        prepare_connection_config(&options).expect("prepare connection config");

    assert_eq!(result.packet_send_rate, 30.0);
    assert!(!result.ini_mutated);

    let unchanged = fs::read_to_string(&path).expect("read unchanged ini");
    assert!(unchanged.contains("PacketSendRate=30"));
    assert!(!unchanged.contains("PacketSendRate=120"));

    let _ = fs::remove_file(path);
}

#[test]
fn overrides_non_zero_rate_when_policy_allows_override() {
    let path = temp_ini_path("override_non_zero");
    fs::write(&path, "PacketSendRate=30\nPort=49123\n")
        .expect("write test ini");

    let options = ClientOptions {
        stats_api_ini_path: Some(path.clone()),
        packet_send_rate: 120.0,
        set_packet_rate_only_when_zero: false,
        ..ClientOptions::default()
    };

    let result =
        prepare_connection_config(&options).expect("prepare connection config");

    assert_eq!(result.packet_send_rate, 120.0);
    assert!(result.ini_mutated);

    let updated = fs::read_to_string(&path).expect("read updated ini");
    assert!(updated.contains("PacketSendRate=120"));

    let _ = fs::remove_file(path);
}

#[test]
fn adds_default_port_when_missing() {
    let path = temp_ini_path("add_missing_port");
    fs::write(&path, "PacketSendRate=0\n").expect("write test ini");

    let options = ClientOptions {
        stats_api_ini_path: Some(path.clone()),
        ..ClientOptions::default()
    };

    let result =
        prepare_connection_config(&options).expect("prepare connection config");

    assert_eq!(result.port, DEFAULT_PORT);
    assert!(result.ini_mutated);

    let updated = fs::read_to_string(&path).expect("read updated ini");
    assert!(updated.contains("Port=49123"));

    let _ = fs::remove_file(path);
}

#[test]
fn creates_ini_file_when_path_does_not_exist() {
    let path = temp_ini_path("create_missing_ini");

    if path.exists() {
        let _ = fs::remove_file(&path);
    }

    let options = ClientOptions {
        stats_api_ini_path: Some(path.clone()),
        ..ClientOptions::default()
    };

    let result =
        prepare_connection_config(&options).expect("prepare connection config");

    assert!(path.exists());
    assert_eq!(result.port, DEFAULT_PORT);
    assert_eq!(result.packet_send_rate, 60.0);
    assert!(!result.ini_mutated);

    let contents = fs::read_to_string(&path).expect("read created ini");
    assert!(contents.contains("[TAGame.MatchStatsExporter_TA]"));
    assert!(
        contents.contains("; Port the client will listen for connections on")
    );
    assert!(
        contents.contains("; How many times per second the game sends the update state (capped at 120, 0 disables this feature)")
    );
    assert!(contents.contains("PacketSendRate=60"));
    assert!(contents.contains("Port=49123"));

    let _ = fs::remove_file(path);
}

#[test]
fn writes_default_port_when_missing_even_if_auto_enable_off() {
    let path = temp_ini_path("add_port_auto_enable_off");
    fs::write(&path, "PacketSendRate=30\n").expect("write test ini");

    let options = ClientOptions {
        stats_api_ini_path: Some(path.clone()),
        auto_enable_packet_rate: false,
        ..ClientOptions::default()
    };

    let result =
        prepare_connection_config(&options).expect("prepare connection config");

    assert_eq!(result.port, DEFAULT_PORT);
    assert_eq!(result.packet_send_rate, 30.0);
    assert!(result.ini_mutated);

    let updated = fs::read_to_string(&path).expect("read updated ini");
    assert!(updated.contains("Port=49123"));

    let _ = fs::remove_file(path);
}

#[test]
fn uses_localhost_defaults_when_ini_not_provided() {
    let options = ClientOptions {
        stats_api_ini_path: None,
        auto_enable_packet_rate: false,
        ..ClientOptions::default()
    };

    let result =
        prepare_connection_config(&options).expect("prepare connection config");

    assert_eq!(result.host, "127.0.0.1");
    assert_eq!(result.port, DEFAULT_PORT);
    assert_eq!(result.packet_send_rate, 60.0);
    assert!(result.ini_path.is_none());
    assert!(!result.ini_mutated);
}
