use super::*;

fn tools() -> Vec<McpTool> {
    vec![McpTool {
        name: "echo".to_string(),
        description: "Echo the arguments back".to_string(),
        input_schema: json!({ "type": "object" }),
    }]
}

/// The tools served the way a fixed-port server serves them: no token
/// required, no classes, no instructions.
fn served() -> Served {
    Served {
        name: "test".into(),
        tools: tools(),
        classes: HashMap::new(),
        instructions: None,
        token: Some("s3cret".into()),
        require_token: false,
    }
}

fn no_calls(_: &str, _: Value) -> Result<Value, String> {
    panic!("no tool call expected");
}

#[test]
fn tools_list_carries_each_class_as_annotations() {
    let server = McpServer::new("test", vec![tools().remove(0), context_tool()])
        .classify(ToolClass::Outward, &["echo", "not-in-this-build"]);
    let classed = Served {
        name: server.name,
        tools: server.tools,
        classes: server.classes,
        instructions: None,
        token: None,
        require_token: false,
    };
    let req = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" });
    let resp = handle_jsonrpc(&req, &classed, &no_calls).unwrap();
    let listed = resp["result"]["tools"].as_array().unwrap();
    let class = |i: usize| ToolClass::from_annotations(&listed[i]["annotations"]);
    assert_eq!(class(0), Some(ToolClass::Outward));
    // `context` is a read wherever it is served.
    assert_eq!(listed[1]["name"], CONTEXT_TOOL);
    assert_eq!(class(1), Some(ToolClass::Read));

    // An unclassified tool carries no annotations at all.
    let resp = handle_jsonrpc(&req, &served(), &no_calls).unwrap();
    assert!(resp["result"]["tools"][0].get("annotations").is_none());
}

#[test]
fn every_class_reads_back_from_its_annotations() {
    for class in [ToolClass::Read, ToolClass::Edit, ToolClass::Outward] {
        assert_eq!(ToolClass::from_annotations(&class.annotations()), Some(class));
    }
    assert_eq!(ToolClass::from_annotations(&json!({})), None);
    // The spec's defaults: not read-only and nothing more said = may destroy.
    assert_eq!(ToolClass::from_annotations(&json!({ "readOnlyHint": false })), Some(ToolClass::Outward));
    // Undoable but reaching outside is still outward.
    assert_eq!(
        ToolClass::from_annotations(&json!({ "readOnlyHint": false, "destructiveHint": false, "openWorldHint": true })),
        Some(ToolClass::Outward)
    );
}

#[test]
fn initialize_sends_the_instructions_when_there_are_some() {
    let req = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} });
    let resp = handle_jsonrpc(&req, &served(), &no_calls).unwrap();
    assert!(resp["result"].get("instructions").is_none());

    let briefed = Served { instructions: Some("Call context first.".into()), ..served() };
    let resp = handle_jsonrpc(&req, &briefed, &no_calls).unwrap();
    assert_eq!(resp["result"]["instructions"], "Call context first.");
}

#[test]
fn the_token_gate() {
    let open = served();
    assert!(open.authorized(None), "a fixed port serves hand-configured clients");
    assert!(open.authorized(Some("Bearer s3cret")));
    assert!(!open.authorized(Some("Bearer s3cre")), "a wrong token is refused even when none is required");
    assert!(!open.authorized(Some("Bearer s3cret!")));

    let gated = Served { require_token: true, ..served() };
    assert!(!gated.authorized(None));
    assert!(!gated.authorized(Some("s3cret")), "the scheme is part of the header");
    assert!(gated.authorized(Some("Bearer s3cret")));

    let tokenless = Served { token: None, require_token: false, ..served() };
    assert!(!tokenless.authorized(Some("Bearer anything")));
}

#[test]
fn a_request_without_the_token_gets_401_and_its_tool_never_runs() {
    let call = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"echo","arguments":{}}}"#;
    let gated = || Served { require_token: true, ..served() };
    let refused = exchange_with(gated(), &post("127.0.0.1:1", "", call));
    assert!(refused.starts_with("HTTP/1.1 401"), "{refused}");
    let ok = exchange_with(gated(), &post("127.0.0.1:1", "Authorization: Bearer s3cret\r\n", call));
    assert!(ok.starts_with("HTTP/1.1 200"), "{ok}");
}

#[test]
fn context_leaves_out_what_an_app_does_not_fill_and_cuts_the_summary() {
    let bare = Context::new("cce-test 1.0").into_result();
    assert_eq!(bare, json!({ "app": "cce-test 1.0" }));

    let long: Vec<String> = (0..100).map(|i| format!("line {i}")).collect();
    let full = Context {
        document: Some(ContextDocument { title: "Lamp".into(), path: Some("/p/lamp.json".into()) }),
        view: json!({ "level": "/obj/lamp" }),
        selection: vec!["/obj/lamp/shade".into()],
        dirty: Some(true),
        undo: Some("the undo tool".into()),
        summary: Some(long.join("\n")),
        ..Context::new("cce-test 1.0")
    }
    .into_result();
    assert_eq!(full["document"]["path"], "/p/lamp.json");
    assert_eq!(full["selection"][0], "/obj/lamp/shade");
    let summary = full["summary"].as_str().unwrap();
    assert_eq!(summary.lines().count(), CONTEXT_SUMMARY_LINES + 1);
    assert!(summary.ends_with("line 39\n…"));
}

#[test]
fn initialize_negotiates_protocol_version() {
    let req = json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": { "protocolVersion": "2025-03-26" }
    });
    let resp = handle_jsonrpc(&req, &served(), &no_calls).unwrap();
    assert_eq!(resp["result"]["protocolVersion"], "2025-03-26");
    assert!(resp["result"]["capabilities"]["tools"].is_object());

    // Unknown requested version falls back to our newest.
    let req = json!({
        "jsonrpc": "2.0", "id": 2, "method": "initialize",
        "params": { "protocolVersion": "2099-01-01" }
    });
    let resp = handle_jsonrpc(&req, &served(), &no_calls).unwrap();
    assert_eq!(resp["result"]["protocolVersion"], PROTOCOL_VERSIONS[0]);
}

#[test]
fn notifications_get_no_response() {
    let req = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
    assert!(handle_jsonrpc(&req, &served(), &no_calls).is_none());
}

#[test]
fn tools_list_reports_declared_tools() {
    let req = json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/list" });
    let resp = handle_jsonrpc(&req, &served(), &no_calls).unwrap();
    assert_eq!(resp["result"]["tools"][0]["name"], "echo");
    assert!(resp["result"]["tools"][0]["inputSchema"].is_object());
}

#[test]
fn tools_call_wraps_ok_and_err_results() {
    let req = json!({
        "jsonrpc": "2.0", "id": 4, "method": "tools/call",
        "params": { "name": "echo", "arguments": { "x": 1 } }
    });
    let resp = handle_jsonrpc(&req, &served(), &|name, args| {
        assert_eq!(name, "echo");
        Ok(args)
    })
    .unwrap();
    assert_eq!(resp["result"]["isError"], false);
    assert!(resp["result"]["content"][0]["text"].as_str().unwrap().contains("\"x\": 1"));

    let resp = handle_jsonrpc(&req, &served(), &|_, _| Err("boom".to_string())).unwrap();
    assert_eq!(resp["result"]["isError"], true);
    assert_eq!(resp["result"]["content"][0]["text"], "boom");
}

#[test]
fn content_results_are_sent_as_built() {
    let req = json!({
        "jsonrpc": "2.0", "id": 7, "method": "tools/call",
        "params": { "name": "echo", "arguments": {} }
    });
    let resp = handle_jsonrpc(&req, &served(), &|_, _| {
        Ok(content(vec![image_block("iVBO".into(), "image/png"), text_block("a shot")]))
    })
    .unwrap();
    let blocks = &resp["result"]["content"];
    assert_eq!(blocks[0]["type"], "image");
    assert_eq!(blocks[0]["data"], "iVBO");
    assert_eq!(blocks[0]["mimeType"], "image/png");
    assert_eq!(blocks[1]["text"], "a shot");

    // A result that merely has the key among others is plain JSON.
    let resp = handle_jsonrpc(&req, &served(), &|_, _| {
        Ok(json!({ CONTENT_KEY: [], "other": 1 }))
    })
    .unwrap();
    assert_eq!(resp["result"]["content"][0]["type"], "text");
}

#[test]
fn unknown_tool_and_method_are_protocol_errors() {
    let req = json!({
        "jsonrpc": "2.0", "id": 5, "method": "tools/call",
        "params": { "name": "nope" }
    });
    let resp = handle_jsonrpc(&req, &served(), &no_calls).unwrap();
    assert_eq!(resp["error"]["code"], -32602);

    let req = json!({ "jsonrpc": "2.0", "id": 6, "method": "resources/list" });
    let resp = handle_jsonrpc(&req, &served(), &no_calls).unwrap();
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
    exchange_with(served(), request)
}

fn exchange_with(served: Served, request: &[u8]) -> String {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let addr = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        handle_connection(stream, &served, &|_: &str, args: Value| Ok(args));
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

/// POST `body` to `url` with an optional bearer token; the raw reply.
fn post_to(url: &str, token: Option<&str>, body: &str) -> String {
    let addr = url.trim_start_matches("http://").trim_end_matches("/mcp");
    let auth = token.map(|t| format!("Authorization: Bearer {t}\r\n")).unwrap_or_default();
    let mut client = TcpStream::connect(addr).unwrap();
    client.write_all(&post(addr, &auth, body)).unwrap();
    let _ = client.shutdown(std::net::Shutdown::Write);
    let mut reply = String::new();
    let _ = client.read_to_string(&mut reply);
    reply
}

#[test]
fn a_started_server_files_itself_and_admits_only_its_token() {
    let dir = std::env::temp_dir().join(format!("cce-ui-mcp-start-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let (sender, _channel) = calloop::channel::channel::<McpToolCall>();
    McpServer::new("cce-start-test", tools())
        .instructions("Call context first.")
        .companions(&["cce-notes"])
        .registry_dir(&dir)
        .start(sender, |call| call);

    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let entry = loop {
        if let Some(e) = registry::find(&dir, "cce-start-test", Some(std::process::id())) {
            break e;
        }
        assert!(std::time::Instant::now() < deadline, "the server never filed itself");
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(entry.companions, vec!["cce-notes".to_string()]);
    assert_eq!(entry.token.len(), 64);
    assert!(!entry.url.ends_with(":0/mcp"), "the bound port is filed, not 0: {}", entry.url);

    let init = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;
    assert!(post_to(&entry.url, None, init).starts_with("HTTP/1.1 401"));
    assert!(post_to(&entry.url, Some("0000"), init).starts_with("HTTP/1.1 401"));
    let ok = post_to(&entry.url, Some(&entry.token), init);
    assert!(ok.starts_with("HTTP/1.1 200"), "{ok}");
    assert!(ok.contains("Call context first."), "{ok}");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn unclassed_names_the_tools_without_a_class() {
    let server = McpServer::new("test", vec![tools().remove(0), context_tool()]);
    assert_eq!(server.unclassed(), ["echo"], "context is classed on its own");
    assert_eq!(server.class_of(CONTEXT_TOOL), Some(ToolClass::Read));
    let server = server.classify(ToolClass::Edit, &["echo"]);
    assert!(server.unclassed().is_empty());
    assert_eq!(server.class_of("echo"), Some(ToolClass::Edit));
}
