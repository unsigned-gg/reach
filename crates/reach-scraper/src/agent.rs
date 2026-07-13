//! Self-healing extraction loop.
//!
//! ```text
//! Observe → Plan → Act → Extract → Validate → Repair
//! ```
//!
//! `Observe` navigates the live browser to `url` (when requested). `Plan`/`Act`
//! attempt the requested selector. `Extract` pulls the requested shape (text,
//! HTML, attribute). `Validate` enforces a non-empty result plus an optional
//! caller-supplied regex. `Repair` falls back to [`AdaptiveMemory`] candidates
//! ranked by:
//!
//! 1. `text_hash` — primary. The visible text of an element ("Add to Cart") is
//!    the most stable identifier across redesigns.
//! 2. `dom_path` — secondary. A `:nth-child` chain survives small refactors but
//!    breaks on wrapper insertion.
//! 3. `bbox` — last resort. Layout coordinates change with the viewport.
//!
//! Successful repairs increment `successful_uses` on the matched fingerprint
//! row and persist a fresh capture so the trail self-heals over time.

use std::sync::Arc;

use anyhow::{Context, Result, anyhow, bail};
use reach_cdp::{
    CdpClient, CdpCommand,
    commands::{PageNavigate, PageNavigateResult, RuntimeEvaluate, RuntimeEvaluateResult},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::{AdaptiveMemory, ElementFingerprint, adaptive::url_components};

/// Shape of value extracted from the matched element.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExtractMode {
    /// `innerText`, trimmed.
    #[default]
    Text,
    /// `outerHTML`.
    Html,
    /// Named attribute (e.g. `href`).
    #[serde(rename = "attr")]
    Attribute { name: String },
}

/// Optional validation constraints applied to the extracted value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateOptions {
    /// Reject empty extractions when true (default).
    #[serde(default = "default_true")]
    pub non_empty: bool,
    /// Optional regex (JS-flavored, evaluated in-page) the value must match.
    #[serde(default)]
    pub matches: Option<String>,
}

impl Default for ValidateOptions {
    fn default() -> Self {
        Self {
            non_empty: true,
            matches: None,
        }
    }
}

fn default_true() -> bool {
    true
}

/// Caller-facing request for the resilient loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResilientRequest {
    pub url: String,
    pub selector: String,
    #[serde(default)]
    pub extract: ExtractMode,
    #[serde(default = "default_true")]
    pub navigate: bool,
    #[serde(default)]
    pub validate: ValidateOptions,
}

/// Result of a successful resilient extraction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResilientOutcome {
    pub url: String,
    pub selector_used: String,
    pub source: SelectorSource,
    pub repair_strategy: Option<RepairStrategy>,
    pub repair_id: Option<i64>,
    pub value: String,
    pub fingerprint: Option<ElementFingerprint>,
}

/// Whether the extraction succeeded with the original selector or a repair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectorSource {
    Original,
    Repaired,
}

/// Which fallback signal located the repaired element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairStrategy {
    TextHash,
    DomPath,
    Bbox,
}

/// Run the full Observe → … → Repair loop against a live CDP target.
pub async fn resilient_extract(
    cdp: &CdpClient,
    memory: &Arc<Mutex<AdaptiveMemory>>,
    request: &ResilientRequest,
) -> Result<ResilientOutcome> {
    let (domain, url_pattern) = url_components(&request.url)
        .ok_or_else(|| anyhow!("could not parse URL components from `{}`", request.url))?;

    if request.navigate {
        observe(cdp, &request.url).await?;
    }

    match try_selector(cdp, &request.selector, &request.extract, &request.validate).await? {
        AttemptOutcome::Hit { value, fingerprint } => {
            debug!(selector = %request.selector, "original selector matched");
            persist_capture(
                memory,
                &domain,
                &url_pattern,
                &request.selector,
                &fingerprint,
            )
            .await?;
            Ok(ResilientOutcome {
                url: request.url.clone(),
                selector_used: request.selector.clone(),
                source: SelectorSource::Original,
                repair_strategy: None,
                repair_id: None,
                value,
                fingerprint: Some(fingerprint),
            })
        }
        AttemptOutcome::Miss(reason) => {
            info!(selector = %request.selector, %reason, "original selector failed; attempting repair");
            repair(cdp, memory, request, &domain, &url_pattern).await
        }
    }
}

async fn observe(cdp: &CdpClient, url: &str) -> Result<()> {
    let nav: PageNavigateResult = send(cdp, PageNavigate::new(url.to_string()))
        .await
        .context("Page.navigate during observe")?;
    if let Some(error_text) = nav.error_text {
        bail!("Page.navigate failed for {url}: {error_text}");
    }
    Ok(())
}

#[derive(Debug)]
enum AttemptOutcome {
    Hit {
        value: String,
        fingerprint: ElementFingerprint,
    },
    Miss(String),
}

async fn try_selector(
    cdp: &CdpClient,
    selector: &str,
    extract: &ExtractMode,
    validate: &ValidateOptions,
) -> Result<AttemptOutcome> {
    let script = build_attempt_script(selector, extract, validate);
    let raw: Value = evaluate_value(cdp, &script, "selector attempt").await?;

    match serde_json::from_value::<AttemptResult>(raw)? {
        AttemptResult::Ok { value, fingerprint } => Ok(AttemptOutcome::Hit { value, fingerprint }),
        AttemptResult::Err { reason } => Ok(AttemptOutcome::Miss(reason)),
    }
}

async fn repair(
    cdp: &CdpClient,
    memory: &Arc<Mutex<AdaptiveMemory>>,
    request: &ResilientRequest,
    domain: &str,
    url_pattern: &str,
) -> Result<ResilientOutcome> {
    let candidates = {
        let mem = memory.lock().await;
        mem.find_candidates(domain, url_pattern)?
    };
    if candidates.is_empty() {
        bail!(
            "no AdaptiveMemory candidates for ({domain}, {url_pattern}); selector `{}` did not match",
            request.selector
        );
    }

    let candidates_json = serde_json::to_string(&candidates).expect("candidate serialization");
    let script = build_repair_script(&candidates_json, &request.extract, &request.validate);
    let raw: Value = evaluate_value(cdp, &script, "selector repair").await?;

    let outcome = serde_json::from_value::<RepairResult>(raw)?;
    match outcome {
        RepairResult::Hit {
            value,
            selector_used,
            strategy,
            candidate_id,
            fingerprint,
        } => {
            let mut repaired_id = None;
            {
                let mem = memory.lock().await;
                if mem.record_success(candidate_id)? {
                    repaired_id = Some(candidate_id);
                }
                let fp = ElementFingerprint {
                    domain: domain.to_string(),
                    url_pattern: url_pattern.to_string(),
                    original_selector: request.selector.clone(),
                    element_tag: fingerprint.element_tag.clone(),
                    text_hash: fingerprint.text_hash.clone(),
                    dom_path: fingerprint.dom_path.clone(),
                    sibling_signature: fingerprint.sibling_signature.clone(),
                    bbox_json: fingerprint.bbox_json.clone(),
                };
                mem.save_fingerprint(&fp)?;
            }

            warn!(
                strategy = ?strategy,
                candidate_id,
                selector_used = %selector_used,
                "selector repaired"
            );

            Ok(ResilientOutcome {
                url: request.url.clone(),
                selector_used,
                source: SelectorSource::Repaired,
                repair_strategy: Some(strategy),
                repair_id: repaired_id,
                value,
                fingerprint: Some(ElementFingerprint {
                    domain: domain.to_string(),
                    url_pattern: url_pattern.to_string(),
                    original_selector: request.selector.clone(),
                    element_tag: fingerprint.element_tag,
                    text_hash: fingerprint.text_hash,
                    dom_path: fingerprint.dom_path,
                    sibling_signature: fingerprint.sibling_signature,
                    bbox_json: fingerprint.bbox_json,
                }),
            })
        }
        RepairResult::Exhausted { reason } => bail!("repair exhausted: {reason}"),
    }
}

async fn persist_capture(
    memory: &Arc<Mutex<AdaptiveMemory>>,
    domain: &str,
    url_pattern: &str,
    selector: &str,
    captured: &ElementFingerprint,
) -> Result<()> {
    let fp = ElementFingerprint {
        domain: domain.to_string(),
        url_pattern: url_pattern.to_string(),
        original_selector: selector.to_string(),
        element_tag: captured.element_tag.clone(),
        text_hash: captured.text_hash.clone(),
        dom_path: captured.dom_path.clone(),
        sibling_signature: captured.sibling_signature.clone(),
        bbox_json: captured.bbox_json.clone(),
    };
    let mem = memory.lock().await;
    mem.save_fingerprint(&fp)?;
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum AttemptResult {
    Ok {
        value: String,
        fingerprint: ElementFingerprint,
    },
    Err {
        reason: String,
    },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
enum RepairResult {
    Hit {
        value: String,
        selector_used: String,
        strategy: RepairStrategy,
        candidate_id: i64,
        fingerprint: ElementFingerprint,
    },
    Exhausted {
        reason: String,
    },
}

/// JS that probes `selector`, applies validation, and returns a Hit or Miss.
fn build_attempt_script(
    selector: &str,
    extract: &ExtractMode,
    validate: &ValidateOptions,
) -> String {
    let escaped_selector = js_string(selector);
    let extractor = extractor_js(extract);
    let validator = validator_js(validate);
    format!(
        r#"
(() => {{
  {COMMON_HELPERS}
  const extract = (el) => {{ {extractor} }};
  const validate = (v) => {{ {validator} }};
  const sel = {escaped_selector};
  const el = document.querySelector(sel);
  if (!el) return {{ kind: "err", reason: "selector did not match any element" }};
  const value = extract(el);
  if (value === null) return {{ kind: "err", reason: "could not extract value" }};
  const validated = validate(value);
  if (validated !== true) return {{ kind: "err", reason: String(validated) }};
  return {{ kind: "ok", value, fingerprint: fingerprintOf(el) }};
}})()
"#,
    )
}

/// JS that walks the candidate list, scoring each with text-hash → dom-path →
/// bbox and returning the first hit with a re-extracted value.
fn build_repair_script(
    candidates_json: &str,
    extract: &ExtractMode,
    validate: &ValidateOptions,
) -> String {
    let extractor = extractor_js(extract);
    let validator = validator_js(validate);
    format!(
        r#"
(() => {{
  {COMMON_HELPERS}
  const extract = (el) => {{ {extractor} }};
  const validate = (v) => {{ {validator} }};
  const candidates = {candidates_json};
  const tryCandidate = (c, el, strategy, selector_used) => {{
    if (!el) return null;
    if (c.element_tag && el.tagName.toLowerCase() !== c.element_tag) return null;
    const value = extract(el);
    if (value === null) return null;
    if (validate(value) !== true) return null;
    return {{
      kind: "hit",
      strategy,
      candidate_id: c.id,
      selector_used,
      value,
      fingerprint: fingerprintOf(el),
    }};
  }};

  // Strategy 1: text_hash. Walk every element and compare hash.
  // querySelectorAll('*') is fine for typical document sizes.
  const all = Array.from(document.querySelectorAll("*"));
  for (const c of candidates) {{
    for (const el of all) {{
      const text = (el.innerText || el.textContent || "").trim().slice(0, 512);
      if (!text) continue;
      if (fnv(text) !== c.text_hash) continue;
      const hit = tryCandidate(c, el, "text_hash", cssPathOf(el));
      if (hit) return hit;
    }}
  }}

  // Strategy 2: dom_path. Reuse the stored CSS path directly.
  for (const c of candidates) {{
    let el = null;
    try {{ el = document.querySelector(c.dom_path); }} catch (_) {{ el = null; }}
    const hit = tryCandidate(c, el, "dom_path", c.dom_path);
    if (hit) return hit;
  }}

  // Strategy 3: bbox neighborhood. Compute the absolute-document center
  // from the stored viewport coords + scroll offset, scroll the page so
  // the target lands in the viewport, then probe with elementFromPoint
  // (which requires viewport-relative coordinates).
  for (const c of candidates) {{
    let bbox;
    try {{ bbox = JSON.parse(c.bbox_json); }} catch (_) {{ continue; }}
    if (!bbox || typeof bbox.x !== "number") continue;
    const scrollX = typeof bbox.scroll_x === "number" ? bbox.scroll_x : 0;
    const scrollY = typeof bbox.scroll_y === "number" ? bbox.scroll_y : 0;
    const absX = bbox.x + bbox.width / 2 + scrollX;
    const absY = bbox.y + bbox.height / 2 + scrollY;
    // Center the target vertically in the current viewport.
    const targetScrollY = Math.max(0, absY - window.innerHeight / 2);
    const targetScrollX = Math.max(0, absX - window.innerWidth / 2);
    window.scrollTo({{ left: targetScrollX, top: targetScrollY, behavior: "instant" }});
    const vx = absX - window.scrollX;
    const vy = absY - window.scrollY;
    if (vx < 0 || vy < 0 || vx > window.innerWidth || vy > window.innerHeight) continue;
    const probe = document.elementFromPoint(vx, vy);
    const hit = tryCandidate(c, probe, "bbox", probe ? cssPathOf(probe) : "");
    if (hit) return hit;
  }}

  return {{ kind: "exhausted", reason: "no candidate matched any strategy" }};
}})()
"#,
    )
}

/// Body of the in-page `extract(el)` helper. Returns the extracted value or
/// `null` to signal an extraction miss.
fn extractor_js(mode: &ExtractMode) -> String {
    match mode {
        ExtractMode::Text => {
            "const t = (el.innerText || el.textContent || '').trim(); return t || null;".to_string()
        }
        ExtractMode::Html => "return el.outerHTML || null;".to_string(),
        ExtractMode::Attribute { name } => format!(
            "const a = el.getAttribute({}); return a === null ? null : a;",
            js_string(name)
        ),
    }
}

/// Body of the in-page `validate(v)` helper. Returns `true` on pass or a
/// human-readable reason string on fail.
fn validator_js(opts: &ValidateOptions) -> String {
    let non_empty = opts.non_empty;
    let regex_block = match opts.matches.as_deref() {
        Some(re) => format!(
            "const re = new RegExp({}); if (!re.test(v)) return 'value did not match regex';",
            js_string(re)
        ),
        None => String::new(),
    };
    format!(
        r#"
        if ({non_empty} && (!v || (typeof v === "string" && !v.trim()))) return 'value was empty';
        {regex_block}
        return true;
    "#,
    )
}

/// Common helpers injected at the top of each script. Keeps `fnv`,
/// `fingerprintOf`, and `cssPathOf` consistent between attempt and repair.
const COMMON_HELPERS: &str = r#"
const fnv = (s) => {
  let h = 0x811c9dc5 >>> 0;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h.toString(16).padStart(8, "0");
};
const cssPathOf = (n) => {
  // Walk to documentElement so elements in <head> (e.g. <title>) get a
  // valid selector instead of being silently rooted under html>body.
  const segs = [];
  while (n && n.nodeType === 1) {
    if (n === document.documentElement) {
      segs.unshift("html");
      break;
    }
    const tag = n.tagName.toLowerCase();
    const parent = n.parentElement;
    if (!parent) break;
    const idx = Array.from(parent.children).indexOf(n) + 1;
    segs.unshift(tag + ":nth-child(" + idx + ")");
    n = parent;
  }
  return segs.join(">");
};
const fingerprintOf = (el) => {
  const text = (el.innerText || el.textContent || "").trim().slice(0, 512);
  const rect = el.getBoundingClientRect();
  const p = el.parentElement;
  const sibs = p
    ? Array.from(p.children).map((c) => c.tagName.toLowerCase()).join("+")
    : "";
  return {
    domain: location.host,
    url_pattern: location.pathname || "/",
    original_selector: "",
    element_tag: el.tagName.toLowerCase(),
    text_hash: fnv(text),
    dom_path: cssPathOf(el),
    sibling_signature: sibs,
    bbox_json: JSON.stringify({
      x: Math.round(rect.x), y: Math.round(rect.y),
      width: Math.round(rect.width), height: Math.round(rect.height),
      scroll_x: Math.round(window.scrollX),
      scroll_y: Math.round(window.scrollY),
    }),
  };
};
"#;

/// Serialize `value` as a JavaScript string literal.
///
/// Starts from `serde_json::to_string` (which escapes quotes, backslashes,
/// control chars, and emits valid JSON escapes), then post-processes
/// `U+2028` / `U+2029`. Those are valid in JSON strings but JavaScript treats
/// them as line terminators, which would silently break a `format!`-embedded
/// expression if the input ever contained one.
fn js_string(value: &str) -> String {
    let json = serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string());
    json.replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

async fn evaluate_value(cdp: &CdpClient, expression: &str, ctx: &str) -> Result<Value> {
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

    result
        .result
        .value
        .ok_or_else(|| anyhow!("Runtime.evaluate ({ctx}) returned no value"))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn js_string_escapes_quotes_and_backslashes() {
        assert_eq!(js_string(r#"hello"world"#), r#""hello\"world""#);
        assert_eq!(js_string(r"path\to"), r#""path\\to""#);
    }

    #[test]
    fn js_string_escapes_control_chars_and_line_separators() {
        // serde_json escapes \n and the U+2028 / U+2029 separators that JS
        // treats as line terminators, so values with them stay valid in a
        // JS source position.
        let with_newline = js_string("a\nb");
        assert!(with_newline.contains("\\n"));
        assert!(!with_newline.contains('\n'));

        let with_ls = js_string("a\u{2028}b");
        // U+2028 must not appear unescaped in the output.
        assert!(!with_ls.contains('\u{2028}'));
    }

    #[test]
    fn extractor_js_text_returns_string() {
        assert!(extractor_js(&ExtractMode::Text).contains("innerText"));
    }

    #[test]
    fn extractor_js_attribute_inlines_attr_name() {
        let js = extractor_js(&ExtractMode::Attribute {
            name: "href".into(),
        });
        assert!(js.contains("getAttribute(\"href\")"));
    }

    #[test]
    fn validator_js_renders_regex_clause_when_provided() {
        let js = validator_js(&ValidateOptions {
            non_empty: true,
            matches: Some("^foo$".into()),
        });
        assert!(js.contains("new RegExp"));
        assert!(js.contains("^foo$"));
    }

    #[test]
    fn validator_js_skips_regex_when_absent() {
        let js = validator_js(&ValidateOptions {
            non_empty: true,
            matches: None,
        });
        assert!(!js.contains("new RegExp"));
    }
}
