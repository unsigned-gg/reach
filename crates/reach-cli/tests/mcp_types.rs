//! Unit tests for MCP protocol types — serialization round-trips,
//! tool definition completeness, and wire format compliance.

use reach_cli::mcp::*;

// ═══════════════════════════════════════════════════════════
// JSON-RPC wire format
// ═══════════════════════════════════════════════════════════

#[test]
fn jsonrpc_response_success_has_no_error_field() {
    let resp = JsonRpcResponse::success(RequestId::Number(1), serde_json::json!({"status": "ok"}));
    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains("\"result\""));
    assert!(!json.contains("\"error\""));
}

#[test]
fn jsonrpc_response_error_has_no_result_field() {
    let resp = JsonRpcResponse::error(RequestId::Number(1), -32600, "invalid request");
    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains("\"error\""));
    assert!(!json.contains("\"result\""));
}

#[test]
fn jsonrpc_request_id_can_be_string_or_number() {
    let num: RequestId = serde_json::from_str("42").unwrap();
    assert!(matches!(num, RequestId::Number(42)));

    let s: RequestId = serde_json::from_str("\"abc\"").unwrap();
    assert!(matches!(s, RequestId::String(ref v) if v == "abc"));
}

#[test]
fn jsonrpc_request_roundtrips() {
    let req = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: RequestId::Number(7),
        method: "tools/call".into(),
        params: serde_json::json!({"name": "screenshot"}),
    };
    let json = serde_json::to_string(&req).unwrap();
    let back: JsonRpcRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(back.method, "tools/call");
}

// ═══════════════════════════════════════════════════════════
// MCP initialization
// ═══════════════════════════════════════════════════════════

#[test]
fn mcp_initialize_default_has_correct_protocol_version() {
    let init = McpInitializeResult::default();
    assert_eq!(init.protocol_version, "2024-11-05");
    assert_eq!(init.server_info.name, "reach");
}

// ═══════════════════════════════════════════════════════════
// Tool definitions — completeness and schema correctness
// ═══════════════════════════════════════════════════════════

#[test]
fn all_tools_are_registered() {
    let tools = tool_definitions();
    assert_eq!(tools.len(), 22);

    let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    assert!(names.contains(&"screenshot"));
    assert!(names.contains(&"click"));
    assert!(names.contains(&"type"));
    assert!(names.contains(&"key"));
    assert!(names.contains(&"browse"));
    assert!(names.contains(&"scrape"));
    assert!(names.contains(&"playwright_eval"));
    assert!(names.contains(&"exec"));
    assert!(names.contains(&"page_text"));
    assert!(names.contains(&"auth_handoff"));
    assert!(names.contains(&"browser_cdp"));
    assert!(names.contains(&"browser_js"));
    assert!(names.contains(&"browser_click"));
    assert!(names.contains(&"browser_type"));
    assert!(names.contains(&"browser_key"));
    assert!(names.contains(&"scrape_static"));
    assert!(names.contains(&"scrape_agent"));
    assert!(names.contains(&"scrape_learn"));
    assert!(names.contains(&"scrape_recover"));
    assert!(names.contains(&"scrape_resilient"));
    assert!(names.contains(&"stealth_apply"));
    assert!(names.contains(&"scrape_search"));
}

#[test]
fn page_text_tool_requires_url() {
    let tools = tool_definitions();
    let pt = tools.iter().find(|t| t.name == "page_text").unwrap();
    let required = pt.input_schema.get("required").unwrap();
    let required: Vec<String> = serde_json::from_value(required.clone()).unwrap();
    assert!(required.contains(&"url".into()));
}

#[test]
fn auth_handoff_tool_requires_url() {
    let tools = tool_definitions();
    let ah = tools.iter().find(|t| t.name == "auth_handoff").unwrap();
    let required = ah.input_schema.get("required").unwrap();
    let required: Vec<String> = serde_json::from_value(required.clone()).unwrap();
    assert!(required.contains(&"url".into()));
}

#[test]
fn page_text_params_default_timeout_is_30s() {
    let params: PageTextParams = serde_json::from_str(r#"{"url": "https://example.com"}"#).unwrap();
    assert_eq!(params.timeout_ms, 30_000);
}

#[test]
fn auth_handoff_params_default_timeout_is_300s() {
    let params: AuthHandoffParams =
        serde_json::from_str(r#"{"url": "https://example.com"}"#).unwrap();
    assert_eq!(params.timeout_seconds, 300);
}

#[test]
fn tool_schemas_are_valid_json_schema_objects() {
    for tool in tool_definitions() {
        let schema = &tool.input_schema;
        assert_eq!(
            schema.get("type").and_then(|v| v.as_str()),
            Some("object"),
            "tool {} schema must be type: object",
            tool.name
        );
        assert!(
            schema.get("properties").is_some(),
            "tool {} schema must have properties",
            tool.name
        );
    }
}

#[test]
fn click_tool_requires_x_and_y() {
    let tools = tool_definitions();
    let click = tools.iter().find(|t| t.name == "click").unwrap();
    let required = click.input_schema.get("required").unwrap();
    let required: Vec<String> = serde_json::from_value(required.clone()).unwrap();
    assert!(required.contains(&"x".into()));
    assert!(required.contains(&"y".into()));
}

#[test]
fn scrape_tool_requires_url_and_selector() {
    let tools = tool_definitions();
    let scrape = tools.iter().find(|t| t.name == "scrape").unwrap();
    let required = scrape.input_schema.get("required").unwrap();
    let required: Vec<String> = serde_json::from_value(required.clone()).unwrap();
    assert!(required.contains(&"url".into()));
    assert!(required.contains(&"selector".into()));
}

#[test]
fn exec_tool_requires_command() {
    let tools = tool_definitions();
    let exec = tools.iter().find(|t| t.name == "exec").unwrap();
    let required = exec.input_schema.get("required").unwrap();
    let required: Vec<String> = serde_json::from_value(required.clone()).unwrap();
    assert!(required.contains(&"command".into()));
}

#[test]
fn screenshot_tool_has_no_required_fields() {
    let tools = tool_definitions();
    let ss = tools.iter().find(|t| t.name == "screenshot").unwrap();
    assert!(ss.input_schema.get("required").is_none());
}

// ═══════════════════════════════════════════════════════════
// Tool parameter serialization
// ═══════════════════════════════════════════════════════════

#[test]
fn screenshot_params_default_format_is_png() {
    let params: ScreenshotParams = serde_json::from_str("{}").unwrap();
    assert!(matches!(params.format, ImageFormat::Png));
}

#[test]
fn click_params_default_button_is_left() {
    let params: ClickParams = serde_json::from_str(r#"{"x": 100, "y": 200}"#).unwrap();
    assert!(matches!(params.button, MouseButton::Left));
    assert_eq!(params.x, 100);
    assert_eq!(params.y, 200);
}

#[test]
fn browse_params_default_headed_is_true() {
    let params: BrowseParams = serde_json::from_str(r#"{"url": "https://example.com"}"#).unwrap();
    assert!(params.headed);
}

#[test]
fn scrape_params_default_stealth_is_true() {
    let params: ScrapeParams =
        serde_json::from_str(r#"{"url": "https://example.com", "selector": "h1"}"#).unwrap();
    assert!(params.stealth);
}

#[test]
fn exec_params_default_timeout_is_30() {
    let params: ExecParams = serde_json::from_str(r#"{"command": "ls"}"#).unwrap();
    assert_eq!(params.timeout, 30);
}

#[test]
fn exec_params_custom_timeout() {
    let params: ExecParams =
        serde_json::from_str(r#"{"command": "sleep 60", "timeout": 120}"#).unwrap();
    assert_eq!(params.timeout, 120);
}

// ═══════════════════════════════════════════════════════════
// Tool response construction
// ═══════════════════════════════════════════════════════════

#[test]
fn tool_response_text_is_not_error() {
    let resp = ToolResponse::text("hello");
    assert!(!resp.is_error);
    assert_eq!(resp.content.len(), 1);
}

#[test]
fn tool_response_error_has_is_error_true() {
    let resp = ToolResponse::error("something broke");
    assert!(resp.is_error);
}

#[test]
fn tool_response_image_carries_mime_type() {
    let resp = ToolResponse::image("base64data".into(), "image/png");
    assert!(!resp.is_error);
    match &resp.content[0] {
        ContentBlock::Image { mime_type, .. } => assert_eq!(mime_type, "image/png"),
        _ => panic!("expected image content block"),
    }
}

#[test]
fn tool_response_scrape_result_serializes_as_json() {
    let resp = ToolResponse::scrape_result(
        vec![ScrapeElement {
            content: "Hello World".into(),
            tag: "h1".into(),
            attributes: Default::default(),
        }],
        "https://example.com".into(),
    );
    match &resp.content[0] {
        ContentBlock::Text { text } => {
            let parsed: serde_json::Value = serde_json::from_str(text).unwrap();
            assert_eq!(parsed["count"], 1);
            assert_eq!(parsed["elements"][0]["content"], "Hello World");
        }
        _ => panic!("expected text content block"),
    }
}

// ═══════════════════════════════════════════════════════════
// SSE message construction
// ═══════════════════════════════════════════════════════════

#[test]
fn sse_endpoint_message_carries_uri() {
    let msg = SseMessage::endpoint("/mcp/v1");
    assert_eq!(msg.event, "endpoint");
    assert_eq!(msg.data, "/mcp/v1");
}

#[test]
fn sse_message_wraps_jsonrpc_response() {
    let resp = JsonRpcResponse::success(RequestId::Number(1), serde_json::json!({"tools": []}));
    let msg = SseMessage::message(&resp);
    assert_eq!(msg.event, "message");
    assert!(msg.data.contains("\"jsonrpc\""));
}
