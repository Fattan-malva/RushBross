use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// Versi MCP terbaru yang didukung RushBross.
pub const PROTOCOL_VERSION: &str = "2025-11-25";

/// Semua versi MCP yang bisa dinegosiasikan. Klien lama (mis. 2024-11-05) tetap diterima.
pub const SUPPORTED_PROTOCOL_VERSIONS: &[&str] =
    &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

/// Batas waktu satu permintaan sebelum RushBross membalas error, supaya CLI tidak menggantung.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

pub const ERR_PARSE: i64 = -32700;
pub const ERR_INVALID_REQUEST: i64 = -32600;
pub const ERR_METHOD_NOT_FOUND: i64 = -32601;
pub const ERR_INVALID_PARAMS: i64 = -32602;
pub const ERR_INTERNAL: i64 = -32603;

#[derive(Debug, Deserialize)]
struct WireRequest {
    token: Option<String>,
    method: String,
    #[serde(default)]
    params: Value,
}

pub struct AgentRequest {
    /// Id asli dari klien (angka atau string), dikembalikan apa adanya.
    pub id: Option<Value>,
    pub method: String,
    pub params: Value,
    pub reply: mpsc::Sender<String>,
}

pub struct AgentServer {
    pub addr: SocketAddr,
    pub token: String,
    shutdown: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl AgentServer {
    pub fn start<F>(token: String, handler: F) -> io::Result<Self>
    where
        F: Fn(AgentRequest) + Send + Sync + 'static,
    {
        Self::start_with_timeout(token, REQUEST_TIMEOUT, handler)
    }

    pub fn start_with_timeout<F>(token: String, timeout: Duration, handler: F) -> io::Result<Self>
    where
        F: Fn(AgentRequest) + Send + Sync + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;
        let shutdown = Arc::new(AtomicBool::new(false));
        let accept_shutdown = shutdown.clone();
        let server_token = token.clone();
        let handler: Arc<dyn Fn(AgentRequest) + Send + Sync> = Arc::new(handler);
        let thread = thread::spawn(move || {
            for stream in listener.incoming() {
                if accept_shutdown.load(Ordering::Relaxed) {
                    break;
                }
                match stream {
                    Ok(stream) => {
                        let handler = handler.clone();
                        let token = server_token.clone();
                        let client_shutdown = accept_shutdown.clone();
                        thread::spawn(move || {
                            serve_client(stream, token, handler, client_shutdown, timeout);
                        });
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            addr,
            token,
            shutdown,
            thread: Some(thread),
        })
    }

    pub fn endpoint(&self) -> String {
        self.addr.to_string()
    }
}

impl Drop for AgentServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect(self.addr);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Negosiasi versi: pakai versi yang diminta klien bila didukung, selain itu versi terbaru.
pub fn negotiate_protocol_version(requested: Option<&str>) -> &'static str {
    requested
        .and_then(|version| {
            SUPPORTED_PROTOCOL_VERSIONS
                .iter()
                .copied()
                .find(|supported| *supported == version)
        })
        .unwrap_or(PROTOCOL_VERSION)
}

/// Balasan JSON-RPC sukses.
pub fn rpc_result(id: Option<Value>, result: Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string()
}

/// Balasan JSON-RPC error protokol (parse error, method tidak dikenal, dst).
pub fn rpc_error(id: Option<Value>, code: i64, message: &str) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
    .to_string()
}

/// Hasil tool sesuai spec MCP. Kegagalan tool dikirim sebagai `isError: true`,
/// bukan sebagai error JSON-RPC, agar agent bisa membacanya sebagai hasil tool.
pub fn tool_result(result: Result<Value, String>) -> Value {
    match result {
        Ok(value) => json!({
            "content": [{ "type": "text", "text": value.to_string() }],
            "isError": false
        }),
        Err(error) => json!({
            "content": [{ "type": "text", "text": error }],
            "isError": true
        }),
    }
}

fn write_line(writer: &Arc<Mutex<BufWriter<TcpStream>>>, message: &str) -> bool {
    let mut writer = writer.lock().unwrap_or_else(|error| error.into_inner());
    writeln!(writer, "{message}").is_ok() && writer.flush().is_ok()
}

fn serve_client(
    stream: TcpStream,
    token: String,
    handler: Arc<dyn Fn(AgentRequest) + Send + Sync>,
    shutdown: Arc<AtomicBool>,
    timeout: Duration,
) {
    let Ok(reader_stream) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(reader_stream);
    let writer = Arc::new(Mutex::new(BufWriter::new(stream)));
    let mut line = String::new();
    loop {
        if shutdown.load(Ordering::Relaxed) {
            break;
        }
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let value: Value = match serde_json::from_str(trimmed) {
            Ok(value) => value,
            Err(_) => {
                write_line(&writer, &rpc_error(None, ERR_PARSE, "Parse error"));
                continue;
            }
        };
        let id = value.get("id").cloned().filter(|id| !id.is_null());
        let request: WireRequest = match serde_json::from_value(value) {
            Ok(request) => request,
            Err(_) => {
                write_line(
                    &writer,
                    &rpc_error(id, ERR_INVALID_REQUEST, "Invalid Request"),
                );
                continue;
            }
        };
        if request.token.as_deref() != Some(token.as_str()) {
            write_line(
                &writer,
                &rpc_error(id, ERR_INVALID_REQUEST, "invalid RushBross token"),
            );
            continue;
        }
        let notification = id.is_none();
        let is_tool_call = request.method == "tools/call";
        let method = request.method;
        let params = request.params;
        let worker_handler = handler.clone();
        let worker_writer = writer.clone();
        let worker_id = id.clone();
        thread::spawn(move || {
            let (reply, response) = mpsc::channel();
            worker_handler(AgentRequest {
                id: worker_id.clone(),
                method,
                params,
                reply,
            });
            if notification {
                return;
            }
            let message = match response.recv_timeout(timeout) {
                Ok(message) => message,
                Err(error) => {
                    let text = if error == mpsc::RecvTimeoutError::Disconnected {
                        "RushBross closed the request before it completed".to_string()
                    } else {
                        format!(
                            "RushBross did not respond within {} seconds",
                            timeout.as_secs()
                        )
                    };
                    if is_tool_call {
                        rpc_result(worker_id, tool_result(Err(text)))
                    } else {
                        rpc_error(worker_id, ERR_INTERNAL, &text)
                    }
                }
            };
            write_line(&worker_writer, &message);
        });
    }
}

pub fn serve_stdio(addr: &str, token: &str) -> io::Result<()> {
    let stream = TcpStream::connect(addr)?;
    let response_stream = stream.try_clone()?;
    // Semua output ke stdout lewat satu jalur, supaya baris JSON tidak saling bercampur.
    let (out_tx, out_rx) = mpsc::channel::<String>();
    let reader_out = out_tx.clone();
    thread::spawn(move || {
        let mut reader = BufReader::new(response_stream);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            if reader_out.send(line.trim_end().to_string()).is_err() {
                break;
            }
        }
    });
    thread::spawn(move || {
        let mut stdout = BufWriter::new(io::stdout());
        for line in out_rx {
            if writeln!(stdout, "{line}").is_err() || stdout.flush().is_err() {
                break;
            }
        }
    });

    let mut writer = BufWriter::new(stream);
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else {
            break;
        };
        if line.trim().is_empty() {
            continue;
        }
        let mut value: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(_) => {
                let _ = out_tx.send(rpc_error(None, ERR_PARSE, "Parse error"));
                continue;
            }
        };
        let id = value.get("id").cloned().filter(|id| !id.is_null());
        let Some(object) = value.as_object_mut() else {
            let _ = out_tx.send(rpc_error(None, ERR_INVALID_REQUEST, "Invalid Request"));
            continue;
        };
        object.insert("token".to_string(), json!(token));
        if writeln!(writer, "{value}")
            .and_then(|_| writer.flush())
            .is_err()
        {
            let _ = out_tx.send(rpc_error(id, ERR_INTERNAL, "RushBross is not connected"));
            break;
        }
    }
    Ok(())
}

/// Nama tabel TOML milik RushBross, misalnya `mcp_servers.rushbross`.
fn toml_is_server_header(trimmed: &str, name: &str) -> bool {
    trimmed == format!("[{name}]") || trimmed.starts_with(&format!("[{name}."))
}

/// Hapus tabel `[name]` dan sub-tabelnya dari teks TOML. `None` bila tidak ada.
pub fn toml_remove_server(text: &str, name: &str) -> Option<String> {
    let mut kept = Vec::new();
    let mut skipping = false;
    let mut found = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            skipping = toml_is_server_header(trimmed, name);
            found |= skipping;
        }
        if !skipping {
            kept.push(line);
        }
    }
    if !found {
        return None;
    }
    let mut out = kept.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    Some(out)
}

/// Tulis ulang tabel `[name]` dengan `body` (pasangan `key = value`, satu per baris).
/// Tabel lain di file tidak disentuh.
pub fn toml_upsert_server(text: &str, name: &str, body: &[(&str, TomlValue)]) -> String {
    let mut out = toml_remove_server(text, name).unwrap_or_else(|| text.to_string());
    if !out.trim().is_empty() && !out.ends_with("\n\n") {
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out.push('\n');
    }
    out.push_str(&format!("[{name}]\n"));
    for (key, value) in body {
        out.push_str(&format!("{key} = {}\n", value.render()));
    }
    out
}

/// Nilai TOML sederhana untuk config MCP.
pub enum TomlValue<'a> {
    Str(&'a str),
    StrArray(&'a [&'a str]),
}

impl TomlValue<'_> {
    fn render(&self) -> String {
        match self {
            TomlValue::Str(text) => toml_string(text),
            TomlValue::StrArray(items) => format!(
                "[{}]",
                items
                    .iter()
                    .map(|item| toml_string(item))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
}

fn toml_string(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ping_via(server: &AgentServer, line: &str) -> Value {
        let mut stream = TcpStream::connect(server.addr).expect("connect");
        writeln!(stream, "{line}").expect("write");
        let mut response = String::new();
        BufReader::new(stream)
            .read_line(&mut response)
            .expect("read");
        serde_json::from_str(response.trim()).expect("json")
    }

    #[test]
    fn server_answers_authorized_request() {
        let server = AgentServer::start("token".to_string(), |request| {
            let response = rpc_result(request.id, json!({ "method": request.method }));
            let _ = request.reply.send(response);
        })
        .expect("server");
        let value = ping_via(
            &server,
            r#"{"token":"token","id":7,"method":"ping","params":{}}"#,
        );
        assert_eq!(value["id"], 7);
        assert_eq!(value["result"]["method"], "ping");
    }

    #[test]
    fn server_keeps_string_ids() {
        let server = AgentServer::start("token".to_string(), |request| {
            let _ = request.reply.send(rpc_result(request.id, json!({})));
        })
        .expect("server");
        let value = ping_via(&server, r#"{"token":"token","id":"abc-1","method":"ping"}"#);
        assert_eq!(value["id"], "abc-1");
    }

    #[test]
    fn server_rejects_invalid_token() {
        let server = AgentServer::start("token".to_string(), |_| {}).expect("server");
        let value = ping_via(
            &server,
            r#"{"token":"wrong","id":3,"method":"ping","params":{}}"#,
        );
        assert_eq!(value["error"]["code"], ERR_INVALID_REQUEST);
    }

    #[test]
    fn server_reports_parse_error_with_standard_code() {
        let server = AgentServer::start("token".to_string(), |_| {}).expect("server");
        let value = ping_via(&server, "{not json");
        assert_eq!(value["error"]["code"], ERR_PARSE);
        assert!(value["id"].is_null());
    }

    #[test]
    fn slow_tool_call_gets_tool_error_instead_of_hanging() {
        let server = AgentServer::start_with_timeout(
            "token".to_string(),
            Duration::from_millis(100),
            |request| {
                // Tidak membalas: simulasi WebView yang tidak merespons. Sender ditahan
                // di thread lain supaya channel tidak tertutup lebih dulu.
                thread::spawn(move || {
                    let _hold = request.reply;
                    thread::sleep(Duration::from_millis(500));
                });
            },
        )
        .expect("server");
        let value = ping_via(
            &server,
            r#"{"token":"token","id":9,"method":"tools/call","params":{"name":"browser_state"}}"#,
        );
        assert_eq!(value["id"], 9);
        assert_eq!(value["result"]["isError"], true);
    }

    #[test]
    fn slow_request_does_not_block_the_next_request() {
        let server = AgentServer::start_with_timeout(
            "token".to_string(),
            Duration::from_secs(2),
            |request| {
                let id = request
                    .id
                    .as_ref()
                    .and_then(Value::as_u64)
                    .unwrap_or_default();
                if id == 1 {
                    thread::spawn(move || {
                        let _hold = request.reply;
                        thread::sleep(Duration::from_millis(500));
                    });
                } else {
                    let _ = request.reply.send(rpc_result(request.id, json!({})));
                }
            },
        )
        .expect("server");
        let mut stream = TcpStream::connect(server.addr).expect("connect");
        let first = r#"{"token":"token","id":1,"method":"tools/call","params":{}}"#.to_string();
        let second = r#"{"token":"token","id":2,"method":"ping","params":{}}"#.to_string();
        writeln!(stream, "{first}").expect("write first");
        writeln!(stream, "{second}").expect("write second");
        let mut reader = BufReader::new(stream);
        let mut first = String::new();
        reader.read_line(&mut first).expect("read first");
        let first: Value = serde_json::from_str(first.trim()).expect("first json");
        assert_eq!(first["id"], 2, "fast request should be answered first");
        let mut second = String::new();
        reader.read_line(&mut second).expect("read second");
        let second: Value = serde_json::from_str(second.trim()).expect("second json");
        assert_eq!(second["id"], 1);
    }

    #[test]
    fn negotiates_supported_version_and_falls_back_to_latest() {
        assert_eq!(negotiate_protocol_version(Some("2024-11-05")), "2024-11-05");
        assert_eq!(negotiate_protocol_version(Some("2025-06-18")), "2025-06-18");
        assert_eq!(
            negotiate_protocol_version(Some("1999-01-01")),
            PROTOCOL_VERSION
        );
        assert_eq!(negotiate_protocol_version(None), PROTOCOL_VERSION);
    }

    #[test]
    fn tool_errors_are_results_with_is_error() {
        let value = tool_result(Err("url is required".to_string()));
        assert_eq!(value["isError"], true);
        assert_eq!(value["content"][0]["text"], "url is required");
    }

    #[test]
    fn toml_upsert_is_idempotent_and_keeps_other_tables() {
        let body = [
            ("command", TomlValue::Str(r"C:\RushBross\rushbross-mcp.exe")),
            ("args", TomlValue::StrArray(&[])),
        ];
        let existing = "model = \"x\"\n\n[mcp_servers.other]\ncommand = \"o\"\n\n[mcp_servers.rushbross.env]\nA = \"1\"\n";
        let once = toml_upsert_server(existing, "mcp_servers.rushbross", &body);
        let twice = toml_upsert_server(&once, "mcp_servers.rushbross", &body);
        assert_eq!(once, twice);
        assert!(once.contains("model = \"x\""));
        assert!(once.contains("[mcp_servers.other]"));
        assert!(!once.contains("[mcp_servers.rushbross.env]"));
        assert!(once.contains(r#"command = "C:\\RushBross\\rushbross-mcp.exe""#));
        assert!(once.contains("args = []"));
    }

    #[test]
    fn toml_remove_leaves_other_content_intact() {
        let text = "[mcp_servers.rushbross]\ncommand = \"x\"\n\n[other]\nk = 1\n";
        let removed = toml_remove_server(text, "mcp_servers.rushbross").expect("found");
        assert_eq!(removed, "[other]\nk = 1\n");
        assert!(toml_remove_server("[other]\nk = 1\n", "mcp_servers.rushbross").is_none());
    }
}
