//! Glue between the host MCP server and `reach-scraper`.
//!
//! The functions here are deliberately thin so `commands::serve` stays focused
//! on transport concerns. Sandbox -> CDP client resolution, AdaptiveMemory
//! initialization, and per-tool execution all live here.

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow, bail};
use reach_cdp::{
    CdpClient, CdpCommand,
    commands::{PageNavigate, PageNavigateResult, RuntimeEvaluate, RuntimeEvaluateResult},
};
use reach_scraper::{
    AdaptiveMemory, CdpFetcher, ElementFingerprint, ExtractMode, FingerprintProfile, HybridFetcher,
    ProxyConfig, ResilientOutcome, ResilientRequest, ScrapeOutput, SearchResult, StaticFetcher,
    ValidateOptions, apply_profile, ddg_html_search, resilient_extract, url_components,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::Mutex;

use crate::docker::DockerClient;
use crate::mcp::ScrapeProxyParams;

/// Bundle of state shared by every scraper-backed MCP tool invocation.
#[derive(Clone)]
pub struct ScraperState {
    pub memory: Arc<Mutex<AdaptiveMemory>>,
    /// Coarse lock that serializes CDP scraper operations until per-request
    /// browser contexts ship. Step 3 removes this.
    pub cdp_lock: Arc<Mutex<()>>,
    /// Default proxy applied to every scrape when the call doesn't specify
    /// one. Wired from `reach serve --proxy ...` so agents don't need to
    /// pass a proxy on every tool call.
    pub default_proxy: Option<ProxyConfig>,
}

impl ScraperState {
    /// Open the AdaptiveMemory database at `path`, run migrations, and wrap it
    /// in shared state ready to mount on the MCP server.
    pub fn open(path: &Path) -> Result<Self> {
        let memory = AdaptiveMemory::connect(path)
            .with_context(|| format!("opening adaptive memory at {}", path.display()))?;
        memory
            .init_db()
            .context("running adaptive memory migrations")?;
        Ok(Self {
            memory: Arc::new(Mutex::new(memory)),
            cdp_lock: Arc::new(Mutex::new(())),
            default_proxy: None,
        })
    }

    /// Set the default proxy applied when a scrape call omits one.
    pub fn with_default_proxy(mut self, proxy: Option<ProxyConfig>) -> Self {
        self.default_proxy = proxy;
        self
    }
}

/// Resolve a sandbox name to a `reach-browserd` HTTP URL on the host.
async fn browserd_url_for(docker: &DockerClient, sandbox: &str) -> Result<String> {
    let info = docker
        .find(sandbox)
        .await
        .with_context(|| format!("looking up sandbox `{sandbox}`"))?;
    let port = info
        .ports
        .browserd
        .ok_or_else(|| anyhow!("sandbox `{sandbox}` does not publish a browserd port"))?;
    Ok(format!("http://127.0.0.1:{port}"))
}

/// Resolve a sandbox name to a connected `CdpClient`.
pub async fn cdp_client_for(docker: &DockerClient, sandbox: &str) -> Result<CdpClient> {
    let url = browserd_url_for(docker, sandbox).await?;
    Ok(CdpClient::new(url))
}

fn proxy_from(params: Option<&ScrapeProxyParams>) -> Option<ProxyConfig> {
    let p = params?;
    if let Some(user) = p.username.as_deref() {
        Some(ProxyConfig::with_credentials(
            p.url.clone(),
            user,
            p.password.clone().unwrap_or_default(),
        ))
    } else {
        Some(ProxyConfig::new(p.url.clone()))
    }
}

/// Pick the per-call proxy when set; otherwise fall back to the server-level
/// default. Returns `None` only when both are absent.
fn resolve_proxy(
    state: &ScraperState,
    per_call: Option<&ScrapeProxyParams>,
) -> Option<ProxyConfig> {
    proxy_from(per_call).or_else(|| state.default_proxy.clone())
}

/// Parse an `http[s]://[user:pass@]host:port` string into a `ProxyConfig`.
///
/// User and password are taken raw (no percent decoding) — keep them plain in
/// the source string. For credentials with special characters, use the
/// per-call `proxy` JSON object instead.
pub fn parse_proxy_url(raw: &str) -> Result<ProxyConfig> {
    let parsed = reqwest::Url::parse(raw).with_context(|| format!("invalid proxy URL `{raw}`"))?;
    let username = if parsed.username().is_empty() {
        None
    } else {
        Some(parsed.username().to_string())
    };
    let password = parsed.password().map(str::to_string);
    let mut sanitized = parsed.clone();
    sanitized.set_username("").ok();
    sanitized.set_password(None).ok();
    Ok(ProxyConfig {
        url: sanitized.as_str().trim_end_matches('/').to_string(),
        username,
        password,
    })
}

/// Run a one-shot static HTTP fetch. The CDP client is unused.
pub async fn run_static(
    state: &ScraperState,
    url: String,
    proxy: Option<ScrapeProxyParams>,
) -> Result<ScrapeOutput> {
    let fetcher = StaticFetcher::new(resolve_proxy(state, proxy.as_ref()))?;
    fetcher.fetch(url).await
}

/// Free no-captcha search via DuckDuckGo HTML. Static-fetcher only.
///
/// Unlike the other scrape_* helpers this DOES NOT inherit the server-level
/// `--proxy` default. DDG rate-limits and anomaly-flags shared residential
/// pools (Webshare etc.) — pretty much every common cheap proxy IP is
/// already burned with DDG. A clean direct egress works much better. Pass
/// `proxy` explicitly per-call when you actually need one.
pub async fn run_search(
    _state: &ScraperState,
    query: String,
    engine: String,
    max_results: usize,
    proxy: Option<ScrapeProxyParams>,
) -> Result<SearchOutput> {
    let fetcher = StaticFetcher::new(proxy_from(proxy.as_ref()))?;
    let results = match engine.as_str() {
        "ddg" | "duckduckgo" | "" => ddg_html_search(&fetcher, &query, max_results).await?,
        other => bail!("unknown search engine `{other}` (supported: `ddg`)"),
    };
    Ok(SearchOutput {
        engine: "ddg".to_string(),
        query,
        count: results.len(),
        results,
    })
}

#[derive(Debug, serde::Serialize)]
pub struct SearchOutput {
    pub engine: String,
    pub query: String,
    pub count: usize,
    pub results: Vec<SearchResult>,
}

/// Run the hybrid fetch path for `sandbox`. Static-first, escalates to CDP on
/// 403/429. When `stealth` is set, the CDP target gets the named fingerprint
/// profile applied before any escalation navigation.
pub async fn run_agent(
    docker: &DockerClient,
    state: &ScraperState,
    sandbox: &str,
    url: String,
    proxy: Option<ScrapeProxyParams>,
    escalate: bool,
    stealth: Option<String>,
) -> Result<ScrapeOutput> {
    let resolved_proxy = resolve_proxy(state, proxy.as_ref());
    let static_fetcher = StaticFetcher::new(resolved_proxy.clone())?;

    if !escalate {
        return static_fetcher.fetch(url).await;
    }

    let cdp = cdp_client_for(docker, sandbox).await?;
    let _guard = state.cdp_lock.lock().await;

    if let Some(profile_id) = stealth.as_deref() {
        apply_named_profile(&cdp, profile_id).await?;
    }

    let cdp_fetcher = CdpFetcher::new(&cdp);
    let hybrid = HybridFetcher::new(static_fetcher, cdp_fetcher);
    hybrid.fetch(url).await
}

/// Apply a built-in stealth profile to the sandbox's CDP target.
pub async fn run_stealth_apply(
    docker: &DockerClient,
    state: &ScraperState,
    sandbox: &str,
    profile_id: &str,
) -> Result<FingerprintProfile> {
    let cdp = cdp_client_for(docker, sandbox).await?;
    let _guard = state.cdp_lock.lock().await;
    let profile = apply_named_profile(&cdp, profile_id).await?;
    Ok(profile)
}

async fn apply_named_profile(
    cdp: &reach_cdp::CdpClient,
    profile_id: &str,
) -> Result<FingerprintProfile> {
    let profile = FingerprintProfile::by_id(profile_id).ok_or_else(|| {
        anyhow!(
            "unknown stealth profile `{profile_id}`. Available: {}",
            FingerprintProfile::builtin_ids().join(", ")
        )
    })?;
    apply_profile(cdp, &profile).await?;
    Ok(profile)
}

/// Capture an element fingerprint via CDP and persist it.
pub async fn run_learn(
    docker: &DockerClient,
    state: &ScraperState,
    sandbox: &str,
    url: String,
    selector: String,
    navigate: bool,
) -> Result<LearnOutput> {
    let cdp = cdp_client_for(docker, sandbox).await?;
    let _guard = state.cdp_lock.lock().await;

    if navigate {
        let nav: PageNavigateResult = send(&cdp, PageNavigate::new(url.clone()))
            .await
            .context("Page.navigate during scrape_learn")?;
        if let Some(error_text) = nav.error_text {
            bail!("Page.navigate failed for {url}: {error_text}");
        }
    }

    let captured: CapturedFingerprint = evaluate_typed(
        &cdp,
        &fingerprint_capture_script(&selector),
        "fingerprint capture",
    )
    .await?;

    let (domain, url_pattern) = url_components(&url)
        .ok_or_else(|| anyhow!("could not parse URL components from `{url}`"))?;

    let fingerprint = ElementFingerprint {
        domain,
        url_pattern,
        original_selector: selector.clone(),
        element_tag: captured.tag.to_lowercase(),
        text_hash: captured.text_hash,
        dom_path: captured.dom_path,
        sibling_signature: captured.sibling_signature,
        bbox_json: captured.bbox_json,
    };

    let row_id = {
        let mem = state.memory.lock().await;
        mem.save_fingerprint(&fingerprint)?
    };

    Ok(LearnOutput {
        id: row_id,
        selector,
        fingerprint,
    })
}

/// Run the full Observe → … → Repair loop and persist learnings.
///
/// When `proxy` (or the server-level default) is set, the loop runs against a
/// freshly minted browser context with that proxy, so the page never sees the
/// host's IP. Stealth, when requested, is applied to whichever session the
/// loop is actually driving (the proxied context, or the default session).
pub async fn run_resilient(
    docker: &DockerClient,
    state: &ScraperState,
    sandbox: &str,
    request: ResilientRequest,
    stealth: Option<String>,
    proxy: Option<ScrapeProxyParams>,
) -> Result<ResilientOutcome> {
    let cdp = cdp_client_for(docker, sandbox).await?;
    let resolved_proxy = resolve_proxy(state, proxy.as_ref());
    let _guard = state.cdp_lock.lock().await;

    if let Some(proxy) = resolved_proxy {
        if proxy.username.is_some() || proxy.password.is_some() {
            anyhow::bail!(
                "authenticated proxies are not yet supported on the CDP path \
                 ({}). Use IP auth or wait for Fetch.authRequired support.",
                proxy.url
            );
        }
        let context = cdp
            .create_context(reach_cdp::NewContext {
                proxy_server: Some(proxy.url.clone()),
                proxy_bypass_list: None,
                url: None,
            })
            .await
            .context("creating proxied browser context for resilient extract")?;
        let scoped = cdp.with_session(context.session_id());
        if let Some(profile_id) = stealth.as_deref() {
            apply_named_profile(&scoped, profile_id).await?;
        }
        let outcome = resilient_extract(&scoped, &state.memory, &request).await;
        if let Err(e) = context.close().await {
            tracing::warn!(error = %e, "failed to dispose proxied browser context");
        }
        outcome
    } else {
        if let Some(profile_id) = stealth.as_deref() {
            apply_named_profile(&cdp, profile_id).await?;
        }
        resilient_extract(&cdp, &state.memory, &request).await
    }
}

/// Parse the loose JSON shape accepted on the MCP wire into a typed
/// [`ExtractMode`]. Defaults to `text` when the payload is empty/null.
pub fn parse_extract_mode(value: &Value) -> Result<ExtractMode> {
    if value.is_null() {
        return Ok(ExtractMode::Text);
    }
    if let Some(s) = value.as_str() {
        return match s {
            "text" => Ok(ExtractMode::Text),
            "html" => Ok(ExtractMode::Html),
            other => bail!("unknown extract mode `{other}`"),
        };
    }
    serde_json::from_value(value.clone())
        .with_context(|| format!("invalid extract mode payload: {value}"))
}

/// Parse the loose JSON shape into [`ValidateOptions`].
pub fn parse_validate_options(value: &Value) -> Result<ValidateOptions> {
    if value.is_null() {
        return Ok(ValidateOptions::default());
    }
    serde_json::from_value(value.clone())
        .with_context(|| format!("invalid validate payload: {value}"))
}

/// Look up AdaptiveMemory candidates for a URL.
pub async fn run_recover(
    state: &ScraperState,
    url: String,
    selector_filter: Option<String>,
) -> Result<RecoverOutput> {
    let (domain, url_pattern) = url_components(&url)
        .ok_or_else(|| anyhow!("could not parse URL components from `{url}`"))?;

    let candidates = {
        let mem = state.memory.lock().await;
        mem.find_candidates(&domain, &url_pattern)?
    };

    let candidates = match selector_filter {
        Some(filter) => candidates
            .into_iter()
            .filter(|c| c.original_selector == filter)
            .collect(),
        None => candidates,
    };

    Ok(RecoverOutput {
        domain,
        url_pattern,
        count: candidates.len(),
        candidates,
    })
}

#[derive(Debug, Serialize)]
pub struct LearnOutput {
    pub id: i64,
    pub selector: String,
    pub fingerprint: ElementFingerprint,
}

#[derive(Debug, Serialize)]
pub struct RecoverOutput {
    pub domain: String,
    pub url_pattern: String,
    pub count: usize,
    pub candidates: Vec<reach_scraper::ElementFingerprintCandidate>,
}

#[derive(Debug, Deserialize)]
struct CapturedFingerprint {
    tag: String,
    text_hash: String,
    dom_path: String,
    sibling_signature: String,
    bbox_json: String,
}

/// JS snippet that locates `selector`, then returns a stable fingerprint shape.
///
/// Text hashing uses a simple FNV-1a 32-bit so the script stays inline and
/// self-contained; we only need stability across observations of the same page,
/// not cryptographic strength.
fn fingerprint_capture_script(selector: &str) -> String {
    let escaped = selector.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        r#"
(() => {{
  const el = document.querySelector("{escaped}");
  if (!el) return {{ error: "selector not found", selector: "{escaped}" }};

  const fnv = (s) => {{
    let h = 0x811c9dc5 >>> 0;
    for (let i = 0; i < s.length; i++) {{
      h ^= s.charCodeAt(i);
      h = Math.imul(h, 0x01000193) >>> 0;
    }}
    return h.toString(16).padStart(8, "0");
  }};

  const path = (n) => {{
    const segs = [];
    while (n && n.nodeType === 1) {{
      if (n === document.documentElement) {{ segs.unshift("html"); break; }}
      const tag = n.tagName.toLowerCase();
      const parent = n.parentElement;
      if (!parent) break;
      const idx = Array.from(parent.children).indexOf(n) + 1;
      segs.unshift(tag + ":nth-child(" + idx + ")");
      n = parent;
    }}
    return segs.join(">");
  }};

  const sibs = (n) => {{
    const p = n.parentElement;
    if (!p) return "";
    return Array.from(p.children).map((c) => c.tagName.toLowerCase()).join("+");
  }};

  const rect = el.getBoundingClientRect();
  const bbox = {{
    x: Math.round(rect.x), y: Math.round(rect.y),
    width: Math.round(rect.width), height: Math.round(rect.height),
    scroll_x: Math.round(window.scrollX),
    scroll_y: Math.round(window.scrollY),
  }};

  const text = (el.innerText || el.textContent || "").trim().slice(0, 512);
  return {{
    tag: el.tagName,
    text_hash: fnv(text),
    dom_path: path(el),
    sibling_signature: sibs(el),
    bbox_json: JSON.stringify(bbox),
  }};
}})()
"#
    )
}

async fn evaluate_typed<T: serde::de::DeserializeOwned>(
    cdp: &CdpClient,
    expression: &str,
    ctx: &str,
) -> Result<T> {
    let result: RuntimeEvaluateResult = send(
        cdp,
        RuntimeEvaluate::new(expression.to_string())
            .with_return_by_value(true)
            .with_await_promise(true),
    )
    .await
    .with_context(|| format!("Runtime.evaluate ({ctx})"))?;

    if let Some(exc) = result.exception_details {
        bail!("Runtime.evaluate ({ctx}) threw: {}", exc.text);
    }

    let value: Value = result
        .result
        .value
        .ok_or_else(|| anyhow!("Runtime.evaluate ({ctx}) returned no value"))?;

    if let Some(err) = value.get("error").and_then(Value::as_str) {
        bail!("{ctx}: {err}");
    }

    serde_json::from_value(value).with_context(|| format!("decoding {ctx} payload"))
}

async fn send<C, R>(cdp: &CdpClient, command: C) -> Result<R>
where
    C: CdpCommand,
    R: serde::de::DeserializeOwned,
{
    let method = command.method();
    cdp.send::<_, R>(command)
        .await?
        .into_result()
        .map_err(|err| anyhow!("CDP {method} failed: {}", err.message))
}
