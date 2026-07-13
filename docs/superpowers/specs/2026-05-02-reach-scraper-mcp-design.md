# Reach Scraper: MCP Wiring, Agent Loop, CDP Proxy Contexts

**Status:** approved
**Date:** 2026-05-02
**Order:** 1 → 2 → 3 → 4 (reordered from initial 3→1→2→4 after Codex+Gemini review)

## Goal

Land the next phase of `reach-scraper` integration: expose the scraper through MCP tools, build the self-healing agent loop on top of `AdaptiveMemory`, then unlock real per-request proxy support via CDP browser contexts.

## Non-goals

- Not redesigning the legacy `scrape` (Scrapling) tool in `mcp.rs`. New tools take new names.
- Not adding visual/embedding-based fingerprints. Text-hash + DOM-path + bbox only for now.
- Not productionizing proxy auth interception beyond a documented hook.

---

## Step 1 — Wire `reach-scraper` into MCP

### New tools (`crates/reach-cli/src/mcp.rs`)

| Tool | Params | Backend |
|------|--------|---------|
| `scrape_static` | `url`, `proxy?`, `sandbox?` | `StaticFetcher` (one-shot, returns `ScrapeOutput` JSON) |
| `scrape_agent` | `url`, `proxy?`, `escalate?: bool = true`, `sandbox?` | `HybridFetcher` (escalates on 403/429) |
| `scrape_learn` | `url`, `selector`, `sandbox?` | Capture `ElementFingerprint` via CDP, persist to `AdaptiveMemory` |
| `scrape_recover` | `url`, `selector`, `sandbox?` | Query candidates by `(domain, url_pattern)`; returns ranked list (no repair attempt yet — that's Step 2) |

`proxy?` is parsed but documented as **deferred until Step 3** for the CDP/agent paths. Static path uses it today via reqwest.

### Server state

```rust
struct McpState {
    docker: DockerClient,            // existing
    scraper_memory: Arc<Mutex<AdaptiveMemory>>,
    scraper_lock: Arc<Mutex<()>>,    // serializes CDP scrape calls until Step 3
}
```

- `AdaptiveMemory` opens at `~/.local/share/reach/adaptive.sqlite`, override via `config.toml [scraper] memory_path`.
- `init_db` runs `PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA user_version=1;` so concurrent `reach-cli` processes don't deadlock and we have a migration anchor.
- `scraper_lock`: every `scrape_agent`/`scrape_learn`/`scrape_recover` invocation holds it. Lifted in Step 3 once browser contexts land.

### Sandbox → CdpClient resolution

Sandbox tools must resolve a `sandbox` name → its host-side published port for `8401`. Reuse the existing `connect.rs` port lookup logic. If sandbox not running, return MCP error.

### Files touched

- `crates/reach-cli/Cargo.toml` (add `reach-scraper`, `reach-cdp` deps; no feature gate — scraping is a core capability, rusqlite is already `bundled`)
- `crates/reach-cli/src/mcp.rs` (new `ToolCall` variants + tool definitions + dispatch)
- `crates/reach-cli/src/commands/serve.rs` (build `McpState`)
- `crates/reach-cli/src/config.rs` (add `[scraper] memory_path`)
- `crates/reach-scraper/src/adaptive.rs` (WAL pragmas + user_version)

---

## Step 2 — Agent loop (Observe → Plan → Act → Extract → Validate → Repair)

### New AdaptiveMemory APIs

- `record_success(id: i64)` — increment `successful_uses`, bump `last_used_at`.
- `record_fingerprint_for_selector(domain, url_pattern, selector, captured: ElementFingerprint)` — upsert, used during `scrape_learn` and after a successful repair.
- `prune_stale(domain, max_per_selector: usize)` — TTL/LRU prune, optional.

### Loop (`crates/reach-scraper/src/agent.rs`, new module)

```
Observe   ─► capture DOM via Runtime.evaluate (querySelector, walk parents, hash innerText)
Plan      ─► attempt original selector
Act       ─► run selector against current DOM
Extract   ─► return innerText / attr / outerHTML per ExtractMode
Validate  ─► non-empty assertion + optional shape predicate (regex / JSON schema)
Repair    ─► on miss: load AdaptiveMemory candidates → score → try each → on hit, update memory + return
```

### Repair scoring order (revised after Gemini review)

1. **`text_hash`** — primary. Visible text ("Add to Cart") is the most semantically stable identifier across redesigns.
2. **`dom_path`** — secondary. Falls over when wrappers get inserted, but still valid signal for generic text.
3. **`bbox` neighborhood** — last resort. Breaks on viewport/responsive changes.

For each candidate, run `Runtime.evaluate` to test the strategy in-page. First hit wins. On hit:
- `record_success(id)`
- capture fresh fingerprint, upsert (so the selector trail self-heals over time)
- return new selector + extracted value

### Where the loop runs

Host-side, inside `reach-cli`. Each phase is one CDP round-trip via `reach-browserd`. AdaptiveMemory is host-side already; co-locating the loop avoids cross-boundary state.

### MCP surface

Step 1's `scrape_recover` becomes "candidate query"; add `scrape_resilient {url, selector, extract, validate?}` that wraps the full loop and is the user-facing entry point.

---

## Step 3 — CDP per-request proxy via browser contexts

### Browserd refactor

**Single actor, multi-session.** Keep the one `CdpActor` managing the browser-level WS at `/devtools/browser/<id>` (`/json/version`'s `webSocketDebuggerUrl` already returns this — verified). All sessions multiplex over it.

```rust
struct CdpActor {
    req_rx: mpsc::Receiver<ActorMessage>,
    pending: HashMap<u64, oneshot::Sender<Value>>,         // global by message id
    sessions: HashMap<SessionId, SessionMeta>,             // sessionId → context, target
    default_session: SessionId,                            // current healing session for back-compat
}
```

**Critical change:** stop tearing down the inner WS loop on `"Session with given id not found"`. Route the error to the specific caller's `oneshot`; only the *default* session triggers reconnect on loss.

### HTTP API

- `POST /cdp` — body extended to `{method, params, sessionId?}`. `sessionId` omitted = default session (back-compat).
- `POST /cdp/context/create` — body `{proxyServer?, proxyBypassList?, url?}`. Returns `{browserContextId, targetId, sessionId}`.
- `POST /cdp/context/dispose` — body `{browserContextId}`. Disposes context + detaches sessions.
- On startup and on WS reconnect, browserd lists existing contexts and disposes orphans (best-effort).

### CdpClient + CdpFetcher

- `CdpClient::with_session(sessionId)` builder.
- `CdpFetcher::configure_proxy(proxy)` becomes real:
  1. Mint context via `/cdp/context/create`.
  2. Returns `CdpContextGuard` with **explicit** `close().await` (no async Drop).
  3. Subsequent CDP calls in the fetcher route through the new sessionId.
- Proxy auth (`username`/`password`): out-of-scope for first cut. Document via `Fetch.enable + Fetch.authRequired` interception as a follow-up; for now, reject `ProxyConfig` with credentials on the CDP path with a clear error.

### Lift the Mutex

Once contexts isolate page state, `scraper_lock` from Step 1 is removed.

---

## Step 4 — Upstream PR

- One PR, three stacked commits matching the steps above.
- Per-commit gates: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo deny check`, `cargo test --workspace`, `cargo test --workspace -- --ignored` (e2e).
- PR description summarizes the architecture diagram + caveats (proxy auth deferred, Mutex removed in commit 3).

---

## Risk register

| Risk | Mitigation |
|------|------------|
| Browserd reconnect drops mid-flight commands and HTTP layer blindly retries | Classify retry-safe methods; non-idempotent ops surface reconnect error to caller |
| Concurrent `scrape_*` clobber single page target | `scraper_lock` Mutex (Step 1), removed when contexts ship (Step 3) |
| AdaptiveMemory schema evolution | `PRAGMA user_version=1` from day one; migration switch in `init_db` |
| Async Drop unreliable for context cleanup | Explicit `close().await` + browserd orphan reaper on reconnect |
| Proxy auth not covered by `createBrowserContext` | Reject creds with explicit error in v1; document Fetch interception path |
| Repair primary signal wrong | text-hash → dom-path → bbox order locked in spec; revisit only with telemetry |

## Open questions (low-priority, can be answered during impl)

- Should `scrape_resilient` accept a `validate` JSON-schema directly or a JS predicate string?
- Per-sandbox vs host-global AdaptiveMemory — host-global wins for cross-project learning; revisit if cookies bleed.
- TTL on `element_fingerprints` — defer until we have usage data.
