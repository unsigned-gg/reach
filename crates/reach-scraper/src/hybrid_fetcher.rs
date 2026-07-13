//! Hybrid fetching strategy that escalates from static HTTP to CDP.

use crate::{CdpFetcher, ScrapeOutput, StaticFetcher};
use anyhow::{Context, Result};
use tracing::debug;

/// Fetcher that retries blocked static requests through a browser.
#[derive(Debug, Clone)]
pub struct HybridFetcher<'a> {
    static_fetcher: StaticFetcher,
    cdp_fetcher: CdpFetcher<'a>,
}

impl<'a> HybridFetcher<'a> {
    /// Create a hybrid fetcher from static and CDP fetchers.
    pub fn new(static_fetcher: StaticFetcher, cdp_fetcher: CdpFetcher<'a>) -> Self {
        Self {
            static_fetcher,
            cdp_fetcher,
        }
    }

    /// Return the static fetcher.
    pub fn static_fetcher(&self) -> &StaticFetcher {
        &self.static_fetcher
    }

    /// Return the CDP fetcher.
    pub fn cdp_fetcher(&self) -> &CdpFetcher<'a> {
        &self.cdp_fetcher
    }

    /// Fetch a URL statically and escalate to CDP on common bot-block statuses.
    pub async fn fetch(&self, url: impl Into<String>) -> Result<ScrapeOutput> {
        let url = url.into();
        let static_output = self.static_fetcher.fetch(url.clone()).await?;

        if !is_bot_block(&static_output) {
            debug!(url = %url, "static fetch succeeded without CDP escalation");
            return Ok(static_output);
        }

        debug!(url = %url, status_code = ?static_output.metadata.status_code, "escalating blocked static fetch to CDP");
        let cdp_output = self
            .cdp_fetcher
            .fetch(url.clone(), self.static_fetcher.proxy().cloned())
            .await
            .context("static fetch was blocked and CDP escalation failed")?;

        let final_url = cdp_output
            .metadata
            .final_url
            .as_deref()
            .unwrap_or(&url)
            .to_owned();
        let mut cookie_urls = vec![url.clone()];

        if final_url != url {
            cookie_urls.push(final_url.clone());
        }

        let cookies = self
            .cdp_fetcher
            .get_cookies(Some(cookie_urls.clone()))
            .await
            .context("failed to extract cookies after CDP escalation")?;

        for cookie_url in cookie_urls {
            self.static_fetcher
                .inject_cookies(&cookies, &cookie_url)
                .context("failed to inject CDP cookies into static fetcher")?;
        }

        self.static_fetcher.fetch(url).await
    }
}

fn is_bot_block(output: &ScrapeOutput) -> bool {
    matches!(output.metadata.status_code, Some(403 | 429))
}
