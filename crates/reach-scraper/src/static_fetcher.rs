//! Static HTTP fetcher used before browser escalation.

use crate::{ProxyConfig, ScrapeMetadata, ScrapeOutput};
use anyhow::{Context, Result};
use reach_cdp::commands::{Cookie, CookieSameSite};
use reqwest::{Client, ClientBuilder, Proxy, Url, cookie::Jar, redirect::Policy};
use std::sync::Arc;
use tracing::{debug, trace};

const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36";

/// Fetches pages with reqwest while preserving cookies between requests.
#[derive(Debug, Clone)]
pub struct StaticFetcher {
    client: Client,
    cookie_jar: Arc<Jar>,
    proxy: Option<ProxyConfig>,
}

impl StaticFetcher {
    /// Create a static fetcher with an optional proxy.
    pub fn new(proxy: Option<ProxyConfig>) -> Result<Self> {
        let cookie_jar = Arc::new(Jar::default());
        let mut builder = ClientBuilder::new()
            .cookie_provider(cookie_jar.clone())
            .redirect(Policy::limited(10))
            .user_agent(DEFAULT_USER_AGENT);

        if let Some(proxy_config) = proxy.as_ref() {
            debug!(proxy_url = %proxy_config.url, "configuring static fetcher proxy");
            builder = builder.proxy(build_proxy(proxy_config)?);
        }

        let client = builder
            .build()
            .context("failed to build static reqwest client")?;

        Ok(Self {
            client,
            cookie_jar,
            proxy,
        })
    }

    /// Create a static fetcher without a proxy.
    pub fn without_proxy() -> Result<Self> {
        Self::new(None)
    }

    /// Return the configured proxy, if any.
    pub fn proxy(&self) -> Option<&ProxyConfig> {
        self.proxy.as_ref()
    }

    /// Fetch a URL with the static HTTP client.
    pub async fn fetch(&self, url: impl Into<String>) -> Result<ScrapeOutput> {
        let url = url.into();
        debug!(url = %url, uses_proxy = self.proxy.is_some(), "starting static fetch");
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .with_context(|| format!("failed to fetch {url} via static client"))?;

        let status_code = response.status().as_u16();
        let final_url = response.url().to_string();
        let content = response
            .text()
            .await
            .with_context(|| format!("failed to read static response body for {url}"))?;
        debug!(url = %url, final_url = %final_url, status_code, "completed static fetch");

        Ok(ScrapeOutput {
            url,
            content: Some(content),
            metadata: ScrapeMetadata {
                final_url: Some(final_url),
                status_code: Some(status_code),
                proxy: self.proxy.clone(),
            },
        })
    }

    /// POST a `application/x-www-form-urlencoded` body and return the response.
    /// Used by search backends like DDG HTML that serve full results only on POST.
    /// `extra_headers` lets callers attach `Referer`, `Origin`, etc. that some
    /// backends require to disambiguate from automated traffic.
    pub async fn post_form<T: serde::Serialize + ?Sized>(
        &self,
        url: impl Into<String>,
        form: &T,
        extra_headers: &[(&str, &str)],
    ) -> Result<ScrapeOutput> {
        let url = url.into();
        debug!(url = %url, uses_proxy = self.proxy.is_some(), "starting static POST");
        let mut req = self.client.post(&url).form(form);
        for (k, v) in extra_headers {
            req = req.header(*k, *v);
        }
        let response = req
            .send()
            .await
            .with_context(|| format!("failed to POST {url} via static client"))?;

        let status_code = response.status().as_u16();
        let final_url = response.url().to_string();
        let content = response
            .text()
            .await
            .with_context(|| format!("failed to read POST response body for {url}"))?;
        debug!(url = %url, final_url = %final_url, status_code, "completed static POST");

        Ok(ScrapeOutput {
            url,
            content: Some(content),
            metadata: ScrapeMetadata {
                final_url: Some(final_url),
                status_code: Some(status_code),
                proxy: self.proxy.clone(),
            },
        })
    }

    /// Inject CDP cookies into the static fetcher's cookie jar.
    pub fn inject_cookies(&self, cookies: &[Cookie], url: &str) -> Result<usize> {
        let url = Url::parse(url).with_context(|| format!("invalid cookie URL: {url}"))?;

        for cookie in cookies {
            self.cookie_jar
                .add_cookie_str(&set_cookie_header(cookie), &url);
        }

        trace!(count = cookies.len(), url = %url, "injected cookies into static fetcher");
        Ok(cookies.len())
    }
}

fn build_proxy(proxy_config: &ProxyConfig) -> Result<Proxy> {
    let mut proxy = Proxy::all(&proxy_config.url)
        .with_context(|| format!("invalid proxy URL: {}", proxy_config.url))?;

    if let Some(username) = proxy_config.username.as_ref() {
        proxy = proxy.basic_auth(username, proxy_config.password.as_deref().unwrap_or(""));
    }

    Ok(proxy)
}

fn set_cookie_header(cookie: &Cookie) -> String {
    let mut header = format!("{}={}", cookie.name, cookie.value);

    if !cookie.domain.is_empty() {
        header.push_str("; Domain=");
        header.push_str(&cookie.domain);
    }

    if !cookie.path.is_empty() {
        header.push_str("; Path=");
        header.push_str(&cookie.path);
    }

    if cookie.secure {
        header.push_str("; Secure");
    }

    if cookie.http_only {
        header.push_str("; HttpOnly");
    }

    if let Some(same_site) = cookie.same_site.as_ref() {
        header.push_str("; SameSite=");
        header.push_str(match same_site {
            CookieSameSite::Strict => "Strict",
            CookieSameSite::Lax => "Lax",
            CookieSameSite::None => "None",
        });
    }

    header
}
