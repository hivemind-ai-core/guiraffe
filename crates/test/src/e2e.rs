//! Throwaway Linux/X11 e2e prototype: newline-delimited JSON-RPC 2.0 on a Unix socket.
//!
//! Explicit configuration only; no environment variables are read here. Selectors must be
//! published by the app during paint: GPUI's debug-selector map is private on real windows.
//! Input uses XTest via xdotool; capture uses ImageMagick. Never enable in production.
use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{BufRead, BufReader, Read, Write},
    os::unix::{
        fs::PermissionsExt,
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const LIMIT: u64 = 64 * 1024;
const TIMEOUT: Duration = Duration::from_secs(3);

/// Logical pixels, window local. The spike requires scale factor 1.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Target {
    pub window: String,
    pub bounds: [f32; 4],
    pub state: Value,
}

#[derive(Default)]
struct Frame {
    targets: BTreeMap<String, Target>,
    painted: Option<Instant>,
}

/// Explicit app-maintained selector mirror, updated from a paint callback.
#[derive(Clone, Default)]
pub struct Registry(Arc<Mutex<Frame>>);
impl Registry {
    /// Replace the complete frame atomically; removed selectors cannot remain stale.
    pub fn publish(&self, targets: BTreeMap<String, Target>) -> Result<()> {
        let mut frame = self.0.lock().map_err(|_| anyhow!("registry poisoned"))?;
        frame.targets = targets;
        frame.painted = Some(Instant::now());
        Ok(())
    }
}

/// One explicitly configured X11 window. Multiple windows need separate adapters for now.
pub struct Config {
    pub socket: PathBuf,
    pub capture_dir: PathBuf,
    /// Exact unique title of a window belonging to this process.
    pub window_title: String,
}

/// Owns the socket and listener thread. Dropping it stops the listener and removes the socket.
pub struct Probe {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    socket: PathBuf,
}
impl Probe {
    pub fn start(config: Config, registry: Registry) -> Result<Self> {
        // Bind fails if anything already occupies the path: never unlink another server.
        let listener = UnixListener::bind(&config.socket)?;
        fs::set_permissions(&config.socket, fs::Permissions::from_mode(0o600))?;
        listener.set_nonblocking(true)?;
        fs::create_dir_all(&config.capture_dir)?;
        let socket = config.socket.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let worker = thread::spawn(move || {
            let mut serial = 0_u64;
            while !stopped.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = stream.set_read_timeout(Some(TIMEOUT));
                        let _ = stream.set_write_timeout(Some(TIMEOUT));
                        let result = serve(&mut stream, |method, params| {
                            serial += 1;
                            dispatch(&config, &registry, serial, method, params)
                        });
                        if let Err(error) = result {
                            eprintln!("e2e connection: {error:#}");
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            stop,
            thread: Some(worker),
            socket,
        })
    }
}
impl Drop for Probe {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.thread.take() {
            let _ = worker.join();
        }
        let _ = fs::remove_file(&self.socket);
    }
}

fn serve(
    stream: &mut UnixStream,
    mut handler: impl FnMut(&str, &Value) -> Result<Value>,
) -> Result<()> {
    let mut line = String::new();
    BufReader::new((&mut *stream).take(LIMIT + 1)).read_line(&mut line)?;
    if line.len() as u64 > LIMIT || !line.ends_with('\n') {
        bail!("oversized or incomplete request");
    }
    let reply = match serde_json::from_str::<Value>(&line) {
        Err(_) => {
            json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Parse error"}})
        }
        Ok(request) => {
            let id = request.get("id").cloned().unwrap_or(Value::Null);
            if request["jsonrpc"] != "2.0" || !request["method"].is_string() || !id.is_u64() {
                json!({"jsonrpc":"2.0","id":null,"error":{"code":-32600,"message":"Invalid request"}})
            } else {
                let method = request["method"].as_str().unwrap_or("");
                if !matches!(
                    method,
                    "windows" | "query" | "click" | "type" | "key" | "wait_idle" | "capture"
                ) {
                    json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Method not found"}})
                } else {
                    match handler(method, &request["params"]) {
                        Ok(result) => json!({"jsonrpc":"2.0","id":id,"result":result}),
                        Err(error) => {
                            json!({"jsonrpc":"2.0","id":id,"error":{"code":-32000,"message":error.to_string()}})
                        }
                    }
                }
            }
        }
    };
    serde_json::to_writer(&mut *stream, &reply)?;
    stream.write_all(b"\n")?;
    Ok(())
}

fn field<'a>(params: &'a Value, key: &str) -> Result<&'a str> {
    params[key]
        .as_str()
        .with_context(|| format!("missing string {key}"))
}

// All subprocesses have a deadline and are reaped. Output is intentionally small (window id).
fn command(program: &str, args: &[&str]) -> Result<String> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let Some(status) = child.try_wait()? {
            if !status.success() {
                bail!("{program} failed: {status}");
            }
            let mut output = String::new();
            if let Some(mut stdout) = child.stdout.take() {
                stdout.read_to_string(&mut output)?;
            }
            return Ok(output.trim().to_owned());
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            bail!("{program} timed out");
        }
        thread::sleep(Duration::from_millis(5));
    }
}
fn window_id(config: &Config) -> Result<String> {
    let pid = std::process::id().to_string();
    // Search by PID first, then compare the exact title rather than interpreting it as a regex.
    let ids = command("xdotool", &["search", "--pid", &pid])?;
    for id in ids.lines() {
        if command("xdotool", &["getwindowname", id])? == config.window_title {
            return Ok(id.into());
        }
    }
    bail!("app window not found")
}
fn dispatch(
    config: &Config,
    registry: &Registry,
    serial: u64,
    method: &str,
    params: &Value,
) -> Result<Value> {
    match method {
        "windows" => Ok(json!([{"id":window_id(config)?,"title":config.window_title}])),
        "query" => {
            let selector = field(params, "selector")?;
            let frame = registry
                .0
                .lock()
                .map_err(|_| anyhow!("registry poisoned"))?;
            Ok(serde_json::to_value(
                frame.targets.get(selector).context("selector not found")?,
            )?)
        }
        "click" => {
            let target: Target =
                serde_json::from_value(dispatch(config, registry, serial, "query", params)?)?;
            let [x, y, w, h] = target.bounds;
            let id = window_id(config)?;
            command(
                "xdotool",
                &[
                    "mousemove",
                    "--window",
                    &id,
                    &(x + w / 2.).round().to_string(),
                    &(y + h / 2.).round().to_string(),
                    "click",
                    "1",
                ],
            )?;
            Ok(Value::Null)
        }
        "type" | "key" => {
            let id = window_id(config)?;
            command("xdotool", &["windowfocus", "--sync", &id])?;
            if method == "type" {
                command(
                    "xdotool",
                    &["type", "--clearmodifiers", "--", field(params, "text")?],
                )?;
            } else {
                command(
                    "xdotool",
                    &["key", "--clearmodifiers", field(params, "key")?],
                )?;
            }
            Ok(Value::Null)
        }
        "wait_idle" => {
            // Heuristic only: require a paint and 100 ms of paint quiet after this request.
            // This is NOT an executor/GPU fence and does not prove async work is complete.
            let start = Instant::now();
            loop {
                let painted = registry
                    .0
                    .lock()
                    .map_err(|_| anyhow!("registry poisoned"))?
                    .painted;
                if start.elapsed() >= Duration::from_millis(100)
                    && painted.is_some_and(|t| t.elapsed() >= Duration::from_millis(100))
                {
                    break;
                }
                if start.elapsed() >= TIMEOUT {
                    bail!("scene did not become quiet");
                }
                thread::sleep(Duration::from_millis(10));
            }
            Ok(json!({"barrier":"paint-quiet-100ms","gpu_fence":false}))
        }
        "capture" => {
            let id = window_id(config)?;
            let path = config.capture_dir.join(format!("frame-{serial}.png"));
            command(
                "import",
                &[
                    "-window",
                    &id,
                    path.to_str().context("non UTF-8 capture directory")?,
                ],
            )?;
            Ok(json!({"path":path}))
        }
        _ => bail!("unknown method: {method}"),
    }
}

/// Tiny synchronous Rust client. Each request uses one bounded connection.
pub struct Client {
    socket: PathBuf,
    next_id: u64,
}
impl Client {
    pub fn new(socket: impl Into<PathBuf>) -> Self {
        Self {
            socket: socket.into(),
            next_id: 0,
        }
    }
    pub fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        self.next_id += 1;
        let mut stream = UnixStream::connect(&self.socket)?;
        stream.set_read_timeout(Some(TIMEOUT + Duration::from_secs(1)))?;
        stream.set_write_timeout(Some(TIMEOUT))?;
        serde_json::to_writer(
            &mut stream,
            &json!({"jsonrpc":"2.0","id":self.next_id,"method":method,"params":params}),
        )?;
        stream.write_all(b"\n")?;
        let mut line = String::new();
        BufReader::new(stream.take(LIMIT + 1)).read_line(&mut line)?;
        if line.len() as u64 > LIMIT || !line.ends_with('\n') {
            bail!("oversized or incomplete response");
        }
        let reply: Value = serde_json::from_str(&line)?;
        if reply["jsonrpc"] != "2.0" || reply["id"] != self.next_id {
            bail!("invalid response identity");
        }
        if let Some(error) = reply.get("error") {
            bail!("RPC: {error}");
        }
        reply.get("result").cloned().context("missing result")
    }
    pub fn socket(&self) -> &Path {
        &self.socket
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn protocol_errors_and_roundtrip() -> Result<()> {
        for (request, code) in [("{\n", -32700), ("{}\n", -32600)] {
            let (mut server, mut client) = UnixStream::pair()?;
            client.write_all(request.as_bytes())?;
            serve(&mut server, |_, _| Ok(Value::Null))?;
            let mut line = String::new();
            BufReader::new(client).read_line(&mut line)?;
            let value: Value = serde_json::from_str(&line)?;
            assert_eq!(value["error"]["code"], code);
        }
        let (mut server, mut client) = UnixStream::pair()?;
        client.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"query\",\"params\":{}}\n")?;
        serve(&mut server, |m, _| {
            assert_eq!(m, "query");
            Ok(json!({"ok":true}))
        })?;
        let mut line = String::new();
        BufReader::new(client).read_line(&mut line)?;
        let value: Value = serde_json::from_str(&line)?;
        assert_eq!(value["id"], 7);
        assert_eq!(value["result"]["ok"], true);
        Ok(())
    }
    #[test]
    fn real_socket_client_and_lifecycle() -> Result<()> {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("guiraffe-probe-{}-{nonce}", std::process::id()));
        fs::create_dir(&root)?;
        let socket = root.join("probe.sock");
        let config = || Config {
            socket: socket.clone(),
            capture_dir: root.clone(),
            window_title: "unused".into(),
        };
        let registry = Registry::default();
        registry.publish(BTreeMap::from([(
            "save".into(),
            Target {
                window: "main".into(),
                bounds: [1., 2., 3., 4.],
                state: json!({"ready":true}),
            },
        )]))?;
        let probe = Probe::start(config(), registry)?;
        assert_eq!(fs::metadata(&socket)?.permissions().mode() & 0o777, 0o600);
        assert!(Probe::start(config(), Registry::default()).is_err());
        let mut client = Client::new(&socket);
        assert_eq!(
            client.call("query", json!({"selector":"save"}))?["state"]["ready"],
            true
        );
        assert!(client.call("unknown", Value::Null).is_err());
        assert!(socket.exists());
        drop(probe);
        assert!(!socket.exists());
        fs::remove_dir(root)?;
        Ok(())
    }
    #[test]
    fn registry_replaces_old_selectors() -> Result<()> {
        let registry = Registry::default();
        registry.publish(BTreeMap::from([(
            "old".into(),
            Target {
                window: "main".into(),
                bounds: [0.; 4],
                state: Value::Null,
            },
        )]))?;
        registry.publish(BTreeMap::new())?;
        assert!(
            registry
                .0
                .lock()
                .map_err(|_| anyhow!("poisoned"))?
                .targets
                .is_empty()
        );
        Ok(())
    }
}
