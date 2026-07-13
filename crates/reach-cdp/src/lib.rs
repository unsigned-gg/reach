//! Typed helpers for sending Chrome DevTools Protocol commands through
//! `reach-browserd`.
//!
//! The crate keeps CDP command construction in Rust types while preserving an
//! escape hatch through [`RawCdpCommand`] for protocol methods that do not have
//! typed wrappers yet.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use tracing::{debug, trace};

/// Typed command definitions for the CDP methods Reach currently uses.
pub mod commands;

const DEFAULT_BROWSERD_URL: &str = "http://127.0.0.1:8401";

/// HTTP client for the `reach-browserd` CDP bridge.
#[derive(Debug, Clone)]
pub struct CdpClient {
    http: reqwest::Client,
    browserd_url: String,
    /// When set, every command is routed to this CDP session. `None` defers to
    /// the daemon's default healing session for back-compat.
    session_id: Option<String>,
}

impl CdpClient {
    /// Create a client for a `reach-browserd` base URL.
    pub fn new(browserd_url: impl Into<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            browserd_url: trim_trailing_slash(browserd_url.into()),
            session_id: None,
        }
    }

    /// Create a client for the default local `reach-browserd` endpoint.
    pub fn localhost() -> Self {
        Self::new(DEFAULT_BROWSERD_URL)
    }

    /// Return the normalized `reach-browserd` base URL.
    pub fn browserd_url(&self) -> &str {
        &self.browserd_url
    }

    /// Return a clone of this client pinned to a specific CDP session id.
    pub fn with_session(&self, session_id: impl Into<String>) -> Self {
        let mut clone = self.clone();
        clone.session_id = Some(session_id.into());
        clone
    }

    /// Return the session id this client is pinned to, if any.
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    /// Create a client and verify that `reach-browserd` responds to CDP.
    pub async fn connect(browserd_url: impl Into<String>) -> Result<Self> {
        let client = Self::new(browserd_url);
        client
            .send::<_, Value>(RawCdpCommand::new("Browser.getVersion"))
            .await?;
        Ok(client)
    }

    /// Mint a fresh browser context with optional proxy and return a guard
    /// owning the context and a session pre-attached to a target inside it.
    pub async fn create_context(&self, options: NewContext) -> Result<BrowserContext> {
        let response = self
            .http
            .post(format!("{}/cdp/context/create", self.browserd_url))
            .json(&options)
            .send()
            .await
            .context("create_context request to reach-browserd")?
            .error_for_status()
            .context("reach-browserd returned an HTTP error for create_context")?
            .json::<CreateContextResponse>()
            .await
            .context("decoding create_context response")?;

        debug!(
            browser_context_id = %response.browser_context_id,
            target_id = %response.target_id,
            session_id = %response.session_id,
            "minted CDP browser context"
        );

        Ok(BrowserContext {
            client: self.clone(),
            browser_context_id: response.browser_context_id,
            target_id: response.target_id,
            session_id: response.session_id,
            disposed: false,
        })
    }

    /// Send a typed CDP command through `reach-browserd`.
    pub async fn send<C, R>(&self, command: C) -> Result<CdpResponse<R>>
    where
        C: CdpCommand,
        R: DeserializeOwned,
    {
        let method = command.method();
        debug!(method, browserd_url = %self.browserd_url, session_id = ?self.session_id, "sending CDP command");

        let request = CdpRequest {
            method: method.to_string(),
            params: serde_json::to_value(command.params())
                .context("failed to serialize CDP command params")?,
            session_id: self.session_id.clone(),
        };

        let response = self
            .http
            .post(format!("{}/cdp", self.browserd_url))
            .json(&request)
            .send()
            .await
            .context("failed to send CDP command to reach-browserd")?
            .error_for_status()
            .context("reach-browserd returned an HTTP error")?
            .json::<CdpResponse<R>>()
            .await
            .context("failed to decode CDP response from reach-browserd")?;

        trace!(
            method,
            has_result = response.result.is_some(),
            has_error = response.error.is_some(),
            "received CDP response"
        );

        Ok(response)
    }
}

#[derive(Debug, Clone, Serialize)]
struct CdpRequest {
    method: String,
    params: Value,
    #[serde(skip_serializing_if = "Option::is_none", rename = "session_id")]
    session_id: Option<String>,
}

/// Options for [`CdpClient::create_context`].
#[derive(Debug, Clone, Default, Serialize)]
pub struct NewContext {
    /// Optional proxy URL such as `http://host:port` or `socks5://host:port`.
    #[serde(skip_serializing_if = "Option::is_none", rename = "proxy_server")]
    pub proxy_server: Option<String>,
    /// Optional `;`-separated list of bypass patterns.
    #[serde(skip_serializing_if = "Option::is_none", rename = "proxy_bypass_list")]
    pub proxy_bypass_list: Option<String>,
    /// Optional initial URL to navigate the freshly minted target to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CreateContextResponse {
    browser_context_id: String,
    target_id: String,
    session_id: String,
}

#[derive(Debug, Serialize)]
struct DisposeContextRequest<'a> {
    browser_context_id: &'a str,
}

/// Owned handle to a CDP browser context minted via `reach-browserd`.
///
/// Drop runs a best-effort, fire-and-forget dispose; for guaranteed cleanup
/// call [`BrowserContext::close`] explicitly so any errors surface.
#[derive(Debug)]
pub struct BrowserContext {
    client: CdpClient,
    browser_context_id: String,
    target_id: String,
    session_id: String,
    disposed: bool,
}

impl BrowserContext {
    /// Return a `CdpClient` pinned to this context's session id.
    pub fn client(&self) -> CdpClient {
        self.client.with_session(self.session_id.clone())
    }

    /// `browserContextId` returned by Chrome.
    pub fn browser_context_id(&self) -> &str {
        &self.browser_context_id
    }

    /// `targetId` of the page minted inside the context.
    pub fn target_id(&self) -> &str {
        &self.target_id
    }

    /// Flattened sessionId attached to the target.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Explicitly dispose the context. Idempotent.
    pub async fn close(mut self) -> Result<()> {
        if self.disposed {
            return Ok(());
        }
        let body = DisposeContextRequest {
            browser_context_id: &self.browser_context_id,
        };
        self.client
            .http
            .post(format!("{}/cdp/context/dispose", self.client.browserd_url))
            .json(&body)
            .send()
            .await
            .context("dispose_context request to reach-browserd")?
            .error_for_status()
            .context("reach-browserd returned an HTTP error for dispose_context")?;
        self.disposed = true;
        Ok(())
    }
}

impl Drop for BrowserContext {
    fn drop(&mut self) {
        if self.disposed {
            return;
        }
        // Async cleanup in `Drop` is unreliable. Spawn a fire-and-forget task
        // so callers who forgot to call `close().await` still get a best-effort
        // dispose. Errors are logged, not propagated.
        let client = self.client.clone();
        let id = self.browser_context_id.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let body = DisposeContextRequest {
                    browser_context_id: &id,
                };
                if let Err(error) = client
                    .http
                    .post(format!("{}/cdp/context/dispose", client.browserd_url))
                    .json(&body)
                    .send()
                    .await
                {
                    tracing::warn!(
                        error = %error,
                        browser_context_id = %id,
                        "best-effort browser context dispose failed in Drop"
                    );
                }
            });
        }
    }
}

/// Top-level response returned by a CDP command.
#[derive(Debug, Clone, Deserialize)]
pub struct CdpResponse<T = Value> {
    /// CDP command identifier, when supplied by the bridge.
    pub id: Option<u64>,
    /// Successful command result payload.
    pub result: Option<T>,
    /// CDP error payload, when the browser rejected the command.
    pub error: Option<CdpError>,
}

impl<T> CdpResponse<T> {
    /// Convert the CDP envelope into a standard `Result`.
    pub fn into_result(self) -> std::result::Result<T, CdpError> {
        match (self.result, self.error) {
            (Some(result), _) => Ok(result),
            (_, Some(error)) => Err(error),
            (None, None) => Err(CdpError {
                code: None,
                message: "CDP response did not include result or error".to_string(),
                data: None,
            }),
        }
    }
}

/// Error object returned by a failed CDP command.
#[derive(Debug, Clone, Deserialize)]
pub struct CdpError {
    /// Protocol error code, when Chromium supplied one.
    pub code: Option<i64>,
    /// Human-readable CDP error message.
    pub message: String,
    /// Optional structured diagnostic data.
    pub data: Option<Value>,
}

/// Trait implemented by typed CDP commands.
pub trait CdpCommand {
    /// Serializable parameter object for the command.
    type Params: Serialize;

    /// CDP method name, such as `Page.navigate`.
    fn method(&self) -> &'static str;
    /// Parameters sent with the command.
    fn params(&self) -> &Self::Params;
}

/// Untyped CDP command for methods without a typed wrapper.
#[derive(Debug, Clone)]
pub struct RawCdpCommand<P = Value> {
    method: &'static str,
    params: P,
}

impl RawCdpCommand<Value> {
    /// Create a raw command with an empty parameter object.
    pub fn new(method: &'static str) -> Self {
        Self {
            method,
            params: Value::Object(Default::default()),
        }
    }
}

impl<P> RawCdpCommand<P>
where
    P: Serialize,
{
    /// Create a raw command with caller-provided parameters.
    pub fn with_params(method: &'static str, params: P) -> Self {
        Self { method, params }
    }
}

impl<P> CdpCommand for RawCdpCommand<P>
where
    P: Serialize,
{
    type Params = P;

    fn method(&self) -> &'static str {
        self.method
    }

    fn params(&self) -> &Self::Params {
        &self.params
    }
}

fn trim_trailing_slash(mut url: String) -> String {
    while url.ends_with('/') {
        url.pop();
    }
    url
}
