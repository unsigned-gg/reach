//! Free, no-captcha search backends.
//!
//! Today only DuckDuckGo HTML is wired up, hit through the static path because
//! it's a no-JS page. Other engines (Mojeek, Startpage, Searx) follow the same
//! shape — add new functions here and a new MCP tool variant when needed.

use anyhow::Result;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::StaticFetcher;

/// One search hit: title, destination URL, snippet text.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// Search via DuckDuckGo's no-JS HTML endpoint.
///
/// POSTs `q=<query>` to `https://html.duckduckgo.com/html/` (the GET form
/// returns just the search box, not results). No captcha, no JS challenge.
/// Tolerates ~10 req/s per IP; for higher volume run the static fetcher
/// behind a rotating proxy via `reach serve --proxy ...`.
pub async fn ddg_html_search(
    fetcher: &StaticFetcher,
    query: &str,
    max_results: usize,
) -> Result<Vec<SearchResult>> {
    debug!(query = %query, "ddg html search");

    let form = [("q", query), ("b", ""), ("kl", "")];
    // DDG returns the search-box page (no results) when these headers are
    // missing. Sending them mimics a normal POST from html.duckduckgo.com.
    let headers: &[(&str, &str)] = &[
        ("Referer", "https://html.duckduckgo.com/"),
        ("Origin", "https://html.duckduckgo.com"),
        ("Accept-Language", "en-US,en;q=0.9"),
        (
            "Accept",
            "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
        ),
    ];
    let output = fetcher
        .post_form("https://html.duckduckgo.com/html/", &form, headers)
        .await?;
    let html = output
        .content
        .ok_or_else(|| anyhow::anyhow!("ddg html search returned no body"))?;

    Ok(parse_ddg_html(&html, max_results))
}

fn parse_ddg_html(html: &str, max_results: usize) -> Vec<SearchResult> {
    let document = Html::parse_document(html);
    let result_sel = Selector::parse("div.result, div.web-result").expect("static selector");
    let title_sel = Selector::parse("a.result__a").expect("static selector");
    let snippet_sel =
        Selector::parse("a.result__snippet, .result__snippet").expect("static selector");
    let url_sel = Selector::parse("a.result__url").expect("static selector");

    let mut out = Vec::new();
    for node in document.select(&result_sel) {
        let title_el = match node.select(&title_sel).next() {
            Some(el) => el,
            None => continue,
        };
        let title = title_el.text().collect::<String>().trim().to_string();
        if title.is_empty() {
            continue;
        }

        let raw_href = title_el.value().attr("href").unwrap_or("").to_string();
        let url = unwrap_ddg_redirect(&raw_href);
        if url.is_empty() {
            continue;
        }

        let snippet = node
            .select(&snippet_sel)
            .next()
            .map(|el| {
                el.text()
                    .collect::<String>()
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default();

        // Fall back to the displayed result URL when the unwrapped href
        // didn't yield anything sensible (very rare for DDG HTML).
        let url = if url.starts_with("http") {
            url
        } else {
            node.select(&url_sel)
                .next()
                .map(|el| el.text().collect::<String>().trim().to_string())
                .filter(|s| !s.is_empty())
                .map(|host| format!("https://{host}"))
                .unwrap_or(url)
        };

        out.push(SearchResult {
            title,
            url,
            snippet,
        });
        if out.len() >= max_results {
            break;
        }
    }
    out
}

/// DDG wraps result links in `//duckduckgo.com/l/?uddg=<percent-encoded-url>`.
/// Pull the real URL out, percent-decode it, and return; pass-through if the
/// href is already a direct URL.
fn unwrap_ddg_redirect(href: &str) -> String {
    let normalized = if let Some(stripped) = href.strip_prefix("//") {
        format!("https://{stripped}")
    } else {
        href.to_string()
    };

    let parsed = match reqwest::Url::parse(&normalized) {
        Ok(u) => u,
        Err(_) => return href.to_string(),
    };

    if parsed.host_str().unwrap_or("").contains("duckduckgo.com")
        && parsed.path().starts_with("/l/")
    {
        for (key, value) in parsed.query_pairs() {
            if key == "uddg" {
                return value.into_owned();
            }
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unwraps_ddg_redirect() {
        let raw = "//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fpath%3Fq%3D1&rut=abc";
        assert_eq!(unwrap_ddg_redirect(raw), "https://example.com/path?q=1");
    }

    #[test]
    fn passes_through_direct_urls() {
        assert_eq!(
            unwrap_ddg_redirect("https://example.com/page"),
            "https://example.com/page"
        );
    }

    #[test]
    fn parses_minimal_result_html() {
        let html = r##"
        <html><body>
          <div class="result web-result">
            <h2><a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Frust-lang.org">The Rust Lang</a></h2>
            <a class="result__snippet" href="...">Empowering everyone to build reliable software.</a>
            <a class="result__url" href="...">rust-lang.org</a>
          </div>
          <div class="result">
            <h2><a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fdocs.rs">docs.rs</a></h2>
            <a class="result__snippet" href="...">Hosts API documentation for Rust crates.</a>
          </div>
        </body></html>
        "##;
        let results = parse_ddg_html(html, 10);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].title, "The Rust Lang");
        assert_eq!(results[0].url, "https://rust-lang.org");
        assert!(results[0].snippet.contains("reliable software"));
        assert_eq!(results[1].url, "https://docs.rs");
    }

    #[test]
    fn respects_max_results() {
        let chunk = r##"<div class="result"><h2><a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com">x</a></h2></div>"##;
        let html = chunk.repeat(5);
        let results = parse_ddg_html(&format!("<html><body>{html}</body></html>"), 3);
        assert_eq!(results.len(), 3);
    }
}
