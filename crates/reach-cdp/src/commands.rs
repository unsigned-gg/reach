//! Typed command and response payloads for the subset of CDP used by Reach.

use crate::CdpCommand;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// `Page.navigate` command builder.
#[derive(Debug, Clone)]
pub struct PageNavigate {
    params: PageNavigateParams,
}

impl PageNavigate {
    /// Create a navigation command for the given URL.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            params: PageNavigateParams {
                url: url.into(),
                referrer: None,
                transition_type: None,
                frame_id: None,
                referrer_policy: None,
            },
        }
    }

    /// Set the navigation referrer.
    pub fn with_referrer(mut self, referrer: impl Into<String>) -> Self {
        self.params.referrer = Some(referrer.into());
        self
    }

    /// Set the navigation transition type.
    pub fn with_transition_type(mut self, transition_type: impl Into<String>) -> Self {
        self.params.transition_type = Some(transition_type.into());
        self
    }

    /// Set the frame to navigate.
    pub fn with_frame_id(mut self, frame_id: impl Into<String>) -> Self {
        self.params.frame_id = Some(frame_id.into());
        self
    }

    /// Set the referrer policy for the navigation.
    pub fn with_referrer_policy(mut self, referrer_policy: impl Into<String>) -> Self {
        self.params.referrer_policy = Some(referrer_policy.into());
        self
    }
}

impl CdpCommand for PageNavigate {
    type Params = PageNavigateParams;

    fn method(&self) -> &'static str {
        "Page.navigate"
    }

    fn params(&self) -> &Self::Params {
        &self.params
    }
}

/// Parameters for `Page.navigate`.
#[derive(Debug, Clone, Serialize)]
pub struct PageNavigateParams {
    /// URL to navigate to.
    pub url: String,
    /// Optional referrer URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub referrer: Option<String>,
    /// Optional CDP transition type.
    #[serde(rename = "transitionType", skip_serializing_if = "Option::is_none")]
    pub transition_type: Option<String>,
    /// Optional target frame ID.
    #[serde(rename = "frameId", skip_serializing_if = "Option::is_none")]
    pub frame_id: Option<String>,
    /// Optional referrer policy.
    #[serde(rename = "referrerPolicy", skip_serializing_if = "Option::is_none")]
    pub referrer_policy: Option<String>,
}

/// Result returned by `Page.navigate`.
#[derive(Debug, Clone, Deserialize)]
pub struct PageNavigateResult {
    /// Navigated frame ID.
    #[serde(rename = "frameId")]
    pub frame_id: String,
    /// Loader ID for the new document, when one was created.
    #[serde(rename = "loaderId")]
    pub loader_id: Option<String>,
    /// Navigation failure text, when Chromium rejected the navigation.
    #[serde(rename = "errorText")]
    pub error_text: Option<String>,
    /// Whether the navigation resulted in a download.
    #[serde(rename = "isDownload")]
    pub is_download: Option<bool>,
}

/// `Runtime.evaluate` command builder.
#[derive(Debug, Clone)]
pub struct RuntimeEvaluate {
    params: RuntimeEvaluateParams,
}

impl RuntimeEvaluate {
    /// Create an evaluation command for a JavaScript expression.
    pub fn new(expression: impl Into<String>) -> Self {
        Self {
            params: RuntimeEvaluateParams {
                expression: expression.into(),
                object_group: None,
                include_command_line_api: None,
                silent: None,
                context_id: None,
                return_by_value: None,
                generate_preview: None,
                user_gesture: None,
                await_promise: None,
                throw_on_side_effect: None,
                timeout: None,
                disable_breaks: None,
                repl_mode: None,
                allow_unsafe_eval_blocked_by_csp: None,
                unique_context_id: None,
            },
        }
    }

    /// Request that Chromium return JSON-serializable values by value.
    pub fn with_return_by_value(mut self, return_by_value: bool) -> Self {
        self.params.return_by_value = Some(return_by_value);
        self
    }

    /// Request that Chromium await a returned promise.
    pub fn with_await_promise(mut self, await_promise: bool) -> Self {
        self.params.await_promise = Some(await_promise);
        self
    }

    /// Set the evaluation timeout in milliseconds.
    pub fn with_timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.params.timeout = Some(timeout_ms);
        self
    }

    /// Mark the evaluation as initiated by a user gesture.
    pub fn with_user_gesture(mut self, user_gesture: bool) -> Self {
        self.params.user_gesture = Some(user_gesture);
        self
    }
}

impl CdpCommand for RuntimeEvaluate {
    type Params = RuntimeEvaluateParams;

    fn method(&self) -> &'static str {
        "Runtime.evaluate"
    }

    fn params(&self) -> &Self::Params {
        &self.params
    }
}

/// Parameters for `Runtime.evaluate`.
#[derive(Debug, Clone, Serialize)]
pub struct RuntimeEvaluateParams {
    /// JavaScript expression to evaluate.
    pub expression: String,
    /// Optional object group for remote handles.
    #[serde(rename = "objectGroup", skip_serializing_if = "Option::is_none")]
    pub object_group: Option<String>,
    /// Whether to expose command-line API helpers.
    #[serde(
        rename = "includeCommandLineAPI",
        skip_serializing_if = "Option::is_none"
    )]
    pub include_command_line_api: Option<bool>,
    /// Whether evaluation should be silent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub silent: Option<bool>,
    /// Execution context ID.
    #[serde(rename = "contextId", skip_serializing_if = "Option::is_none")]
    pub context_id: Option<i64>,
    /// Whether to return the result by value.
    #[serde(rename = "returnByValue", skip_serializing_if = "Option::is_none")]
    pub return_by_value: Option<bool>,
    /// Whether to generate a preview for remote objects.
    #[serde(rename = "generatePreview", skip_serializing_if = "Option::is_none")]
    pub generate_preview: Option<bool>,
    /// Whether the evaluation should count as a user gesture.
    #[serde(rename = "userGesture", skip_serializing_if = "Option::is_none")]
    pub user_gesture: Option<bool>,
    /// Whether to await a returned promise.
    #[serde(rename = "awaitPromise", skip_serializing_if = "Option::is_none")]
    pub await_promise: Option<bool>,
    /// Whether side effects should be rejected.
    #[serde(rename = "throwOnSideEffect", skip_serializing_if = "Option::is_none")]
    pub throw_on_side_effect: Option<bool>,
    /// Timeout in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    /// Whether breakpoints should be disabled during evaluation.
    #[serde(rename = "disableBreaks", skip_serializing_if = "Option::is_none")]
    pub disable_breaks: Option<bool>,
    /// Whether to use REPL mode.
    #[serde(rename = "replMode", skip_serializing_if = "Option::is_none")]
    pub repl_mode: Option<bool>,
    /// Whether unsafe eval may run when blocked by CSP.
    #[serde(
        rename = "allowUnsafeEvalBlockedByCSP",
        skip_serializing_if = "Option::is_none"
    )]
    pub allow_unsafe_eval_blocked_by_csp: Option<bool>,
    /// Unique execution context ID.
    #[serde(rename = "uniqueContextId", skip_serializing_if = "Option::is_none")]
    pub unique_context_id: Option<String>,
}

/// Result returned by `Runtime.evaluate`.
#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeEvaluateResult {
    /// Remote object describing the evaluation result.
    pub result: RemoteObject,
    /// Exception details, when evaluation threw.
    #[serde(rename = "exceptionDetails")]
    pub exception_details: Option<RuntimeExceptionDetails>,
}

/// CDP runtime remote object descriptor.
#[derive(Debug, Clone, Deserialize)]
pub struct RemoteObject {
    /// CDP object type.
    #[serde(rename = "type")]
    pub object_type: String,
    /// Optional subtype, such as `array` or `node`.
    pub subtype: Option<String>,
    /// Optional JavaScript class name.
    #[serde(rename = "className")]
    pub class_name: Option<String>,
    /// JSON value when returned by value.
    pub value: Option<Value>,
    /// Non-JSON primitive representation.
    #[serde(rename = "unserializableValue")]
    pub unserializable_value: Option<String>,
    /// Human-readable object description.
    pub description: Option<String>,
    /// Remote object ID for by-reference results.
    #[serde(rename = "objectId")]
    pub object_id: Option<String>,
}

/// Exception information returned by `Runtime.evaluate`.
#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeExceptionDetails {
    /// CDP exception ID.
    #[serde(rename = "exceptionId")]
    pub exception_id: i64,
    /// Exception summary text.
    pub text: String,
    /// Zero-based line number.
    #[serde(rename = "lineNumber")]
    pub line_number: i64,
    /// Zero-based column number.
    #[serde(rename = "columnNumber")]
    pub column_number: i64,
    /// Optional script ID.
    #[serde(rename = "scriptId")]
    pub script_id: Option<String>,
    /// Optional script URL.
    pub url: Option<String>,
    /// Optional stack trace object.
    #[serde(rename = "stackTrace")]
    pub stack_trace: Option<Value>,
    /// Optional exception object.
    pub exception: Option<RemoteObject>,
}

/// `Network.enable` command builder.
#[derive(Debug, Clone, Default)]
pub struct NetworkEnable {
    params: NetworkEnableParams,
}

impl NetworkEnable {
    /// Create a `Network.enable` command with default parameters.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the maximum total network buffer size.
    pub fn with_max_total_buffer_size(mut self, max_total_buffer_size: u64) -> Self {
        self.params.max_total_buffer_size = Some(max_total_buffer_size);
        self
    }

    /// Set the maximum per-resource buffer size.
    pub fn with_max_resource_buffer_size(mut self, max_resource_buffer_size: u64) -> Self {
        self.params.max_resource_buffer_size = Some(max_resource_buffer_size);
        self
    }

    /// Set the maximum POST body size preserved by Chromium.
    pub fn with_max_post_data_size(mut self, max_post_data_size: u64) -> Self {
        self.params.max_post_data_size = Some(max_post_data_size);
        self
    }
}

impl CdpCommand for NetworkEnable {
    type Params = NetworkEnableParams;

    fn method(&self) -> &'static str {
        "Network.enable"
    }

    fn params(&self) -> &Self::Params {
        &self.params
    }
}

/// Parameters for `Network.enable`.
#[derive(Debug, Clone, Default, Serialize)]
pub struct NetworkEnableParams {
    /// Maximum total buffer size.
    #[serde(rename = "maxTotalBufferSize", skip_serializing_if = "Option::is_none")]
    pub max_total_buffer_size: Option<u64>,
    /// Maximum buffer size per resource.
    #[serde(
        rename = "maxResourceBufferSize",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_resource_buffer_size: Option<u64>,
    /// Maximum POST body size.
    #[serde(rename = "maxPostDataSize", skip_serializing_if = "Option::is_none")]
    pub max_post_data_size: Option<u64>,
}

/// Empty result returned by `Network.enable`.
#[derive(Debug, Clone, Deserialize)]
pub struct NetworkEnableResult {}

/// `Network.getCookies` command builder.
#[derive(Debug, Clone, Default)]
pub struct NetworkGetCookies {
    params: NetworkGetCookiesParams,
}

impl NetworkGetCookies {
    /// Create a command that reads cookies for all known URLs.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a command that reads cookies for specific URLs.
    pub fn for_urls(urls: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            params: NetworkGetCookiesParams {
                urls: Some(urls.into_iter().map(Into::into).collect()),
            },
        }
    }
}

impl CdpCommand for NetworkGetCookies {
    type Params = NetworkGetCookiesParams;

    fn method(&self) -> &'static str {
        "Network.getCookies"
    }

    fn params(&self) -> &Self::Params {
        &self.params
    }
}

/// Parameters for `Network.getCookies`.
#[derive(Debug, Clone, Default, Serialize)]
pub struct NetworkGetCookiesParams {
    /// URLs whose applicable cookies should be returned.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub urls: Option<Vec<String>>,
}

/// Result returned by `Network.getCookies`.
#[derive(Debug, Clone, Deserialize)]
pub struct NetworkGetCookiesResult {
    /// Cookies visible to the current browser context.
    pub cookies: Vec<Cookie>,
}

/// Cookie object returned by CDP.
#[derive(Debug, Clone, Deserialize)]
pub struct Cookie {
    /// Cookie name.
    pub name: String,
    /// Cookie value.
    pub value: String,
    /// Cookie domain.
    pub domain: String,
    /// Cookie path.
    pub path: String,
    /// Expiration timestamp.
    pub expires: f64,
    /// Cookie size in bytes.
    pub size: i64,
    /// Whether the cookie is HTTP-only.
    #[serde(rename = "httpOnly")]
    pub http_only: bool,
    /// Whether the cookie is secure-only.
    pub secure: bool,
    /// Whether the cookie is a session cookie.
    pub session: bool,
    /// SameSite policy.
    #[serde(rename = "sameSite")]
    pub same_site: Option<CookieSameSite>,
    /// Cookie priority.
    pub priority: Option<CookiePriority>,
    /// Whether the cookie is same-party.
    #[serde(rename = "sameParty")]
    pub same_party: Option<bool>,
    /// Source scheme.
    #[serde(rename = "sourceScheme")]
    pub source_scheme: Option<CookieSourceScheme>,
    /// Source port.
    #[serde(rename = "sourcePort")]
    pub source_port: Option<i64>,
    /// Partition key object.
    #[serde(rename = "partitionKey")]
    pub partition_key: Option<serde_json::Value>,
    /// Whether the partition key is opaque.
    #[serde(rename = "partitionKeyOpaque")]
    pub partition_key_opaque: Option<bool>,
}

/// Cookie SameSite values returned by CDP.
#[derive(Debug, Clone, Deserialize)]
pub enum CookieSameSite {
    /// Strict SameSite policy.
    Strict,
    /// Lax SameSite policy.
    Lax,
    /// No SameSite restrictions.
    None,
}

/// Cookie priority values returned by CDP.
#[derive(Debug, Clone, Deserialize)]
pub enum CookiePriority {
    /// Low priority.
    Low,
    /// Medium priority.
    Medium,
    /// High priority.
    High,
}

/// Cookie source scheme values returned by CDP.
#[derive(Debug, Clone, Deserialize)]
pub enum CookieSourceScheme {
    /// Source scheme is unset.
    Unset,
    /// Cookie was set from a non-secure source.
    NonSecure,
    /// Cookie was set from a secure source.
    Secure,
}

/// `Network.getResponseBody` command builder.
#[derive(Debug, Clone)]
pub struct NetworkGetResponseBody {
    params: NetworkGetResponseBodyParams,
}

impl NetworkGetResponseBody {
    /// Create a command for a CDP network request ID.
    pub fn new(request_id: impl Into<String>) -> Self {
        Self {
            params: NetworkGetResponseBodyParams {
                request_id: request_id.into(),
            },
        }
    }
}

impl CdpCommand for NetworkGetResponseBody {
    type Params = NetworkGetResponseBodyParams;

    fn method(&self) -> &'static str {
        "Network.getResponseBody"
    }

    fn params(&self) -> &Self::Params {
        &self.params
    }
}

/// Parameters for `Network.getResponseBody`.
#[derive(Debug, Clone, Serialize)]
pub struct NetworkGetResponseBodyParams {
    /// CDP request ID.
    #[serde(rename = "requestId")]
    pub request_id: String,
}

/// Result returned by `Network.getResponseBody`.
#[derive(Debug, Clone, Deserialize)]
pub struct NetworkGetResponseBodyResult {
    /// Response body text or base64 data.
    pub body: String,
    /// Whether `body` is base64-encoded.
    #[serde(rename = "base64Encoded")]
    pub base64_encoded: bool,
}

// ──────────────────────────────────────────────────────────────────────
// Stealth / fingerprint emulation
// ──────────────────────────────────────────────────────────────────────

/// `Page.addScriptToEvaluateOnNewDocument` — installs a JS snippet that runs
/// on every new document **before** any page script. The standard CDP hook for
/// fingerprint shimming.
#[derive(Debug, Clone)]
pub struct PageAddScriptToEvaluateOnNewDocument {
    params: PageAddScriptToEvaluateOnNewDocumentParams,
}

impl PageAddScriptToEvaluateOnNewDocument {
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            params: PageAddScriptToEvaluateOnNewDocumentParams {
                source: source.into(),
                world_name: None,
                include_command_line_api: None,
                run_immediately: None,
            },
        }
    }

    pub fn with_world_name(mut self, world_name: impl Into<String>) -> Self {
        self.params.world_name = Some(world_name.into());
        self
    }

    pub fn with_run_immediately(mut self, run_immediately: bool) -> Self {
        self.params.run_immediately = Some(run_immediately);
        self
    }
}

impl CdpCommand for PageAddScriptToEvaluateOnNewDocument {
    type Params = PageAddScriptToEvaluateOnNewDocumentParams;

    fn method(&self) -> &'static str {
        "Page.addScriptToEvaluateOnNewDocument"
    }

    fn params(&self) -> &Self::Params {
        &self.params
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PageAddScriptToEvaluateOnNewDocumentParams {
    pub source: String,
    #[serde(rename = "worldName", skip_serializing_if = "Option::is_none")]
    pub world_name: Option<String>,
    #[serde(
        rename = "includeCommandLineAPI",
        skip_serializing_if = "Option::is_none"
    )]
    pub include_command_line_api: Option<bool>,
    #[serde(rename = "runImmediately", skip_serializing_if = "Option::is_none")]
    pub run_immediately: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PageAddScriptToEvaluateOnNewDocumentResult {
    pub identifier: String,
}

/// `Page.enable` — required before `addScriptToEvaluateOnNewDocument` can fire.
#[derive(Debug, Clone, Default)]
pub struct PageEnable {
    params: PageEnableParams,
}

impl PageEnable {
    pub fn new() -> Self {
        Self::default()
    }
}

impl CdpCommand for PageEnable {
    type Params = PageEnableParams;
    fn method(&self) -> &'static str {
        "Page.enable"
    }
    fn params(&self) -> &Self::Params {
        &self.params
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct PageEnableParams {}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PageEnableResult {}

/// `Emulation.setUserAgentOverride` — UA + accept-language + platform +
/// `userAgentMetadata` (Sec-CH-UA client hints).
#[derive(Debug, Clone)]
pub struct EmulationSetUserAgentOverride {
    params: EmulationSetUserAgentOverrideParams,
}

impl EmulationSetUserAgentOverride {
    pub fn new(user_agent: impl Into<String>) -> Self {
        Self {
            params: EmulationSetUserAgentOverrideParams {
                user_agent: user_agent.into(),
                accept_language: None,
                platform: None,
                user_agent_metadata: None,
            },
        }
    }

    pub fn with_accept_language(mut self, lang: impl Into<String>) -> Self {
        self.params.accept_language = Some(lang.into());
        self
    }

    pub fn with_platform(mut self, platform: impl Into<String>) -> Self {
        self.params.platform = Some(platform.into());
        self
    }

    pub fn with_metadata(mut self, metadata: UserAgentMetadata) -> Self {
        self.params.user_agent_metadata = Some(metadata);
        self
    }
}

impl CdpCommand for EmulationSetUserAgentOverride {
    type Params = EmulationSetUserAgentOverrideParams;
    fn method(&self) -> &'static str {
        "Emulation.setUserAgentOverride"
    }
    fn params(&self) -> &Self::Params {
        &self.params
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct EmulationSetUserAgentOverrideParams {
    #[serde(rename = "userAgent")]
    pub user_agent: String,
    #[serde(rename = "acceptLanguage", skip_serializing_if = "Option::is_none")]
    pub accept_language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    #[serde(rename = "userAgentMetadata", skip_serializing_if = "Option::is_none")]
    pub user_agent_metadata: Option<UserAgentMetadata>,
}

/// User-agent client hints (Sec-CH-UA-* headers + `navigator.userAgentData`).
#[derive(Debug, Clone, Serialize)]
pub struct UserAgentMetadata {
    pub brands: Vec<UserAgentBrand>,
    #[serde(rename = "fullVersionList", skip_serializing_if = "Option::is_none")]
    pub full_version_list: Option<Vec<UserAgentBrand>>,
    pub platform: String,
    #[serde(rename = "platformVersion")]
    pub platform_version: String,
    pub architecture: String,
    pub model: String,
    pub mobile: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitness: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wow64: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UserAgentBrand {
    pub brand: String,
    pub version: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct EmulationSetUserAgentOverrideResult {}

/// `Emulation.setHardwareConcurrencyOverride`.
#[derive(Debug, Clone)]
pub struct EmulationSetHardwareConcurrencyOverride {
    params: EmulationSetHardwareConcurrencyOverrideParams,
}

impl EmulationSetHardwareConcurrencyOverride {
    pub fn new(hardware_concurrency: u32) -> Self {
        Self {
            params: EmulationSetHardwareConcurrencyOverrideParams {
                hardware_concurrency,
            },
        }
    }
}

impl CdpCommand for EmulationSetHardwareConcurrencyOverride {
    type Params = EmulationSetHardwareConcurrencyOverrideParams;
    fn method(&self) -> &'static str {
        "Emulation.setHardwareConcurrencyOverride"
    }
    fn params(&self) -> &Self::Params {
        &self.params
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct EmulationSetHardwareConcurrencyOverrideParams {
    #[serde(rename = "hardwareConcurrency")]
    pub hardware_concurrency: u32,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct EmulationSetHardwareConcurrencyOverrideResult {}

/// `Emulation.setLocaleOverride`.
#[derive(Debug, Clone)]
pub struct EmulationSetLocaleOverride {
    params: EmulationSetLocaleOverrideParams,
}

impl EmulationSetLocaleOverride {
    pub fn new(locale: impl Into<String>) -> Self {
        Self {
            params: EmulationSetLocaleOverrideParams {
                locale: Some(locale.into()),
            },
        }
    }
}

impl CdpCommand for EmulationSetLocaleOverride {
    type Params = EmulationSetLocaleOverrideParams;
    fn method(&self) -> &'static str {
        "Emulation.setLocaleOverride"
    }
    fn params(&self) -> &Self::Params {
        &self.params
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct EmulationSetLocaleOverrideParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct EmulationSetLocaleOverrideResult {}

/// `Emulation.setTimezoneOverride`.
#[derive(Debug, Clone)]
pub struct EmulationSetTimezoneOverride {
    params: EmulationSetTimezoneOverrideParams,
}

impl EmulationSetTimezoneOverride {
    pub fn new(timezone_id: impl Into<String>) -> Self {
        Self {
            params: EmulationSetTimezoneOverrideParams {
                timezone_id: timezone_id.into(),
            },
        }
    }
}

impl CdpCommand for EmulationSetTimezoneOverride {
    type Params = EmulationSetTimezoneOverrideParams;
    fn method(&self) -> &'static str {
        "Emulation.setTimezoneOverride"
    }
    fn params(&self) -> &Self::Params {
        &self.params
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct EmulationSetTimezoneOverrideParams {
    #[serde(rename = "timezoneId")]
    pub timezone_id: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct EmulationSetTimezoneOverrideResult {}

/// `Emulation.setDeviceMetricsOverride`.
#[derive(Debug, Clone)]
pub struct EmulationSetDeviceMetricsOverride {
    params: EmulationSetDeviceMetricsOverrideParams,
}

impl EmulationSetDeviceMetricsOverride {
    pub fn new(width: u32, height: u32, device_scale_factor: f32, mobile: bool) -> Self {
        Self {
            params: EmulationSetDeviceMetricsOverrideParams {
                width,
                height,
                device_scale_factor,
                mobile,
                screen_width: None,
                screen_height: None,
            },
        }
    }

    pub fn with_screen(mut self, screen_width: u32, screen_height: u32) -> Self {
        self.params.screen_width = Some(screen_width);
        self.params.screen_height = Some(screen_height);
        self
    }
}

impl CdpCommand for EmulationSetDeviceMetricsOverride {
    type Params = EmulationSetDeviceMetricsOverrideParams;
    fn method(&self) -> &'static str {
        "Emulation.setDeviceMetricsOverride"
    }
    fn params(&self) -> &Self::Params {
        &self.params
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct EmulationSetDeviceMetricsOverrideParams {
    pub width: u32,
    pub height: u32,
    #[serde(rename = "deviceScaleFactor")]
    pub device_scale_factor: f32,
    pub mobile: bool,
    #[serde(rename = "screenWidth", skip_serializing_if = "Option::is_none")]
    pub screen_width: Option<u32>,
    #[serde(rename = "screenHeight", skip_serializing_if = "Option::is_none")]
    pub screen_height: Option<u32>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct EmulationSetDeviceMetricsOverrideResult {}

/// `Emulation.setTouchEmulationEnabled`.
#[derive(Debug, Clone)]
pub struct EmulationSetTouchEmulationEnabled {
    params: EmulationSetTouchEmulationEnabledParams,
}

impl EmulationSetTouchEmulationEnabled {
    pub fn new(enabled: bool, max_touch_points: Option<u8>) -> Self {
        Self {
            params: EmulationSetTouchEmulationEnabledParams {
                enabled,
                max_touch_points,
            },
        }
    }
}

impl CdpCommand for EmulationSetTouchEmulationEnabled {
    type Params = EmulationSetTouchEmulationEnabledParams;
    fn method(&self) -> &'static str {
        "Emulation.setTouchEmulationEnabled"
    }
    fn params(&self) -> &Self::Params {
        &self.params
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct EmulationSetTouchEmulationEnabledParams {
    pub enabled: bool,
    #[serde(rename = "maxTouchPoints", skip_serializing_if = "Option::is_none")]
    pub max_touch_points: Option<u8>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct EmulationSetTouchEmulationEnabledResult {}

/// `Network.setExtraHTTPHeaders` — used to install Sec-CH-UA-* hints
/// alongside the UA override.
#[derive(Debug, Clone)]
pub struct NetworkSetExtraHttpHeaders {
    params: NetworkSetExtraHttpHeadersParams,
}

impl NetworkSetExtraHttpHeaders {
    pub fn new(headers: Value) -> Self {
        Self {
            params: NetworkSetExtraHttpHeadersParams { headers },
        }
    }
}

impl CdpCommand for NetworkSetExtraHttpHeaders {
    type Params = NetworkSetExtraHttpHeadersParams;
    fn method(&self) -> &'static str {
        "Network.setExtraHTTPHeaders"
    }
    fn params(&self) -> &Self::Params {
        &self.params
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct NetworkSetExtraHttpHeadersParams {
    pub headers: Value,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct NetworkSetExtraHttpHeadersResult {}
