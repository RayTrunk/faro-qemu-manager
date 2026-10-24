// FARO QEMU MANAGER (C) R.Trunk 2026
// Speaking QMP: the greeting, the handshake, commands, and a free port.

mod common;

use std::io::{Cursor, Read, Write};
use std::time::Duration;

use faro_qem::config::Arch;
use faro_qem::qemu::find_qemu_binary;
use faro_qem::qmp::{connect_tcp, free_local_port, qmp_args, QmpClient};

/// A transport that replays canned lines and records what was written.
///
/// The protocol's failure paths — silence, rubbish, a refused handshake — are
/// things a transport can be told to do and a live QEMU cannot.
struct Wire {
    to_read: Cursor<Vec<u8>>,
    written: Vec<u8>,
}

impl Wire {
    fn replaying(lines: &[&str]) -> Self {
        let mut body = String::new();
        for line in lines {
            body.push_str(line);
            body.push('\n');
        }
        Self {
            to_read: Cursor::new(body.into_bytes()),
            written: Vec::new(),
        }
    }
}

impl Read for Wire {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.to_read.read(buf)
    }
}

impl Write for Wire {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.written.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

const GREETING: &str = r#"{"QMP": {"version": {"qemu": {"micro": 0, "minor": 1, "major": 11}, "package": "v11.1.0-12130-ge470268ff4"}, "capabilities": ["oob"]}}"#;

#[test]
fn test_the_handshake_reads_the_greeting_and_sends_capabilities() {
    let wire = Wire::replaying(&[GREETING, r#"{"return": {}}"#]);
    let (client, greeting) = QmpClient::handshake(wire).unwrap();
    assert_eq!((greeting.major, greeting.minor, greeting.micro), (11, 1, 0));
    assert!(greeting.package.starts_with("v11.1.0"));
    assert!(
        client.wrote().contains("qmp_capabilities"),
        "no command is accepted before capabilities, got: {}",
        client.wrote()
    );
}

#[test]
fn test_a_transport_that_says_nothing_is_not_a_confirmed_vm() {
    assert!(
        QmpClient::handshake(Wire::replaying(&[])).is_err(),
        "silence must not pass for a handshake"
    );
}

#[test]
fn test_rubbish_instead_of_a_greeting_is_refused() {
    // A foreign program holding the port must not be mistaken for QEMU.
    // This is Review Focus 1 at the protocol level.
    for junk in ["HTTP/1.1 200 OK", "{}", "{\"QMP\": {}}", "not json at all"] {
        assert!(
            QmpClient::handshake(Wire::replaying(&[junk, r#"{"return": {}}"#])).is_err(),
            "accepted {:?} as a QEMU greeting",
            junk
        );
    }
}

#[test]
fn test_capabilities_refused_means_no_client() {
    let wire = Wire::replaying(&[
        GREETING,
        r#"{"error": {"class": "GenericError", "desc": "no"}}"#,
    ]);
    assert!(QmpClient::handshake(wire).is_err());
}

#[test]
fn test_a_command_returns_its_value_and_an_error_is_an_error() {
    let wire = Wire::replaying(&[
        GREETING,
        r#"{"return": {}}"#,
        r#"{"return": {"status": "running", "running": true}}"#,
        r#"{"error": {"class": "CommandNotFound", "desc": "no such command"}}"#,
    ]);
    let (mut client, _) = QmpClient::handshake(wire).unwrap();
    assert_eq!(client.execute("query-status").unwrap()["status"], "running");
    let err = client.execute("nonsense").unwrap_err();
    assert!(
        err.contains("no such command"),
        "pass the server's words along, got: {}",
        err
    );
}

#[test]
fn test_an_event_line_is_not_mistaken_for_a_reply() {
    // QEMU interleaves events with replies; powerdown answers with an event.
    let wire = Wire::replaying(&[
        GREETING,
        r#"{"return": {}}"#,
        r#"{"timestamp": {"seconds": 1, "microseconds": 2}, "event": "POWERDOWN"}"#,
        r#"{"return": {}}"#,
    ]);
    let (mut client, _) = QmpClient::handshake(wire).unwrap();
    assert!(client.powerdown().is_ok());
}

#[test]
fn test_a_free_port_is_actually_free() {
    let port = free_local_port().unwrap();
    assert!(port >= 1024);
    // Binding it right after must succeed: the probe has to let go.
    std::net::TcpListener::bind(("127.0.0.1", port)).expect("the discovered port must be usable");
}

#[test]
fn test_a_port_already_in_use_is_not_offered() {
    let a = free_local_port().unwrap();
    let _hold = std::net::TcpListener::bind(("127.0.0.1", a)).unwrap();
    assert_ne!(free_local_port().unwrap(), a);
}

#[test]
fn test_the_qmp_arguments_are_the_form_qemu_accepts() {
    assert_eq!(
        qmp_args(12345),
        vec![
            "-qmp".to_string(),
            "tcp:127.0.0.1:12345,server=on,wait=off".to_string()
        ]
    );
}

#[test]
fn test_a_real_qemu_answers_three_handshakes_in_a_row() {
    // The reattach of Task 6 stands or falls on this: QEMU's QMP server must
    // take a new client after the previous one is gone.
    let Some(bin) = find_qemu_binary(None, Arch::X86_64) else {
        eprintln!("skipped: no qemu-system-x86_64");
        return;
    };
    let port = free_local_port().unwrap();
    let mut child = common::spawn_paused_qemu(&bin, &qmp_args(port));

    for round in 1..=3 {
        let (mut client, greeting) = connect_tcp(port, Duration::from_secs(5))
            .unwrap_or_else(|e| panic!("round {}: {}", round, e));
        assert_eq!(greeting.major, 11, "round {}", round);
        assert_eq!(
            client.execute("query-status").unwrap()["running"],
            false,
            "round {}",
            round
        );
        drop(client);
    }

    let (mut client, _) = connect_tcp(port, Duration::from_secs(5)).unwrap();
    client.powerdown().unwrap();
    drop(client);
    let _ = child.kill();
    let _ = child.wait();
}
