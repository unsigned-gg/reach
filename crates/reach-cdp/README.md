# reach-cdp

Typed Rust client for the `reach-browserd` Chrome DevTools Protocol HTTP bridge.

## Purpose

`reach-cdp` keeps browser automation protocol details out of higher-level Reach
crates. It provides:

- `CdpClient`, an HTTP client for the `reach-browserd` `/cdp` endpoint.
- `CdpCommand`, a trait for typed CDP command builders.
- Typed wrappers for the CDP methods currently needed by Reach scraping code,
  including `Page.navigate`, `Runtime.evaluate`, `Network.enable`,
  `Network.getCookies`, and `Network.getResponseBody`.
- `RawCdpCommand` for protocol calls that do not have typed wrappers yet.

The crate uses `anyhow` for contextual error propagation and `tracing` for
command-level observability.

## Fit in Reach

Reach runs browser-capable sandbox containers. Inside those containers,
`reach-browserd` exposes a small HTTP API that forwards CDP commands to Chrome.
This crate is the host-side Rust library for talking to that bridge.

The intended dependency direction is:

```text
reach-scraper -> reach-cdp -> reach-browserd HTTP API -> Chrome/CDP
```

`reach-cdp` should stay independent of `reach-cli` and MCP concerns. The CLI can
wire these capabilities into tools later, but this crate only models and sends
CDP commands.

## Example

```rust,no_run
use anyhow::Result;
use reach_cdp::{CdpClient, commands::{PageNavigate, PageNavigateResult}};

#[tokio::main]
async fn main() -> Result<()> {
    let cdp = CdpClient::connect("http://127.0.0.1:8401").await?;
    let response = cdp
        .send::<_, PageNavigateResult>(PageNavigate::new("https://example.com"))
        .await?;
    let navigation = response.into_result()?;

    println!("navigated frame {}", navigation.frame_id);
    Ok(())
}
```

