// FARO QEMU MANAGER (C) R.Trunk 2026
// Speaking QMP to a running QEMU: one JSON object per line, over TCP.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

/// What QEMU says about itself when a client connects.
///
/// This is the identity proof from spec section 7.3. A process ID is not one:
/// the operating system reuses them, so a recorded ID can belong to a
/// stranger by the time the application looks again. A greeting in this shape
/// on the recorded port can only have come from a QEMU.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Greeting {
    pub major: u32,
    pub minor: u32,
    pub micro: u32,
    pub package: String,
}

/// A QMP connection that has completed its handshake.
///
/// There is no way to hold one of these without having greeted the server and
/// had `qmp_capabilities` accepted: `handshake` is the only constructor, and
/// it returns an error for every other outcome. A half-open connection must
/// never pass for a confirmed machine.
pub struct QmpClient<T: Read + Write> {
    reader: BufReader<T>,
    written: String,
    events: Vec<String>,
}

impl<T: Read + Write> QmpClient<T> {
    /// Greet the server and enable commands.
    ///
    /// Reads the greeting line, sends `qmp_capabilities`, reads the reply, and
    /// only then hands out a client. Silence, rubbish, or a refused
    /// capabilities exchange are all errors.
    pub fn handshake(transport: T) -> Result<(Self, Greeting), String> {
        let mut client = Self {
            reader: BufReader::new(transport),
            written: String::new(),
            events: Vec::new(),
        };

        let line = client.read_line()?;
        let greeting = parse_greeting(&line)?;

        // Nothing else is accepted before this, so it is not optional.
        let reply = client.send_and_read("qmp_capabilities")?;
        if reply.get("return").is_none() {
            return Err(format!("qmp_capabilities refused: {}", describe(&reply)));
        }

        Ok((client, greeting))
    }

    /// Run a command and return what it returned.
    pub fn execute(&mut self, command: &str) -> Result<serde_json::Value, String> {
        let reply = self.send_and_read(command)?;
        match reply.get("return") {
            Some(value) => Ok(value.clone()),
            None => Err(describe(&reply)),
        }
    }

    /// Press the guest's power button.
    ///
    /// The guest decides what to do with it: Windows and Linux shut down the
    /// way they would on a desk. That takes seconds to minutes, and a hung
    /// guest may never finish — which is why the caller watches the clock and
    /// the interface offers the hard way beside the waiting.
    pub fn powerdown(&mut self) -> Result<(), String> {
        self.execute("system_powerdown").map(|_| ())
    }

    /// Who this machine says it is.
    ///
    /// The machine UUID, which the application sets to the machine's own id
    /// at start. A handshake proves only that *a* QEMU holds the recorded
    /// port; loopback ports are handed out again, so the one answering may
    /// be a different machine of ours. This answers which.
    pub fn machine_uuid(&mut self) -> Result<String, String> {
        let reply = self.execute("query-uuid")?;
        reply
            .get("UUID")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| format!("no UUID in the answer: {}", reply))
    }

    /// End the machine now, through its own monitor.
    ///
    /// The hard way for a machine this application did not start: it has no
    /// process handle for one, and a recorded process number is not an
    /// identity. Going through the monitor means the thing that dies is the
    /// thing that just proved who it was.
    pub fn quit(&mut self) -> Result<(), String> {
        self.execute("quit").map(|_| ())
    }

    /// Everything sent to the server so far. For tests.
    pub fn wrote(&self) -> &str {
        &self.written
    }

    /// The events the server reported along the way, and forget them.
    ///
    /// They are not replies, but they are the machine's own account of what
    /// it did — `POWERDOWN` is the proof that the request arrived. Dropping
    /// them would leave the console silent at the one moment the user is
    /// watching it.
    pub fn take_events(&mut self) -> Vec<String> {
        std::mem::take(&mut self.events)
    }

    fn send_and_read(&mut self, command: &str) -> Result<serde_json::Value, String> {
        let request = format!("{{\"execute\":\"{}\"}}\n", command);
        self.reader
            .get_mut()
            .write_all(request.as_bytes())
            .map_err(|e| format!("{}: {}", command, e))?;
        self.reader
            .get_mut()
            .flush()
            .map_err(|e| format!("{}: {}", command, e))?;
        self.written.push_str(&request);

        // QEMU interleaves events with replies, and `system_powerdown`
        // answers with a POWERDOWN event before its return. An event is not
        // this command's answer, so keep reading.
        loop {
            let line = self.read_line()?;
            let value: serde_json::Value = serde_json::from_str(&line)
                .map_err(|e| format!("{}: {} in {:?}", command, e, line))?;
            if let Some(event) = value.get("event").and_then(|e| e.as_str()) {
                self.events.push(event.to_string());
                continue;
            }
            if value.get("event").is_some() {
                continue;
            }
            return Ok(value);
        }
    }

    fn read_line(&mut self) -> Result<String, String> {
        let mut line = String::new();
        match self.reader.read_line(&mut line) {
            Ok(0) => Err("the connection said nothing".to_string()),
            Ok(_) if line.trim().is_empty() => Err("an empty line is not a message".to_string()),
            Ok(_) => Ok(line.trim().to_string()),
            Err(e) => Err(e.to_string()),
        }
    }
}

/// Read a greeting, or say why this is not one.
///
/// Strict on purpose: a foreign program holding the recorded port — a web
/// server, another tool, anything — must not be mistaken for the machine's
/// QEMU, because the application would then offer to kill it.
fn parse_greeting(line: &str) -> Result<Greeting, String> {
    let value: serde_json::Value =
        serde_json::from_str(line).map_err(|e| format!("not a QMP greeting: {}", e))?;
    let version = value
        .get("QMP")
        .and_then(|qmp| qmp.get("version"))
        .ok_or_else(|| format!("not a QMP greeting: {}", line))?;
    let qemu = version
        .get("qemu")
        .ok_or_else(|| format!("a QMP greeting without a version: {}", line))?;

    let number = |key: &str| -> Result<u32, String> {
        qemu.get(key)
            .and_then(|v| v.as_u64())
            .map(|v| v as u32)
            .ok_or_else(|| format!("a QMP version without {}: {}", key, line))
    };

    Ok(Greeting {
        major: number("major")?,
        minor: number("minor")?,
        micro: number("micro")?,
        package: version
            .get("package")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
    })
}

/// The server's own words for a failure, so they reach the user intact.
fn describe(reply: &serde_json::Value) -> String {
    reply
        .get("error")
        .and_then(|e| e.get("desc"))
        .and_then(|d| d.as_str())
        .map(|d| d.to_string())
        .unwrap_or_else(|| reply.to_string())
}

/// Connect to a QMP server on the loopback interface and complete the
/// handshake.
///
/// Deadlines on both directions: a silent peer must not hold the caller for
/// ever, and the caller here is sometimes the application's start-up.
pub fn connect_tcp(
    port: u16,
    timeout: Duration,
) -> Result<(QmpClient<TcpStream>, Greeting), String> {
    let address = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let stream = TcpStream::connect_timeout(&address, timeout)
        .map_err(|e| format!("127.0.0.1:{}: {}", port, e))?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|e| e.to_string())?;
    QmpClient::handshake(stream)
}

/// A loopback port nothing is using.
///
/// Asked for, never assumed. On this host binding 45555-45561 and 47000
/// fails while 12345, 52400 and 55000 succeed, and Windows reserves whole
/// ranges besides — so a fixed port would work on one machine and not the
/// next. Binding `:0` lets the operating system pick one it knows is free;
/// the listener is dropped immediately so QEMU can take it.
pub fn free_local_port() -> Result<u16, String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|e| format!("could not find a free port: {}", e))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("could not read the chosen port: {}", e))?
        .port();
    drop(listener);
    Ok(port)
}

/// The arguments that put a QMP server on this port.
///
/// `server=on,wait=off` means QEMU listens but does not wait for a client, so
/// a machine starts whether or not anything is watching. A named Windows pipe
/// was tried and rejected: QEMU accepts exactly one client on it, and the
/// reattach needs a second one.
pub fn qmp_args(port: u16) -> Vec<String> {
    vec![
        "-qmp".to_string(),
        format!("tcp:127.0.0.1:{},server=on,wait=off", port),
    ]
}

/// The arguments that stamp a machine with its own identity.
///
/// `-uuid` sets what the guest sees as its SMBIOS UUID and what
/// `query-uuid` reports back. The application passes the machine's own id,
/// which turns the reattach from "some QEMU is on that port" into "this
/// machine is on that port".
pub fn uuid_args(id: &str) -> Vec<String> {
    vec!["-uuid".to_string(), id.to_string()]
}
