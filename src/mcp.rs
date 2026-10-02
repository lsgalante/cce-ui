//! Minimal MCP (Model Context Protocol) server support for cce-ui apps.
//!
//! Implements the tools-only subset of the spec — `initialize`, `tools/list`,
//! `tools/call`, `ping` — over the Streamable HTTP transport (JSON-RPC 2.0 in
//! HTTP POST bodies), so any MCP client can inspect and drive a running app,
//! e.g. `claude mcp add --transport http <name> http://127.0.0.1:<port>/mcp`.
//! The server is stateless: no sessions, no SSE stream, no server-initiated
//! messages (a GET gets 405).
//!
//! An app declares its [`McpTool`]s and calls [`start_mcp_server`] with the
//! engine's calloop sender plus a constructor wrapping [`McpToolCall`] into
//! its `Application::Message`; each `tools/call` is executed on the app's
//! event loop and answered over the carried mpsc channel — the same bridge
//! pattern as cce-designer's HTTP automation API.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use serde_json::{json, Value};

/// Protocol revisions this server accepts; the newest is offered when the
/// client requests anything else. The tools-only subset is identical across
/// all of them.
const PROTOCOL_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

/// How long a `tools/call` waits on the app's event loop before failing.
const CALL_TIMEOUT: Duration = Duration::from_secs(30);

/// How long a connection may take to deliver its request. Each connection
/// holds a thread, so one that never finishes sending must not hold it forever.
const READ_TIMEOUT: Duration = Duration::from_secs(10);

/// The largest request body read. The body is allocated at the size the
/// client's `Content-Length` claims, and an allocation that fails aborts the
/// whole app — not just this thread — so one request claiming 2^63 bytes
/// would take the window and its unsaved work down with it.
const MAX_BODY: usize = 16 << 20;
/// Room for the request line and headers on top of the body.
const MAX_HEAD: usize = 64 << 10;

/// Whether a request came from a program on this machine rather than from a
/// web page the user has open.
///
/// Binding loopback keeps the network out, not the browser: any page can POST
/// to `127.0.0.1:<port>` (a `text/plain` body needs no CORS preflight, and
/// this server parses whatever arrives as JSON), and with DNS rebinding it can
/// read the replies too — which for cce-notes is the whole vault. Two checks,
/// as the MCP transport spec asks:
///
/// - **`Origin`**, which a browser attaches to every POST, must be absent (an
///   MCP client is not a browser and sends none) or name loopback. That stops
///   a page posting from its own origin.
/// - **`Host`** must name loopback. A rebound page posts to ITS hostname, now
///   resolving to 127.0.0.1, and that hostname is what arrives here — the one
///   thing a rebind cannot change.
fn request_from_this_machine(host: Option<&str>, origin: Option<&str>) -> bool {
    // "localhost:3002", "[::1]:3002", "127.0.0.1" -> the bare host name.
    fn hostname(authority: &str) -> &str {
        if let Some(rest) = authority.strip_prefix('[') {
            return rest.split(']').next().unwrap_or("");
        }
        authority.split(':').next().unwrap_or("")
    }
    fn loopback(authority: &str) -> bool {
        let h = hostname(authority).to_ascii_lowercase();
        h == "localhost" || h == "::1" || h.parse::<std::net::Ipv4Addr>().is_ok_and(|ip| ip.is_loopback())
    }
    let host_ok = host.is_some_and(loopback);
    let origin_ok = match origin {
        None => true,
        Some(o) => o
            .strip_prefix("http://")
            .or_else(|| o.strip_prefix("https://"))
            .is_some_and(|authority| loopback(authority.trim_end_matches('/'))),
    };
    host_ok && origin_ok
}

/// A tool the app exposes over MCP.
#[derive(Debug, Clone)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    /// JSON Schema for the tool's arguments (the spec's `inputSchema`).
    pub input_schema: Value,
}

/// A `tools/call` in flight: delivered to the app's event loop wrapped in its
/// `Application::Message`; the handler sends the outcome back over `reply`.
/// An `Ok` value becomes the result's text content (strings verbatim, other
/// JSON pretty-printed); an `Err` becomes an `isError` tool result.
#[derive(Debug, Clone)]
pub struct McpToolCall {
    pub name: String,
    pub arguments: Value,
    pub reply: mpsc::Sender<Result<Value, String>>,
}

/// Spawn the MCP server on `127.0.0.1:port` (one thread per connection,
/// mirroring the raw-HTTP style of cce-designer's api.rs). `wrap` lifts a
/// tool call into the app's message type for delivery over `sender`.
pub fn start_mcp_server<M, F>(
    server_name: &str,
    port: u16,
    tools: Vec<McpTool>,
    sender: calloop::channel::Sender<M>,
    wrap: F,
) where
    M: Send + 'static,
    F: Fn(McpToolCall) -> M + Send + Sync + 'static,
{
    let server_name = server_name.to_string();
    std::thread::spawn(move || {
        let listener = match TcpListener::bind(("127.0.0.1", port)) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("Failed to bind MCP server to port {port}: {e:?}");
                return;
            }
        };
        println!("MCP server '{server_name}' listening on http://127.0.0.1:{port}");

        let shared = Arc::new((server_name, tools, sender, wrap));
        for stream in listener.incoming() {
            let stream = match stream {
                Ok(s) => s,
                Err(_) => continue,
            };
            let shared = Arc::clone(&shared);
            std::thread::spawn(move || {
                let (server_name, tools, sender, wrap) = &*shared;
                handle_connection(stream, server_name, tools, &|name, arguments| {
                    let (tx, rx) = mpsc::channel();
                    let call = McpToolCall { name: name.to_string(), arguments, reply: tx };
                    sender
                        .send(wrap(call))
                        .map_err(|_| "app event loop is gone".to_string())?;
                    rx.recv_timeout(CALL_TIMEOUT)
                        .map_err(|_| "timed out waiting for the app".to_string())?
                });
            });
        }
    });
}

fn handle_connection<F>(stream: TcpStream, server_name: &str, tools: &[McpTool], call_tool: &F)
where
    F: Fn(&str, Value) -> Result<Value, String>,
{
    let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
    let mut write_stream = match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    };
    // Bounded like the body: `read_line` would otherwise buffer one endless
    // header line for as long as the timeout keeps being met.
    let mut reader = BufReader::new(stream.take((MAX_BODY + MAX_HEAD) as u64));
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }

    let mut content_length = 0usize;
    let mut host = None;
    let mut origin = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).is_err() || line == "\r\n" || line == "\n" || line.is_empty()
        {
            break;
        }
        let Some((name, value)) = line.split_once(':') else { continue };
        let value = value.trim().to_string();
        match name.trim().to_ascii_lowercase().as_str() {
            "content-length" => {
                if let Ok(len) = value.parse::<usize>() {
                    content_length = len;
                }
            }
            "host" => host = Some(value),
            "origin" => origin = Some(value),
            _ => {}
        }
    }

    if !request_from_this_machine(host.as_deref(), origin.as_deref()) {
        let _ = write_stream.write_all(
            b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        );
        return;
    }

    if content_length > MAX_BODY {
        let _ = write_stream.write_all(
            b"HTTP/1.1 413 Content Too Large\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        );
        return;
    }

    if !request_line.starts_with("POST ") {
        // Stateless server: no SSE stream (GET) or session teardown (DELETE).
        let _ = write_stream.write_all(
            b"HTTP/1.1 405 Method Not Allowed\r\nAllow: POST\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        );
        return;
    }

    let mut body = vec![0; content_length];
    if reader.read_exact(&mut body).is_err() {
        return;
    }

    let response = match serde_json::from_slice::<Value>(&body) {
        Ok(req) => handle_jsonrpc(&req, server_name, tools, call_tool),
        Err(_) => Some(error_response(Value::Null, -32700, "parse error")),
    };
    let http = match response {
        Some(resp) => {
            let body = resp.to_string();
            format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
        }
        // Notifications get no JSON-RPC response, just an HTTP ack.
        None => "HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string(),
    };
    let _ = write_stream.write_all(http.as_bytes());
    let _ = write_stream.flush();
}

/// Dispatch one JSON-RPC message. Returns `None` for notifications (no id).
fn handle_jsonrpc<F>(req: &Value, server_name: &str, tools: &[McpTool], call_tool: &F) -> Option<Value>
where
    F: Fn(&str, Value) -> Result<Value, String>,
{
    let method = req.get("method").and_then(Value::as_str).unwrap_or("");
    let id = match req.get("id") {
        Some(id) if !id.is_null() => id.clone(),
        _ => return None,
    };

    let result = match method {
        "initialize" => {
            let requested = req
                .pointer("/params/protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or("");
            let version = if PROTOCOL_VERSIONS.contains(&requested) {
                requested
            } else {
                PROTOCOL_VERSIONS[0]
            };
            json!({
                "protocolVersion": version,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": server_name, "version": env!("CARGO_PKG_VERSION") },
            })
        }
        "ping" => json!({}),
        "tools/list" => json!({
            "tools": tools.iter().map(|t| json!({
                "name": t.name,
                "description": t.description,
                "inputSchema": t.input_schema,
            })).collect::<Vec<_>>(),
        }),
        "tools/call" => {
            let name = req
                .pointer("/params/name")
                .and_then(Value::as_str)
                .unwrap_or("");
            if !tools.iter().any(|t| t.name == name) {
                return Some(error_response(id, -32602, &format!("unknown tool: {name}")));
            }
            let arguments = req
                .pointer("/params/arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            match call_tool(name, arguments) {
                Ok(value) => {
                    let text = match value {
                        Value::String(s) => s,
                        other => serde_json::to_string_pretty(&other).unwrap_or_default(),
                    };
                    json!({ "content": [{ "type": "text", "text": text }], "isError": false })
                }
                Err(e) => json!({ "content": [{ "type": "text", "text": e }], "isError": true }),
            }
        }
        _ => return Some(error_response(id, -32601, &format!("method not found: {method}"))),
    };
    Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tools() -> Vec<McpTool> {
        vec![McpTool {
            name: "echo".to_string(),
            description: "Echo the arguments back".to_string(),
            input_schema: json!({ "type": "object" }),
        }]
    }

    fn no_calls(_: &str, _: Value) -> Result<Value, String> {
        panic!("no tool call expected");
    }

    #[test]
    fn initialize_negotiates_protocol_version() {
        let req = json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": { "protocolVersion": "2025-03-26" }
        });
        let resp = handle_jsonrpc(&req, "test", &tools(), &no_calls).unwrap();
        assert_eq!(resp["result"]["protocolVersion"], "2025-03-26");
        assert!(resp["result"]["capabilities"]["tools"].is_object());

        // Unknown requested version falls back to our newest.
        let req = json!({
            "jsonrpc": "2.0", "id": 2, "method": "initialize",
            "params": { "protocolVersion": "2099-01-01" }
        });
        let resp = handle_jsonrpc(&req, "test", &tools(), &no_calls).unwrap();
        assert_eq!(resp["result"]["protocolVersion"], PROTOCOL_VERSIONS[0]);
    }

    #[test]
    fn notifications_get_no_response() {
        let req = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
        assert!(handle_jsonrpc(&req, "test", &tools(), &no_calls).is_none());
    }

    #[test]
    fn tools_list_reports_declared_tools() {
        let req = json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/list" });
        let resp = handle_jsonrpc(&req, "test", &tools(), &no_calls).unwrap();
        assert_eq!(resp["result"]["tools"][0]["name"], "echo");
        assert!(resp["result"]["tools"][0]["inputSchema"].is_object());
    }

    #[test]
    fn tools_call_wraps_ok_and_err_results() {
        let req = json!({
            "jsonrpc": "2.0", "id": 4, "method": "tools/call",
            "params": { "name": "echo", "arguments": { "x": 1 } }
        });
        let resp = handle_jsonrpc(&req, "test", &tools(), &|name, args| {
            assert_eq!(name, "echo");
            Ok(args)
        })
        .unwrap();
        assert_eq!(resp["result"]["isError"], false);
        assert!(resp["result"]["content"][0]["text"].as_str().unwrap().contains("\"x\": 1"));

        let resp = handle_jsonrpc(&req, "test", &tools(), &|_, _| Err("boom".to_string())).unwrap();
        assert_eq!(resp["result"]["isError"], true);
        assert_eq!(resp["result"]["content"][0]["text"], "boom");
    }

    #[test]
    fn unknown_tool_and_method_are_protocol_errors() {
        let req = json!({
            "jsonrpc": "2.0", "id": 5, "method": "tools/call",
            "params": { "name": "nope" }
        });
        let resp = handle_jsonrpc(&req, "test", &tools(), &no_calls).unwrap();
        assert_eq!(resp["error"]["code"], -32602);

        let req = json!({ "jsonrpc": "2.0", "id": 6, "method": "resources/list" });
        let resp = handle_jsonrpc(&req, "test", &tools(), &no_calls).unwrap();
        assert_eq!(resp["error"]["code"], -32601);
    }

    #[test]
    fn only_requests_from_this_machine_are_served() {
        // An MCP client: no Origin, a loopback Host in any spelling.
        for host in ["127.0.0.1:3002", "localhost:3002", "LOCALHOST", "[::1]:3002", "127.0.0.2:3002"] {
            assert!(request_from_this_machine(Some(host), None), "{host}");
        }
        // A page on a loopback origin is this machine too.
        assert!(request_from_this_machine(Some("127.0.0.1:3002"), Some("http://localhost:5173")));
        assert!(request_from_this_machine(Some("127.0.0.1:3002"), Some("http://[::1]:8080")));

        // A web page posting cross-origin: the Host is ours, the Origin is not.
        assert!(!request_from_this_machine(Some("127.0.0.1:3002"), Some("https://evil.example")));
        assert!(!request_from_this_machine(Some("127.0.0.1:3002"), Some("null")));
        // Look-alikes of loopback are not loopback.
        assert!(!request_from_this_machine(Some("127.0.0.1:3002"), Some("http://localhost.evil.example")));
        assert!(!request_from_this_machine(Some("127.0.0.1:3002"), Some("http://127.0.0.1.evil.example")));
        // DNS rebinding: the page's own hostname arrives as the Host.
        assert!(!request_from_this_machine(Some("rebind.evil.example:3002"), Some("http://rebind.evil.example:3002")));
        assert!(!request_from_this_machine(Some("rebind.evil.example:3002"), None));
        assert!(!request_from_this_machine(Some("localhost.evil.example"), None));
        // No Host at all is not HTTP/1.1 from anyone we serve.
        assert!(!request_from_this_machine(None, None));
    }

    /// Run one raw request through `handle_connection` over a real socket.
    fn exchange(request: &[u8]) -> String {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            handle_connection(stream, "test", &tools(), &|_: &str, args: Value| Ok(args));
        });
        let mut client = TcpStream::connect(addr).unwrap();
        client.write_all(request).unwrap();
        let _ = client.shutdown(std::net::Shutdown::Write);
        let mut reply = String::new();
        let _ = client.read_to_string(&mut reply);
        server.join().unwrap();
        reply
    }

    fn post(host: &str, extra: &str, body: &str) -> Vec<u8> {
        format!(
            "POST /mcp HTTP/1.1\r\nHost: {host}\r\n{extra}Content-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .into_bytes()
    }

    #[test]
    fn a_web_page_gets_403_and_its_tool_never_runs() {
        let call = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"echo","arguments":{"x":1}}}"#;
        let ok = exchange(&post("127.0.0.1:3002", "", call));
        assert!(ok.starts_with("HTTP/1.1 200"), "{ok}");
        assert!(ok.contains("isError\":false"), "{ok}");

        let csrf = exchange(&post("127.0.0.1:3002", "Origin: https://evil.example\r\nContent-Type: text/plain\r\n", call));
        assert!(csrf.starts_with("HTTP/1.1 403"), "{csrf}");
        let rebind = exchange(&post("rebind.evil.example:3002", "Origin: http://rebind.evil.example:3002\r\n", call));
        assert!(rebind.starts_with("HTTP/1.1 403"), "{rebind}");
    }

    #[test]
    fn a_huge_content_length_is_refused_not_allocated() {
        // Before the cap this allocated the claimed size, and a failed
        // allocation aborts the process: this test would take the runner down.
        let req = "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 9223372036854775807\r\n\r\n{}";
        let reply = exchange(req.as_bytes());
        assert!(reply.starts_with("HTTP/1.1 413"), "{reply}");
    }
}
