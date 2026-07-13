# reach-scraper

Rust-native scraping primitives for Reach.

## Purpose

`reach-scraper` provides reusable library building blocks for fetching web pages
from a Reach sandbox:

- `StaticFetcher` fetches pages with `reqwest`, supports proxies, follows
  redirects, and keeps a cookie jar.
- `CdpFetcher` navigates Chrome through `reach-cdp` and extracts rendered HTML.
- `HybridFetcher` starts with the static path and escalates to CDP when common
  bot-block responses such as `403` or `429` are returned.
- `ReachScraper` is a small facade for callers that want a single CDP-backed
  scrape entry point.

The crate uses `anyhow` for contextual errors and `tracing` for scrape,
escalation, and cookie-transfer events.

## Fit in Reach

Reach gives agents a containerized desktop with Chrome available through
`reach-browserd`. `reach-scraper` sits above that browser bridge and below any
future user-facing command or MCP tool layer:

```text
future reach-cli/MCP tools
        |
        v
reach-scraper
        |
        +-- static HTTP via reqwest
        |
        +-- browser escalation via reach-cdp
```

This crate is intentionally not wired into `reach-cli` MCP tools yet. Keeping it
as a library boundary makes the upstream pull request easier to review and lets
the MCP integration happen separately.

## Example

```rust,no_run
use anyhow::Result;
use reach_cdp::CdpClient;
use reach_scraper::{ReachScraper, ScrapeRequest};

#[tokio::main]
async fn main() -> Result<()> {
    let cdp = CdpClient::connect("http://127.0.0.1:8401").await?;
    let scraper = ReachScraper::new(cdp);
    let output = scraper
        .scrape(ScrapeRequest {
            url: "https://example.com".to_string(),
            proxy: None,
        })
        .await?;

    println!("fetched {} bytes", output.content.unwrap_or_default().len());
    Ok(())
}
```

