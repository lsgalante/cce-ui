//! Minimal MCP (Model Context Protocol) server support for cce-ui apps.
//!
//! Implements the tools-only subset of the spec — `initialize`, `tools/list`,
//! `tools/call`, `ping` — over the Streamable HTTP transport (JSON-RPC 2.0 in
//! HTTP POST bodies), so any MCP client can inspect and drive a running app,
//! e.g. `claude mcp add --transport http <name> http://127.0.0.1:<port>/mcp`.
//! The server is stateless: no sessions, no SSE stream, no server-initiated
//! messages (a GET gets 405).
//!
//! An app declares its [`McpTool`]s and starts an [`McpServer`] with the
//! engine's calloop sender plus a constructor wrapping [`McpToolCall`] into
//! its `Application::Message`; each `tools/call` is executed on the app's
//! event loop and answered over the carried mpsc channel — the same bridge
//! pattern as cce-designer's HTTP automation API.
//!
//! What a server tells its clients beyond the tools:
//!
//! - **Where it is.** Once listening, it writes its URL and a fresh bearer
//!   token to the [`registry`], keyed by app id and pid, so a client (the
//!   cce-code-agent popup) reaches the instance that owns the focused window.
//!   By default the OS picks the port and every request must carry the
//!   token; [`McpServer::fixed_port`] keeps a port for clients configured
//!   by hand in `~/.claude.json`, which carry none.
//! - **What each tool does to the world** ([`ToolClass`], sent as the spec's
//!   tool `annotations`), so a client can run reads without asking and ask
//!   before anything that cannot be undone.
//! - **How to use the app** ([`McpServer::instructions`], the `initialize`
//!   reply's `instructions`, which clients put in the model's context).
//! - **What the user is looking at**, through the standard `context` tool
//!   ([`context_tool`], answered with a [`Context`]).

pub mod registry;

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use serde::Serialize;
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

/// What a tool does to the world, which decides how a client asks before
/// running it. Sent in `tools/list` as the spec's `annotations` hints; a tool
/// the server does not classify carries none, which the spec reads as
/// "may destroy, may reach outside".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolClass {
    /// Reads only. A client may run it without asking.
    Read,
    /// Changes the app's state in a way the app can undo (its undo history,
    /// the vault's versions, a message moved back).
    Edit,
    /// Cannot be undone, or leaves the machine: sending, deleting for good,
    /// submitting a form. A client asks every time.
    Outward,
}

impl ToolClass {
    /// The `annotations` object `tools/list` carries for this class.
    pub fn annotations(self) -> Value {
        match self {
            ToolClass::Read => json!({ "readOnlyHint": true }),
            ToolClass::Edit => json!({ "readOnlyHint": false, "destructiveHint": false }),
            ToolClass::Outward => {
                json!({ "readOnlyHint": false, "destructiveHint": true, "openWorldHint": true })
            }
        }
    }

    /// The class a tool's `annotations` stand for, as a client reads them
    /// back; `None` for a tool that carries no hints.
    pub fn from_annotations(annotations: &Value) -> Option<ToolClass> {
        let hint = |k: &str| annotations.get(k).and_then(Value::as_bool);
        match (hint("readOnlyHint"), hint("destructiveHint"), hint("openWorldHint")) {
            (Some(true), ..) => Some(ToolClass::Read),
            (Some(false), Some(false), open) if open != Some(true) => Some(ToolClass::Edit),
            (Some(false), ..) => Some(ToolClass::Outward),
            _ => None,
        }
    }
}

/// The standard `context` tool's name.
pub const CONTEXT_TOOL: &str = "context";

/// The standard `context` tool: every app answers it with a [`Context`] of
/// what the user is looking at, so a client can resolve "this" and "here"
/// before its first question. An [`McpServer`] classes it [`ToolClass::Read`].
pub fn context_tool() -> McpTool {
    McpTool {
        name: CONTEXT_TOOL.to_string(),
        description: "What the user is looking at in this app right now: the open document, the view, \
                      the selection (as ids the other tools take), whether there are unsaved changes, \
                      and how a change made through these tools is undone. Call it first."
            .to_string(),
        input_schema: json!({ "type": "object", "properties": {} }),
    }
}

/// The longest `summary` a [`Context`] sends, in lines.
pub const CONTEXT_SUMMARY_LINES: usize = 40;

/// The `context` tool's answer. Every app fills the fields that apply and
/// leaves the rest out; the shape is the same in every app so a client can
/// read it without knowing which app answered.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Context {
    /// The app's id and version (`cce-designer 0.4.0`).
    pub app: String,
    /// The open file, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document: Option<ContextDocument>,
    /// What is on screen, in the app's own terms: a folder, a date range, a
    /// sheet and its visible cells, a node network's level.
    #[serde(skip_serializing_if = "Value::is_null")]
    pub view: Value,
    /// The selected items, by the ids the app's other tools take.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub selection: Vec<String>,
    /// Whether there are unsaved changes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dirty: Option<bool>,
    /// How a change made through the tools is undone (`"the undo tool"`,
    /// `"vault history"`), or that it cannot be.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub undo: Option<String>,
    /// Plain text the app chooses to add (the selected message's headers),
    /// cut to [`CONTEXT_SUMMARY_LINES`] lines when sent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

/// The file a [`Context`] names.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ContextDocument {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

impl Context {
    /// A context naming `app` (an app id and version) and nothing else yet.
    pub fn new(app: impl Into<String>) -> Self {
        Context { app: app.into(), ..Context::default() }
    }

    /// The tool result: the context as JSON, its summary cut to length.
    pub fn into_result(mut self) -> Value {
        if let Some(s) = &mut self.summary {
            if let Some((cut, _)) = s.match_indices('\n').nth(CONTEXT_SUMMARY_LINES - 1) {
                s.truncate(cut);
                s.push_str("\n…");
            }
        }
        serde_json::to_value(self).unwrap_or_default()
    }
}

/// A `tools/call` in flight: delivered to the app's event loop wrapped in its
/// `Application::Message`; the handler sends the outcome back over `reply`.
/// An `Ok` value becomes the result's text content (strings verbatim, other
/// JSON pretty-printed) — unless it was built by [`content`], whose blocks
/// are sent as they are; an `Err` becomes an `isError` tool result.
#[derive(Debug, Clone)]
pub struct McpToolCall {
    pub name: String,
    pub arguments: Value,
    pub reply: mpsc::Sender<Result<Value, String>>,
}

/// The key [`content`] wraps its blocks under. Spelled so that no ordinary
/// JSON result a tool returns could carry it by accident.
const CONTENT_KEY: &str = "$mcp-content";

/// A tool result made of ready-built MCP content blocks — an image beside
/// its caption, say — rather than one text block. Return it as the `Ok`
/// value of a [`McpToolCall`].
pub fn content(blocks: Vec<Value>) -> Value {
    json!({ CONTENT_KEY: blocks })
}

/// A text content block, for [`content`].
pub fn text_block(text: impl Into<String>) -> Value {
    json!({ "type": "text", "text": text.into() })
}

/// An image content block, for [`content`]: `data` is the base64 of the
/// encoded image, `mime_type` its type (`image/png`, `image/jpeg`).
pub fn image_block(data: String, mime_type: &str) -> Value {
    json!({ "type": "image", "data": data, "mimeType": mime_type })
}

/// The content blocks a tool's `Ok` value stands for.
fn content_blocks(value: Value) -> Value {
    match value {
        Value::Object(mut map) if map.len() == 1 && map.get(CONTENT_KEY).is_some_and(Value::is_array) => {
            map.remove(CONTENT_KEY).unwrap_or_default()
        }
        Value::String(s) => json!([text_block(s)]),
        other => json!([text_block(serde_json::to_string_pretty(&other).unwrap_or_default())]),
    }
}

/// An MCP server to start: the app's tools and what it tells clients about
/// them. Build with [`McpServer::new`], adjust, then [`start`](Self::start).
#[derive(Debug, Clone)]
pub struct McpServer {
    name: String,
    app_id: String,
    tools: Vec<McpTool>,
    classes: HashMap<String, ToolClass>,
    instructions: Option<String>,
    companions: Vec<String>,
    /// 0: the OS picks.
    port: u16,
    /// Where the server files itself: [`registry::dir`] outside tests.
    registry_dir: std::path::PathBuf,
}

impl McpServer {
    /// A server named `name` (also its registry app id) serving `tools` on a
    /// port the OS picks, every request carrying the registry's token. A
    /// tool named [`CONTEXT_TOOL`] is classed [`ToolClass::Read`].
    pub fn new(name: &str, tools: Vec<McpTool>) -> Self {
        McpServer {
            name: name.to_string(),
            app_id: name.to_string(),
            tools,
            classes: HashMap::from([(CONTEXT_TOOL.to_string(), ToolClass::Read)]),
            instructions: None,
            companions: Vec::new(),
            port: 0,
            registry_dir: registry::dir(),
        }
    }

    /// The app id the registry files the server under, when it differs from
    /// the server's name: it must match what the app's windows report.
    pub fn app_id(mut self, app_id: &str) -> Self {
        self.app_id = app_id.to_string();
        self
    }

    /// Bind `port` rather than one the OS picks, for clients configured by
    /// hand with that URL (`~/.claude.json`). Those send no token, so a
    /// fixed-port server serves requests without one; a request that does
    /// carry a token must carry the right one.
    pub fn fixed_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// Class the tools named in `names`. Unknown names are ignored (a tool
    /// cfg'd out of this build), so one list can serve every build.
    pub fn classify(mut self, class: ToolClass, names: &[&str]) -> Self {
        for name in names {
            self.classes.insert((*name).to_string(), class);
        }
        self
    }

    /// The app's brief for the model, sent as `initialize`'s `instructions`:
    /// what the app is, what to call first, how undo works.
    pub fn instructions(mut self, text: &str) -> Self {
        self.instructions = Some(text.to_string());
        self
    }

    /// App ids whose servers a client should load beside this one.
    pub fn companions(mut self, app_ids: &[&str]) -> Self {
        self.companions = app_ids.iter().map(|s| s.to_string()).collect();
        self
    }

    /// File the server in `dir` instead of the session's registry.
    #[cfg(test)]
    fn registry_dir(mut self, dir: &std::path::Path) -> Self {
        self.registry_dir = dir.to_path_buf();
        self
    }

    /// The class a tool was given, if any.
    pub fn class_of(&self, tool: &str) -> Option<ToolClass> {
        self.classes.get(tool).copied()
    }

    /// The tools served without a class, which a client treats as able to
    /// destroy anything (the cce-code-agent popup asks before every call). An
    /// app's test asserts this is empty, so a new tool cannot ship unclassed.
    pub fn unclassed(&self) -> Vec<&str> {
        self.tools.iter().map(|t| t.name.as_str()).filter(|n| !self.classes.contains_key(*n)).collect()
    }

    /// Spawn the server on `127.0.0.1` (one thread per connection) and file
    /// it in the [`registry`]. `wrap` lifts a tool call into the app's
    /// message type for delivery over `sender`.
    pub fn start<M, F>(self, sender: calloop::channel::Sender<M>, wrap: F)
    where
        M: Send + 'static,
        F: Fn(McpToolCall) -> M + Send + Sync + 'static,
    {
        std::thread::spawn(move || {
            let McpServer { name, app_id, tools, classes, instructions, companions, port, registry_dir } = self;
            let listener = match TcpListener::bind(("127.0.0.1", port)) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("Failed to bind MCP server '{name}' to port {port}: {e:?}");
                    return;
                }
            };
            let bound = listener.local_addr().map(|a| a.port()).unwrap_or(port);
            let url = format!("http://127.0.0.1:{bound}/mcp");
            println!("MCP server '{name}' listening on {url}");

            let token = match new_token() {
                Ok(t) => Some(t),
                Err(e) => {
                    eprintln!("MCP server '{name}': no token ({e}), so it is not registered");
                    None
                }
            };
            if token.is_none() && port == 0 {
                // Nothing could find it and nothing could authenticate to it.
                return;
            }
            if let Some(token) = &token {
                let pid = std::process::id();
                let entry = registry::Entry {
                    app_id,
                    pid,
                    start_time: registry::process_start_time(pid).unwrap_or(0),
                    url,
                    token: token.clone(),
                    companions,
                    display: registry::current_display(),
                };
                if let Err(e) = registry::register(&registry_dir, &entry) {
                    eprintln!("MCP server '{name}': registry write failed: {e}");
                }
            }

            let served = Served { name, tools, classes, instructions, token, require_token: port == 0 };
            let shared = Arc::new((served, sender, wrap));
            for stream in listener.incoming() {
                let stream = match stream {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                let shared = Arc::clone(&shared);
                std::thread::spawn(move || {
                    let (served, sender, wrap) = &*shared;
                    handle_connection(stream, served, &|name, arguments| {
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
}

/// Spawn an MCP server on the fixed `127.0.0.1:port`, as
/// `McpServer::new(server_name, tools).fixed_port(port).start(sender, wrap)`.
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
    McpServer::new(server_name, tools).fixed_port(port).start(sender, wrap);
}

/// 32 random bytes as hex.
fn new_token() -> std::io::Result<String> {
    let mut bytes = [0u8; 32];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// What a running server answers with: its tools and what it says about them.
struct Served {
    name: String,
    tools: Vec<McpTool>,
    classes: HashMap<String, ToolClass>,
    instructions: Option<String>,
    token: Option<String>,
    /// Refuse a request with no token (a request with a wrong one is always
    /// refused).
    require_token: bool,
}

impl Served {
    /// Whether a request's `Authorization` header lets it in.
    fn authorized(&self, authorization: Option<&str>) -> bool {
        let presented = authorization.and_then(|v| v.strip_prefix("Bearer ")).map(str::trim);
        match (presented, &self.token) {
            (None, _) => !self.require_token,
            (Some(p), Some(t)) => constant_time_eq(p.as_bytes(), t.as_bytes()),
            (Some(_), None) => false,
        }
    }
}

/// Compare without stopping at the first difference, so the time a refusal
/// takes says nothing about how much of a guess was right.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn handle_connection<F>(stream: TcpStream, served: &Served, call_tool: &F)
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
    let mut authorization = None;
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
            "authorization" => authorization = Some(value),
            _ => {}
        }
    }

    if !request_from_this_machine(host.as_deref(), origin.as_deref()) {
        let _ = write_stream.write_all(
            b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        );
        return;
    }

    if !served.authorized(authorization.as_deref()) {
        let _ = write_stream.write_all(
            b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
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
        Ok(req) => handle_jsonrpc(&req, served, call_tool),
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
fn handle_jsonrpc<F>(req: &Value, served: &Served, call_tool: &F) -> Option<Value>
where
    F: Fn(&str, Value) -> Result<Value, String>,
{
    let tools = &served.tools;
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
            let mut result = json!({
                "protocolVersion": version,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": served.name, "version": env!("CARGO_PKG_VERSION") },
            });
            if let Some(text) = &served.instructions {
                result["instructions"] = json!(text);
            }
            result
        }
        "ping" => json!({}),
        "tools/list" => json!({
            "tools": tools.iter().map(|t| {
                let mut tool = json!({
                    "name": t.name,
                    "description": t.description,
                    "inputSchema": t.input_schema,
                });
                if let Some(class) = served.classes.get(&t.name) {
                    tool["annotations"] = class.annotations();
                }
                tool
            }).collect::<Vec<_>>(),
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
                Ok(value) => json!({ "content": content_blocks(value), "isError": false }),
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
mod tests;
