use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════
// JSON-RPC 2.0 — MCP transport layer
// ═══════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: RequestId,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestId {
    Number(i64),
    String(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: RequestId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl JsonRpcResponse {
    pub fn success(id: RequestId, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: RequestId, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }
}

// ═══════════════════════════════════════════════════════════
// MCP protocol messages
// ═══════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpInitializeResult {
    pub protocol_version: String,
    pub capabilities: ServerCapabilities,
    pub server_info: ServerInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerCapabilities {
    pub tools: ToolCapabilities,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCapabilities {
    pub list_changed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

impl Default for McpInitializeResult {
    fn default() -> Self {
        Self {
            protocol_version: "2024-11-05".into(),
            capabilities: ServerCapabilities {
                tools: ToolCapabilities {
                    list_changed: false,
                },
            },
            server_info: ServerInfo {
                name: "reach".into(),
                version: env!("CARGO_PKG_VERSION").into(),
            },
        }
    }
}

// ═══════════════════════════════════════════════════════════
// Tool definitions — the contract between agent and sandbox
// ═══════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

/// All tools exposed by reach. Each variant maps to a sandbox operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "tool", content = "params")]
pub enum ToolCall {
    #[serde(rename = "screenshot")]
    Screenshot(ScreenshotParams),
    #[serde(rename = "click")]
    Click(ClickParams),
    #[serde(rename = "type")]
    Type(TypeParams),
    #[serde(rename = "key")]
    Key(KeyParams),
    #[serde(rename = "browse")]
    Browse(BrowseParams),
    #[serde(rename = "scrape")]
    Scrape(ScrapeParams),
    #[serde(rename = "playwright_eval")]
    PlaywrightEval(PlaywrightEvalParams),
    #[serde(rename = "exec")]
    Exec(ExecParams),
    #[serde(rename = "page_text")]
    PageText(PageTextParams),
    #[serde(rename = "auth_handoff")]
    AuthHandoff(AuthHandoffParams),
    #[serde(rename = "browser_cdp")]
    BrowserCdp(BrowserCdpParams),
    #[serde(rename = "browser_js")]
    BrowserJs(BrowserJsParams),
    #[serde(rename = "browser_click")]
    BrowserClick(BrowserClickParams),
    #[serde(rename = "browser_type")]
    BrowserType(BrowserTypeParams),
    #[serde(rename = "browser_key")]
    BrowserKey(BrowserKeyParams),
    #[serde(rename = "scrape_static")]
    ScrapeStatic(ScrapeStaticParams),
    #[serde(rename = "scrape_agent")]
    ScrapeAgent(ScrapeAgentParams),
    #[serde(rename = "scrape_learn")]
    ScrapeLearn(ScrapeLearnParams),
    #[serde(rename = "scrape_recover")]
    ScrapeRecover(ScrapeRecoverParams),
    #[serde(rename = "scrape_resilient")]
    ScrapeResilient(ScrapeResilientParams),
    #[serde(rename = "stealth_apply")]
    StealthApply(StealthApplyParams),
    #[serde(rename = "scrape_search")]
    ScrapeSearch(ScrapeSearchParams),
}

// ═══════════════════════════════════════════════════════════
// Tool parameter types
// ═══════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenshotParams {
    #[serde(default)]
    pub sandbox: Option<String>,
    #[serde(default = "default_format")]
    pub format: ImageFormat,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ImageFormat {
    #[default]
    Png,
    Jpeg,
}

fn default_format() -> ImageFormat {
    ImageFormat::Png
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClickParams {
    pub x: i32,
    pub y: i32,
    #[serde(default)]
    pub button: MouseButton,
    #[serde(default)]
    pub sandbox: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum MouseButton {
    #[default]
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeParams {
    pub text: String,
    #[serde(default)]
    pub sandbox: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyParams {
    pub combo: String,
    #[serde(default)]
    pub sandbox: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowseParams {
    pub url: String,
    #[serde(default = "default_true")]
    pub headed: bool,
    #[serde(default)]
    pub sandbox: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapeParams {
    pub url: String,
    pub selector: String,
    #[serde(default)]
    pub extract: ExtractMode,
    #[serde(default = "default_true")]
    pub stealth: bool,
    #[serde(default)]
    pub sandbox: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ExtractMode {
    #[default]
    Text,
    Html,
    /// Extract a specific attribute value
    #[serde(rename = "attr")]
    Attribute {
        name: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaywrightEvalParams {
    pub script: String,
    #[serde(default)]
    pub sandbox: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecParams {
    pub command: String,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default = "default_timeout")]
    pub timeout: u32,
    #[serde(default)]
    pub sandbox: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageTextParams {
    pub url: String,
    #[serde(default)]
    pub wait_for: Option<String>,
    #[serde(default)]
    pub selector: Option<String>,
    #[serde(default = "default_page_text_timeout")]
    pub timeout_ms: u64,
    #[serde(default)]
    pub use_profile: Option<String>,
    #[serde(default)]
    pub sandbox: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthHandoffParams {
    pub url: String,
    #[serde(default)]
    pub wait_for_selector: Option<String>,
    #[serde(default)]
    pub wait_for_url_contains: Option<String>,
    #[serde(default = "default_auth_timeout_seconds")]
    pub timeout_seconds: u64,
    #[serde(default)]
    pub use_profile: Option<String>,
    #[serde(default)]
    pub sandbox: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserCdpParams {
    pub method: String,
    #[serde(default)]
    pub params: Option<serde_json::Value>,
    #[serde(default)]
    pub sandbox: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserJsParams {
    pub expression: String,
    #[serde(default)]
    pub sandbox: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserClickParams {
    pub x: i32,
    pub y: i32,
    #[serde(default)]
    pub sandbox: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserTypeParams {
    pub text: String,
    #[serde(default)]
    pub sandbox: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserKeyParams {
    pub key: String,
    #[serde(default)]
    pub sandbox: Option<String>,
}

/// Proxy override accepted by `scrape_*` tools.
///
/// Used today only by [`ScrapeStaticParams`]. The CDP-backed paths accept the
/// same shape but the proxy is currently ignored (Step 3 lights it up via
/// `Target.createBrowserContext`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapeProxyParams {
    pub url: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapeStaticParams {
    pub url: String,
    #[serde(default)]
    pub proxy: Option<ScrapeProxyParams>,
    #[serde(default)]
    pub sandbox: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapeAgentParams {
    pub url: String,
    #[serde(default)]
    pub proxy: Option<ScrapeProxyParams>,
    #[serde(default = "default_true")]
    pub escalate: bool,
    /// Optional stealth profile id to apply before navigation
    /// (e.g. `"windows-chrome-128"`).
    #[serde(default)]
    pub stealth: Option<String>,
    #[serde(default)]
    pub sandbox: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapeLearnParams {
    pub url: String,
    pub selector: String,
    #[serde(default = "default_true")]
    pub navigate: bool,
    #[serde(default)]
    pub sandbox: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapeRecoverParams {
    pub url: String,
    #[serde(default)]
    pub selector: Option<String>,
    #[serde(default)]
    pub sandbox: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapeResilientParams {
    pub url: String,
    pub selector: String,
    #[serde(default)]
    pub extract: serde_json::Value,
    #[serde(default = "default_true")]
    pub navigate: bool,
    #[serde(default)]
    pub validate: serde_json::Value,
    /// Optional stealth profile id to apply before navigation.
    #[serde(default)]
    pub stealth: Option<String>,
    /// Optional per-call proxy override. Mints a fresh CDP browser context.
    #[serde(default)]
    pub proxy: Option<ScrapeProxyParams>,
    #[serde(default)]
    pub sandbox: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StealthApplyParams {
    /// Built-in profile id (`windows-chrome-128`, `mac-chrome-128`,
    /// `linux-chrome-128`).
    pub profile: String,
    #[serde(default)]
    pub sandbox: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapeSearchParams {
    pub query: String,
    /// Backend id. `ddg` is the only one wired today.
    #[serde(default = "default_search_engine")]
    pub engine: String,
    #[serde(default = "default_max_results")]
    pub max_results: usize,
    /// Per-call proxy override; falls back to the server-level default.
    #[serde(default)]
    pub proxy: Option<ScrapeProxyParams>,
}

fn default_search_engine() -> String {
    "ddg".into()
}

fn default_max_results() -> usize {
    10
}

fn default_true() -> bool {
    true
}

fn default_timeout() -> u32 {
    30
}

fn default_page_text_timeout() -> u64 {
    30_000
}

fn default_auth_timeout_seconds() -> u64 {
    300
}

// ═══════════════════════════════════════════════════════════
// Tool results — what comes back from the sandbox
// ═══════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResponse {
    pub content: Vec<ContentBlock>,
    #[serde(default)]
    pub is_error: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
}

impl ToolResponse {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: vec![ContentBlock::Text { text: text.into() }],
            is_error: false,
        }
    }

    pub fn image(data: String, mime_type: impl Into<String>) -> Self {
        Self {
            content: vec![ContentBlock::Image {
                data,
                mime_type: mime_type.into(),
            }],
            is_error: false,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            content: vec![ContentBlock::Text {
                text: message.into(),
            }],
            is_error: true,
        }
    }

    pub fn scrape_result(elements: Vec<ScrapeElement>, final_url: String) -> Self {
        let text = serde_json::to_string_pretty(&ScrapeOutput {
            elements: &elements,
            count: elements.len(),
            final_url,
        })
        .unwrap_or_else(|_| "serialization error".into());

        Self {
            content: vec![ContentBlock::Text { text }],
            is_error: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapeElement {
    pub content: String,
    pub tag: String,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub attributes: HashMap<String, String>,
}

#[derive(Debug, Serialize)]
struct ScrapeOutput<'a> {
    elements: &'a [ScrapeElement],
    count: usize,
    final_url: String,
}

// ═══════════════════════════════════════════════════════════
// SSE transport types
// ═══════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct SseMessage {
    pub event: &'static str,
    pub data: String,
}

impl SseMessage {
    pub fn message(data: &JsonRpcResponse) -> Self {
        Self {
            event: "message",
            data: serde_json::to_string(data).unwrap(),
        }
    }

    pub fn endpoint(uri: &str) -> Self {
        Self {
            event: "endpoint",
            data: uri.into(),
        }
    }
}

// ═══════════════════════════════════════════════════════════
// Tool registry
// ═══════════════════════════════════════════════════════════

pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "screenshot".into(),
            description: "Capture a screenshot of the sandbox display".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "sandbox": { "type": "string", "description": "Sandbox name" },
                    "format": { "type": "string", "enum": ["png", "jpeg"], "default": "png" }
                }
            }),
        },
        ToolDefinition {
            name: "click".into(),
            description: "Click at screen coordinates".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["x", "y"],
                "properties": {
                    "x": { "type": "integer" },
                    "y": { "type": "integer" },
                    "button": { "type": "string", "enum": ["left", "right", "middle"], "default": "left" },
                    "sandbox": { "type": "string" }
                }
            }),
        },
        ToolDefinition {
            name: "type".into(),
            description: "Type text using the keyboard".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["text"],
                "properties": {
                    "text": { "type": "string" },
                    "sandbox": { "type": "string" }
                }
            }),
        },
        ToolDefinition {
            name: "key".into(),
            description: "Press a key combination (e.g., ctrl+c, Return, alt+F4)".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["combo"],
                "properties": {
                    "combo": { "type": "string" },
                    "sandbox": { "type": "string" }
                }
            }),
        },
        ToolDefinition {
            name: "browse".into(),
            description: "Open a URL in Chrome".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["url"],
                "properties": {
                    "url": { "type": "string" },
                    "headed": { "type": "boolean", "default": true },
                    "sandbox": { "type": "string" }
                }
            }),
        },
        ToolDefinition {
            name: "scrape".into(),
            description: "Scrape a webpage with Scrapling (adaptive selectors, anti-bot)".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["url", "selector"],
                "properties": {
                    "url": { "type": "string" },
                    "selector": { "type": "string", "description": "CSS selector" },
                    "extract": { "type": "string", "enum": ["text", "html"], "default": "text" },
                    "stealth": { "type": "boolean", "default": true },
                    "sandbox": { "type": "string" }
                }
            }),
        },
        ToolDefinition {
            name: "playwright_eval".into(),
            description: "Run a Playwright Python script in the sandbox".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["script"],
                "properties": {
                    "script": { "type": "string", "description": "Python script using playwright sync_api" },
                    "sandbox": { "type": "string" }
                }
            }),
        },
        ToolDefinition {
            name: "exec".into(),
            description: "Execute a shell command in the sandbox".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["command"],
                "properties": {
                    "command": { "type": "string" },
                    "cwd": { "type": "string" },
                    "timeout": { "type": "integer", "default": 30 },
                    "sandbox": { "type": "string" }
                }
            }),
        },
        ToolDefinition {
            name: "page_text".into(),
            description:
                "Navigate to a URL using Playwright (real Chromium), wait for the page to render, \
                 and return the visible text content. Handles JS-heavy SPAs that Scrapling can't."
                    .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["url"],
                "properties": {
                    "url": { "type": "string", "description": "URL to load" },
                    "wait_for": {
                        "type": "string",
                        "description": "CSS selector to wait for before extracting (default: networkidle)"
                    },
                    "selector": {
                        "type": "string",
                        "description": "Only extract text from elements matching this selector (default: body)"
                    },
                    "timeout_ms": {
                        "type": "integer",
                        "default": 30000,
                        "description": "Max wait time in milliseconds"
                    },
                    "use_profile": {
                        "type": "string",
                        "description": "Persistent Chrome profile name (see `reach create --persist-profile`)"
                    },
                    "sandbox": { "type": "string" }
                }
            }),
        },
        ToolDefinition {
            name: "auth_handoff".into(),
            description:
                "Open a URL in the sandbox's Chrome and pause until the user has authenticated. \
                 Returns the noVNC URL the user should open to perform login. Optionally polls \
                 for a CSS selector or URL substring that indicates auth is complete."
                    .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["url"],
                "properties": {
                    "url": { "type": "string", "description": "URL that requires auth" },
                    "wait_for_selector": {
                        "type": "string",
                        "description": "CSS selector that appears after successful auth"
                    },
                    "wait_for_url_contains": {
                        "type": "string",
                        "description": "Substring that should appear in the URL after auth"
                    },
                    "timeout_seconds": {
                        "type": "integer",
                        "default": 300,
                        "description": "How long to wait for the auth signal"
                    },
                    "use_profile": {
                        "type": "string",
                        "description": "Persistent Chrome profile name (see `reach create --persist-profile`)"
                    },
                    "sandbox": { "type": "string" }
                }
            }),
        },
        ToolDefinition {
            name: "browser_cdp".into(),
            description: "Send a CDP command directly to the browser".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["method"],
                "properties": {
                    "method": { "type": "string", "description": "CDP method name (e.g. 'Runtime.evaluate')" },
                    "params": { "type": "object", "description": "CDP method parameters" },
                    "sandbox": { "type": "string" }
                }
            }),
        },
        ToolDefinition {
            name: "browser_js".into(),
            description: "Execute JavaScript in the browser via CDP".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["expression"],
                "properties": {
                    "expression": { "type": "string", "description": "JavaScript expression to evaluate" },
                    "sandbox": { "type": "string" }
                }
            }),
        },
        ToolDefinition {
            name: "browser_click".into(),
            description: "Click at screen coordinates using CDP".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["x", "y"],
                "properties": {
                    "x": { "type": "integer" },
                    "y": { "type": "integer" },
                    "sandbox": { "type": "string" }
                }
            }),
        },
        ToolDefinition {
            name: "browser_type".into(),
            description: "Type text using CDP".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["text"],
                "properties": {
                    "text": { "type": "string" },
                    "sandbox": { "type": "string" }
                }
            }),
        },
        ToolDefinition {
            name: "browser_key".into(),
            description: "Press a key using CDP".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["key"],
                "properties": {
                    "key": { "type": "string" },
                    "sandbox": { "type": "string" }
                }
            }),
        },
        ToolDefinition {
            name: "scrape_static".into(),
            description: "Fetch a URL with the static HTTP scraper (no browser). \
                Returns rendered HTML and metadata. Optional proxy is honored."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["url"],
                "properties": {
                    "url": { "type": "string" },
                    "proxy": SCRAPE_PROXY_SCHEMA.clone(),
                    "sandbox": { "type": "string" }
                }
            }),
        },
        ToolDefinition {
            name: "scrape_agent".into(),
            description: "Hybrid fetch: static HTTP first, escalate to CDP browser \
                on 403/429 and forward solved cookies back to the static client. \
                Optionally applies a stealth profile to the CDP target before \
                escalation."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["url"],
                "properties": {
                    "url": { "type": "string" },
                    "proxy": SCRAPE_PROXY_SCHEMA.clone(),
                    "escalate": { "type": "boolean", "default": true },
                    "stealth": { "type": "string",
                        "description": "Built-in profile id (windows-chrome-128, mac-chrome-128, linux-chrome-128)" },
                    "sandbox": { "type": "string" }
                }
            }),
        },
        ToolDefinition {
            name: "scrape_learn".into(),
            description: "Capture an element fingerprint (DOM path, text hash, \
                bbox) for a CSS selector via CDP and persist it to the host's \
                AdaptiveMemory store for later self-healing recovery."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["url", "selector"],
                "properties": {
                    "url": { "type": "string" },
                    "selector": { "type": "string" },
                    "navigate": { "type": "boolean", "default": true,
                        "description": "Navigate to `url` first; set false if the \
                            page is already loaded in the sandbox." },
                    "sandbox": { "type": "string" }
                }
            }),
        },
        ToolDefinition {
            name: "scrape_recover".into(),
            description: "Look up AdaptiveMemory candidates for a URL (by domain + \
                path). Optionally filter by original selector. Returns a ranked \
                list; the actual repair attempt lives in `scrape_resilient`."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["url"],
                "properties": {
                    "url": { "type": "string" },
                    "selector": { "type": "string" },
                    "sandbox": { "type": "string" }
                }
            }),
        },
        ToolDefinition {
            name: "scrape_resilient".into(),
            description: "Self-healing extract loop: navigate, try selector, \
                validate (non-empty + optional regex), and on miss fall back to \
                AdaptiveMemory candidates ranked by text-hash → dom-path → bbox. \
                On repair success the candidate's successful_uses is incremented \
                and a fresh fingerprint is persisted under the original \
                selector."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["url", "selector"],
                "properties": {
                    "url": { "type": "string" },
                    "selector": { "type": "string" },
                    "navigate": { "type": "boolean", "default": true },
                    "extract": {
                        "oneOf": [
                            { "const": "text" },
                            { "const": "html" },
                            {
                                "type": "object",
                                "required": ["attr"],
                                "properties": {
                                    "attr": {
                                        "type": "object",
                                        "required": ["name"],
                                        "properties": { "name": { "type": "string" } }
                                    }
                                }
                            }
                        ],
                        "default": "text"
                    },
                    "validate": {
                        "type": "object",
                        "properties": {
                            "non_empty": { "type": "boolean", "default": true },
                            "matches": { "type": "string",
                                "description": "JS-flavored regex applied to the value" }
                        }
                    },
                    "stealth": { "type": "string",
                        "description": "Built-in profile id; applied before navigation" },
                    "sandbox": { "type": "string" }
                }
            }),
        },
        ToolDefinition {
            name: "stealth_apply".into(),
            description: "Apply a built-in browser fingerprint profile to the \
                sandbox: UA + sec-ch-ua hints, hardware concurrency, locale, \
                timezone, screen / device metrics, plus a Page.addScriptToEvaluate \
                shim that normalizes navigator.webdriver, plugins/mimeTypes, \
                permissions.query, WebGL VENDOR/RENDERER, screen.*, \
                navigator.deviceMemory, and window.chrome.runtime."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["profile"],
                "properties": {
                    "profile": { "type": "string",
                        "enum": ["windows-chrome-128", "mac-chrome-128", "linux-chrome-128"] },
                    "sandbox": { "type": "string" }
                }
            }),
        },
        ToolDefinition {
            name: "scrape_search".into(),
            description: "Free no-captcha web search via DuckDuckGo HTML. \
                Hits the static fetcher (no browser), parses the static HTML, \
                returns [{title, url, snippet}]. Use this instead of \
                Google/Bing scraping for general web search — DDG HTML has \
                no JS challenge, no captcha, and tolerates ~10 req/s per IP."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["query"],
                "properties": {
                    "query": { "type": "string" },
                    "engine": { "type": "string", "enum": ["ddg"], "default": "ddg" },
                    "max_results": { "type": "integer", "default": 10, "minimum": 1, "maximum": 50 },
                    "proxy": SCRAPE_PROXY_SCHEMA.clone()
                }
            }),
        },
    ]
}

static SCRAPE_PROXY_SCHEMA: std::sync::LazyLock<serde_json::Value> =
    std::sync::LazyLock::new(|| {
        serde_json::json!({
            "type": "object",
            "required": ["url"],
            "properties": {
                "url": { "type": "string", "description": "Proxy URL, e.g. http://host:port" },
                "username": { "type": "string" },
                "password": { "type": "string" }
            }
        })
    });

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_includes_new_tools() {
        let names: Vec<String> = tool_definitions().into_iter().map(|t| t.name).collect();
        assert!(names.contains(&"page_text".to_string()));
        assert!(names.contains(&"auth_handoff".to_string()));
    }

    #[test]
    fn page_text_schema_marks_url_required() {
        let tool = tool_definitions()
            .into_iter()
            .find(|t| t.name == "page_text")
            .unwrap();
        let required = tool
            .input_schema
            .get("required")
            .unwrap()
            .as_array()
            .unwrap();
        assert!(required.iter().any(|v| v == "url"));
    }

    #[test]
    fn auth_handoff_schema_marks_url_required() {
        let tool = tool_definitions()
            .into_iter()
            .find(|t| t.name == "auth_handoff")
            .unwrap();
        let required = tool
            .input_schema
            .get("required")
            .unwrap()
            .as_array()
            .unwrap();
        assert!(required.iter().any(|v| v == "url"));
    }
}
