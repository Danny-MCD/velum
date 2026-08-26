//! Everything to do with running the bundled Tor binary and talking to its
//! control port. Deliberately kept to plain blocking I/O for the control
//! protocol (it's a handful of short request/response round trips) - only
//! the process itself and its stdout are handled asynchronously.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;

use crate::ports::free_local_port;

/// Current lifecycle state of the bundled Tor process, mirrored to the
/// frontend via the `tor://status` event so the UI can show a live indicator.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum TorStatus {
    Stopped,
    Starting,
    Bootstrapping { percent: u8 },
    Running,
    Failed { message: String },
}

pub struct TorManager {
    app: AppHandle,
    status: Arc<Mutex<TorStatus>>,
    running: Mutex<Option<RunningTor>>,
}

struct RunningTor {
    #[allow(dead_code)] // kept alive so the process isn't dropped/killed
    child: CommandChild,
    control_port: u16,
    data_dir: PathBuf,
}

impl TorManager {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            status: Arc::new(Mutex::new(TorStatus::Stopped)),
            running: Mutex::new(None),
        }
    }

    pub fn status(&self) -> TorStatus {
        self.status.lock().unwrap().clone()
    }

    fn set_status(&self, status: TorStatus) {
        *self.status.lock().unwrap() = status.clone();
        let _ = self.app.emit("tor://status", status);
    }

    /// Launch the bundled `tor` sidecar with a fresh data directory, wait for
    /// its control port + auth cookie to appear, and start tracking bootstrap
    /// progress from its stdout.
    pub fn start(&self) -> anyhow::Result<()> {
        if self.running.lock().unwrap().is_some() {
            return Ok(()); // already running
        }
        self.set_status(TorStatus::Starting);

        let data_dir = self
            .app
            .path()
            .app_data_dir()?
            .join("tor-data");
        std::fs::create_dir_all(&data_dir)?;

        let control_port = free_local_port()?;

        let (mut rx, child) = self
            .app
            .shell()
            .sidecar("tor")?
            .args([
                "--ControlPort".into(),
                format!("127.0.0.1:{control_port}"),
                "--CookieAuthentication".into(),
                "1".into(),
                "--SocksPort".into(),
                "0".into(),
                "--DataDirectory".into(),
                data_dir.to_string_lossy().to_string(),
                "--Log".into(),
                "notice stdout".into(),
                "--ignore-missing-torrc".into(),
            ])
            .spawn()?;

        *self.running.lock().unwrap() = Some(RunningTor {
            child,
            control_port,
            data_dir: data_dir.clone(),
        });

        let status = self.status.clone();
        let app = self.app.clone();
        tauri::async_runtime::spawn(async move {
            while let Some(event) = rx.recv().await {
                if let CommandEvent::Stdout(line) | CommandEvent::Stderr(line) = &event {
                    let text = String::from_utf8_lossy(line);
                    if let Some(percent) = parse_bootstrap_percent(&text) {
                        let s = if percent >= 100 {
                            TorStatus::Running
                        } else {
                            TorStatus::Bootstrapping { percent }
                        };
                        *status.lock().unwrap() = s.clone();
                        let _ = app.emit("tor://status", s);
                    }
                }
                if let CommandEvent::Error(err) = &event {
                    let s = TorStatus::Failed {
                        message: err.clone(),
                    };
                    *status.lock().unwrap() = s.clone();
                    let _ = app.emit("tor://status", s);
                }
                if let CommandEvent::Terminated(_) = &event {
                    let s = TorStatus::Stopped;
                    *status.lock().unwrap() = s.clone();
                    let _ = app.emit("tor://status", s);
                }
            }
        });

        Ok(())
    }

    pub fn stop(&self) {
        if let Some(running) = self.running.lock().unwrap().take() {
            let _ = running.child.kill();
        }
        self.set_status(TorStatus::Stopped);
    }

    /// Open a fresh authenticated control connection. Cheap enough to do
    /// per-request given how infrequent onion create/delete calls are.
    fn connect(&self) -> anyhow::Result<ControlClient> {
        let running = self.running.lock().unwrap();
        let running = running
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Tor is not running"))?;

        let cookie_hex = read_auth_cookie(&running.data_dir)?;
        let mut client = ControlClient::connect(running.control_port)?;
        client.authenticate(&cookie_hex)?;
        Ok(client)
    }

    /// Publish (or re-publish) an onion service. Pass `existing_key` to keep
    /// the same `.onion` address across restarts.
    pub fn add_onion(
        &self,
        existing_key: Option<&str>,
        local_port: u16,
    ) -> anyhow::Result<OnionInfo> {
        self.connect()?.add_onion(existing_key, local_port)
    }

    pub fn del_onion(&self, service_id: &str) -> anyhow::Result<()> {
        self.connect()?.del_onion(service_id)
    }
}

pub struct OnionInfo {
    pub address: String,
    pub private_key: String,
}

fn read_auth_cookie(data_dir: &PathBuf) -> anyhow::Result<String> {
    let cookie_path = data_dir.join("control_auth_cookie");
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut bytes = Vec::new();
    loop {
        if cookie_path.exists() {
            let mut f = std::fs::File::open(&cookie_path)?;
            bytes.clear();
            f.read_to_end(&mut bytes)?;
            if bytes.len() == 32 {
                break;
            }
        }
        if Instant::now() > deadline {
            anyhow::bail!("Timed out waiting for Tor's control auth cookie");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

fn parse_bootstrap_percent(line: &str) -> Option<u8> {
    let idx = line.find("Bootstrapped ")?;
    let rest = &line[idx + "Bootstrapped ".len()..];
    let pct_str: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    pct_str.parse::<u8>().ok()
}

/// A minimal client for Tor's line-based control protocol.
/// See: https://spec.torproject.org/control-spec/
struct ControlClient {
    stream: TcpStream,
}

impl ControlClient {
    fn connect(port: u16) -> anyhow::Result<Self> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match TcpStream::connect(("127.0.0.1", port)) {
                Ok(stream) => return Ok(Self { stream }),
                Err(e) => {
                    if Instant::now() > deadline {
                        return Err(e.into());
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        }
    }

    fn authenticate(&mut self, cookie_hex: &str) -> anyhow::Result<()> {
        let reply = self.command(&format!("AUTHENTICATE {cookie_hex}"))?;
        reply.expect_ok()
    }

    fn add_onion(&mut self, existing_key: Option<&str>, local_port: u16) -> anyhow::Result<OnionInfo> {
        let key_arg = existing_key
            .map(|k| k.to_string())
            .unwrap_or_else(|| "NEW:ED25519-V3".to_string());
        let cmd = format!("ADD_ONION {key_arg} Port=80,127.0.0.1:{local_port}");
        let reply = self.command(&cmd)?;
        reply.expect_ok()?;

        let service_id = reply
            .field("ServiceID")
            .ok_or_else(|| anyhow::anyhow!("Tor didn't return a ServiceID"))?;
        let private_key = reply
            .field("PrivateKey")
            .unwrap_or_else(|| existing_key.unwrap_or_default().to_string());

        Ok(OnionInfo {
            address: format!("{service_id}.onion"),
            private_key,
        })
    }

    fn del_onion(&mut self, service_id: &str) -> anyhow::Result<()> {
        let service_id = service_id.trim_end_matches(".onion");
        let reply = self.command(&format!("DEL_ONION {service_id}"))?;
        reply.expect_ok()
    }

    fn command(&mut self, cmd: &str) -> anyhow::Result<ControlReply> {
        self.stream.write_all(cmd.as_bytes())?;
        self.stream.write_all(b"\r\n")?;

        let mut reader = BufReader::new(self.stream.try_clone()?);
        let mut lines = Vec::new();
        loop {
            let mut line = String::new();
            let n = reader.read_line(&mut line)?;
            if n == 0 {
                break;
            }
            let line = line.trim_end_matches(['\r', '\n']).to_string();
            let is_final = line.len() >= 4 && line.as_bytes()[3] == b' ';
            lines.push(line);
            if is_final {
                break;
            }
        }
        Ok(ControlReply { lines })
    }
}

struct ControlReply {
    lines: Vec<String>,
}

impl ControlReply {
    fn expect_ok(&self) -> anyhow::Result<()> {
        match self.lines.last() {
            Some(last) if last.starts_with("250") => Ok(()),
            Some(last) => anyhow::bail!("Tor control error: {last}"),
            None => anyhow::bail!("Empty reply from Tor control port"),
        }
    }

    /// Looks for a `Key=Value` pair inside any `250-Key=Value` line.
    fn field(&self, key: &str) -> Option<String> {
        let prefix = format!("{key}=");
        for line in &self.lines {
            if let Some(pos) = line.find(&prefix) {
                return Some(line[pos + prefix.len()..].to_string());
            }
        }
        None
    }
}
