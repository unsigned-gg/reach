//! Scraping primitives that combine static HTTP fetching with CDP-backed
//! browser escalation for Reach sandboxes.
//!
//! This crate is intentionally library-only for now. The host CLI and MCP tool
//! wiring can depend on it later without embedding scraping behavior directly in
//! `reach-cli`.

use anyhow::Result;
use reach_cdp::CdpClient;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// SQLite-backed adaptive selector memory.
pub mod adaptive;
/// Self-healing extraction loop on top of CDP and adaptive memory.
pub mod agent;
/// CDP-backed page fetcher.
pub mod cdp_fetcher;
/// Fetcher that starts with static HTTP and escalates to CDP when blocked.
pub mod hybrid_fetcher;
/// Free no-captcha search backends (DuckDuckGo HTML for now).
pub mod search;
/// Static HTTP fetcher with cookie and proxy support.
pub mod static_fetcher;
/// Browser fingerprint spoofing presets and the CDP+JS shim driver.
pub mod stealth;
pub use adaptive::{
    AdaptiveMemory, ElementFingerprint, ElementFingerprintCandidate, SCHEMA_VERSION, url_components,
};
pub use agent::{
    ExtractMode, RepairStrategy, ResilientOutcome, ResilientRequest, SelectorSource,
    ValidateOptions, resilient_extract,
};
pub use cdp_fetcher::CdpFetcher;
pub use hybrid_fetcher::HybridFetcher;
pub use search::{SearchResult, ddg_html_search};
pub use static_fetcher::StaticFetcher;
pub use stealth::{
    FingerprintProfile, apply_profile, profile_linux_chrome, profile_mac_chrome,
    profile_windows_chrome,
};

/// High-level scraper facade for Reach.
#[derive(Debug, Clone)]
pub struct ReachScraper {
    cdp: CdpClient,
    proxy_rotator: Option<ProxyRotator>,
}

impl ReachScraper {
    /// Create a scraper backed by a CDP client.
    pub fn new(cdp: CdpClient) -> Self {
        Self {
            cdp,
            proxy_rotator: None,
        }
    }

    /// Attach a proxy rotator for callers that manage proxy selection.
    pub fn with_proxy_rotator(mut self, proxy_rotator: ProxyRotator) -> Self {
        self.proxy_rotator = Some(proxy_rotator);
        self
    }

    /// Return the underlying CDP client.
    pub fn cdp(&self) -> &CdpClient {
        &self.cdp
    }

    /// Return the configured proxy rotator, if any.
    pub fn proxy_rotator(&self) -> Option<&ProxyRotator> {
        self.proxy_rotator.as_ref()
    }

    /// Scrape a URL using the CDP-backed fetch path.
    pub async fn scrape(&self, request: ScrapeRequest) -> Result<ScrapeOutput> {
        CdpFetcher::from_scraper(self)
            .fetch(request.url, request.proxy)
            .await
    }
}

/// Request accepted by the high-level scraper facade.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapeRequest {
    /// URL to scrape.
    pub url: String,
    /// Optional proxy to use for the request.
    #[serde(default)]
    pub proxy: Option<ProxyConfig>,
}

/// Scraped page output and associated metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapeOutput {
    /// Original requested URL.
    pub url: String,
    /// Extracted page content, when available.
    pub content: Option<String>,
    /// Metadata collected while fetching.
    pub metadata: ScrapeMetadata,
}

/// Metadata describing a scrape attempt.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScrapeMetadata {
    /// Final browser or HTTP URL after redirects.
    pub final_url: Option<String>,
    /// HTTP status code when available.
    pub status_code: Option<u16>,
    /// Proxy used for the request.
    pub proxy: Option<ProxyConfig>,
}

/// Backend trait for components that expose a CDP client.
pub trait ScraperBackend {
    /// Return the CDP client used by this backend.
    fn cdp(&self) -> &CdpClient;
}

impl ScraperBackend for ReachScraper {
    fn cdp(&self) -> &CdpClient {
        &self.cdp
    }
}

/// Source of proxy configurations.
pub trait ProxyProvider {
    /// Return the next proxy to use.
    fn next_proxy(&mut self) -> Option<ProxyConfig>;
}

/// Round-robin in-memory proxy provider.
#[derive(Debug, Clone, Default)]
pub struct ProxyRotator {
    proxies: VecDeque<ProxyConfig>,
}

impl ProxyRotator {
    /// Create a rotator from a collection of proxy configs.
    pub fn new(proxies: impl IntoIterator<Item = ProxyConfig>) -> Self {
        Self {
            proxies: proxies.into_iter().collect(),
        }
    }

    /// Return whether the rotator has no proxies.
    pub fn is_empty(&self) -> bool {
        self.proxies.is_empty()
    }

    /// Return the number of configured proxies.
    pub fn len(&self) -> usize {
        self.proxies.len()
    }

    /// Return the next proxy and rotate it to the back of the queue.
    pub fn next_proxy(&mut self) -> Option<ProxyConfig> {
        let proxy = self.proxies.pop_front()?;
        self.proxies.push_back(proxy.clone());
        Some(proxy)
    }

    /// Iterate over configured proxies without rotating them.
    pub fn proxies(&self) -> impl Iterator<Item = &ProxyConfig> {
        self.proxies.iter()
    }
}

impl ProxyProvider for ProxyRotator {
    fn next_proxy(&mut self) -> Option<ProxyConfig> {
        ProxyRotator::next_proxy(self)
    }
}

/// Proxy endpoint and optional basic credentials.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProxyConfig {
    /// Proxy URL accepted by reqwest, such as `http://host:port`.
    pub url: String,
    /// Optional proxy username.
    #[serde(default)]
    pub username: Option<String>,
    /// Optional proxy password.
    #[serde(default)]
    pub password: Option<String>,
}

impl ProxyConfig {
    /// Create a proxy config without credentials.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            username: None,
            password: None,
        }
    }

    /// Create a proxy config with basic credentials.
    pub fn with_credentials(
        url: impl Into<String>,
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Self {
        Self {
            url: url.into(),
            username: Some(username.into()),
            password: Some(password.into()),
        }
    }
}
