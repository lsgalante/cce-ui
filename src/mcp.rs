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
    let mut write_stream = match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    };
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }

    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).is_err() || line == "\r\n" || line == "\n" || line.is_empty()
        {
            break;
        }
        let lower = line.to_lowercase();
        if let Some(rest) = lower.strip_prefix("content-length:") {
            if let Ok(len) = rest.trim().parse::<usize>() {
                content_length = len;
            }
        }
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
}
