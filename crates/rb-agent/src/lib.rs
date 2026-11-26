use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::{self, JoinHandle};

pub const PROTOCOL_VERSION: &str = "2025-11-25";

#[derive(Debug, Deserialize)]
struct WireRequest {
    token: Option<String>,
    id: Option<u64>,
    method: String,
    #[serde(default)]
    params: Value,
}

pub struct AgentRequest {
    pub id: Option<u64>,
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
                            serve_client(stream, token, handler, client_shutdown);
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

fn error_response(id: Option<u64>, message: &str) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": -32603, "message": message }
    })
    .to_string()
}

fn serve_client(
    stream: TcpStream,
    token: String,
    handler: Arc<dyn Fn(AgentRequest) + Send + Sync>,
    shutdown: Arc<AtomicBool>,
) {
    let Ok(reader_stream) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(reader_stream);
    let mut writer = BufWriter::new(stream);
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
        let request: WireRequest = match serde_json::from_str(trimmed) {
            Ok(request) => request,
            Err(_) => {
                let _ = writeln!(writer, "{}", error_response(None, "invalid JSON"));
                let _ = writer.flush();
                continue;
            }
        };
        if request.token.as_deref() != Some(token.as_str()) {
            let _ = writeln!(
                writer,
                "{}",
                error_response(request.id, "invalid RushBross token")
            );
            let _ = writer.flush();
            continue;
        }
        let notification = request.id.is_none();
        let (reply, response) = mpsc::channel();
        handler(AgentRequest {
            id: request.id,
            method: request.method,
            params: request.params,
            reply,
        });
        if notification {
            continue;
        }
        match response.recv() {
            Ok(response) => {
                if writeln!(writer, "{response}").is_err() || writer.flush().is_err() {
                    break;
                }
            }
            Err(_) => {
                let _ = writeln!(
                    writer,
                    "{}",
                    error_response(request.id, "handler disconnected")
                );
                let _ = writer.flush();
                break;
            }
        }
    }
}

pub fn serve_stdio(addr: &str, token: &str) -> io::Result<()> {
    let stream = TcpStream::connect(addr)?;
    let response_stream = stream.try_clone()?;
    thread::spawn(move || {
        let mut reader = BufReader::new(response_stream);
        let mut stdout = BufWriter::new(io::stdout());
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            if writeln!(stdout, "{}", line.trim_end()).is_err() || stdout.flush().is_err() {
                break;
            }
        }
    });

    let mut writer = BufWriter::new(stream);
    for line in io::stdin().lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let mut value: Value = serde_json::from_str(&line).unwrap_or(Value::Null);
        if let Some(object) = value.as_object_mut() {
            object.insert("token".to_string(), json!(token));
        }
        writeln!(writer, "{value}")?;
        writer.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_answers_authorized_request() {
        let server = AgentServer::start("token".to_string(), |request| {
            let response = json!({
                "jsonrpc": "2.0",
                "id": request.id,
                "result": { "method": request.method }
            })
            .to_string();
            let _ = request.reply.send(response);
        })
        .expect("server");
        let mut stream = TcpStream::connect(server.addr).expect("connect");
        writeln!(
            stream,
            r#"{{"token":"token","id":7,"method":"ping","params":{{}}}}"#
        )
        .expect("write");
        let mut response = String::new();
        BufReader::new(stream)
            .read_line(&mut response)
            .expect("read");
        let value: Value = serde_json::from_str(response.trim()).expect("json");
        assert_eq!(value["id"], 7);
        assert_eq!(value["result"]["method"], "ping");
    }

    #[test]
    fn server_rejects_invalid_token() {
        let server = AgentServer::start("token".to_string(), |_| {}).expect("server");
        let mut stream = TcpStream::connect(server.addr).expect("connect");
        writeln!(
            stream,
            r#"{{"token":"wrong","id":3,"method":"ping","params":{{}}}}"#
        )
        .expect("write");
        let mut response = String::new();
        BufReader::new(stream)
            .read_line(&mut response)
            .expect("read");
        let value: Value = serde_json::from_str(response.trim()).expect("json");
        assert!(value["error"].is_object());
    }
}
