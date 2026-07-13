//! Browser fingerprint spoofing.
//!
//! Combines CDP `Emulation.*` overrides (UA, hardware concurrency, locale,
//! timezone, device metrics, touch) with a JS shim installed via
//! `Page.addScriptToEvaluateOnNewDocument` that runs *before* any page
//! script. The shim normalizes `navigator.webdriver`, plugins/mimeTypes,
//! permission queries, WebGL vendor/renderer, screen.*, deviceMemory, and
//! adds a plausible `window.chrome.runtime` shape — the surface that almost
//! every modern bot detector probes.
//!
//! The shim avoids canvas/audio noise injection on purpose: those defenses
//! are easy to detect via differential reads and frequently break legitimate
//! features. Add them only if you can validate against a real fingerprinting
//! suite first.
//!
//! # Limitations
//!
//! - **TLS / JA3 / HTTP-2 fingerprint** are server-visible before any JS
//!   runs. CDP cannot influence them.
//! - **GPU shader timing**: a determined adversary can compile a probe
//!   shader and time it; the real GPU still leaks.
//! - **WebGPU adapter info**: not yet patched here.

use anyhow::{Context, Result, anyhow};
use reach_cdp::{
    CdpClient, CdpCommand,
    commands::{
        EmulationSetDeviceMetricsOverride, EmulationSetDeviceMetricsOverrideResult,
        EmulationSetHardwareConcurrencyOverride, EmulationSetHardwareConcurrencyOverrideResult,
        EmulationSetLocaleOverride, EmulationSetLocaleOverrideResult, EmulationSetTimezoneOverride,
        EmulationSetTimezoneOverrideResult, EmulationSetTouchEmulationEnabled,
        EmulationSetTouchEmulationEnabledResult, EmulationSetUserAgentOverride,
        EmulationSetUserAgentOverrideResult, NetworkEnable, NetworkEnableResult,
        NetworkSetExtraHttpHeaders, NetworkSetExtraHttpHeadersResult,
        PageAddScriptToEvaluateOnNewDocument, PageAddScriptToEvaluateOnNewDocumentResult,
        PageEnable, PageEnableResult, UserAgentBrand, UserAgentMetadata,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::debug;

/// A single browser fingerprint preset.
///
/// Every field is part of one consistent profile — mixing values across
/// profiles (mac UA + windows GPU strings) is the fastest way to *fail* a
/// fingerprint check, so callers should pick a profile and apply it whole.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FingerprintProfile {
    /// Stable identifier, e.g. `"windows-chrome-128"`.
    pub id: String,
    pub user_agent: String,
    pub accept_language: String,
    pub languages: Vec<String>,
    pub locale: String,
    pub timezone: String,
    /// `navigator.platform` value (`"Win32"`, `"MacIntel"`, `"Linux x86_64"`).
    pub platform: String,
    /// CDP `Emulation.setUserAgentOverride.platform` override.
    pub ua_platform: String,
    pub ua_platform_version: String,
    pub ua_architecture: String,
    pub ua_model: String,
    pub ua_mobile: bool,
    pub ua_bitness: String,
    pub ua_brands: Vec<(String, String)>,
    pub ua_full_version_list: Vec<(String, String)>,
    pub hardware_concurrency: u32,
    /// Must be one of: 0.25, 0.5, 1, 2, 4, 8 (Chrome's allowed values for
    /// `navigator.deviceMemory`). We round to the nearest valid bucket.
    pub device_memory: f32,
    pub viewport_width: u32,
    pub viewport_height: u32,
    pub screen_width: u32,
    pub screen_height: u32,
    pub screen_color_depth: u32,
    pub device_pixel_ratio: f32,
    /// What `WebGLRenderingContext.getParameter(UNMASKED_VENDOR_WEBGL)` returns.
    pub webgl_vendor: String,
    /// What `WebGLRenderingContext.getParameter(UNMASKED_RENDERER_WEBGL)` returns.
    pub webgl_renderer: String,
    /// Touch points — 0 disables touch.
    pub max_touch_points: u8,
}

impl FingerprintProfile {
    /// Look up a built-in profile by id.
    pub fn by_id(id: &str) -> Option<Self> {
        match id {
            "windows-chrome-128" => Some(profile_windows_chrome()),
            "mac-chrome-128" => Some(profile_mac_chrome()),
            "linux-chrome-128" => Some(profile_linux_chrome()),
            _ => None,
        }
    }

    /// Names of every built-in profile.
    pub fn builtin_ids() -> &'static [&'static str] {
        &["windows-chrome-128", "mac-chrome-128", "linux-chrome-128"]
    }

    fn metadata(&self) -> UserAgentMetadata {
        UserAgentMetadata {
            brands: self
                .ua_brands
                .iter()
                .map(|(b, v)| UserAgentBrand {
                    brand: b.clone(),
                    version: v.clone(),
                })
                .collect(),
            full_version_list: Some(
                self.ua_full_version_list
                    .iter()
                    .map(|(b, v)| UserAgentBrand {
                        brand: b.clone(),
                        version: v.clone(),
                    })
                    .collect(),
            ),
            platform: self.ua_platform.clone(),
            platform_version: self.ua_platform_version.clone(),
            architecture: self.ua_architecture.clone(),
            model: self.ua_model.clone(),
            mobile: self.ua_mobile,
            bitness: Some(self.ua_bitness.clone()),
            wow64: Some(false),
        }
    }
}

fn brands_windows() -> Vec<(String, String)> {
    vec![
        ("Not_A Brand".into(), "8".into()),
        ("Chromium".into(), "128".into()),
        ("Google Chrome".into(), "128".into()),
    ]
}

fn full_versions_windows() -> Vec<(String, String)> {
    vec![
        ("Not_A Brand".into(), "8.0.0.0".into()),
        ("Chromium".into(), "128.0.6613.85".into()),
        ("Google Chrome".into(), "128.0.6613.85".into()),
    ]
}

/// Windows 10 / 11 desktop on Chrome 128, integrated Intel UHD GPU.
pub fn profile_windows_chrome() -> FingerprintProfile {
    FingerprintProfile {
        id: "windows-chrome-128".into(),
        user_agent:
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) \
             Chrome/128.0.0.0 Safari/537.36"
                .into(),
        accept_language: "en-US,en;q=0.9".into(),
        languages: vec!["en-US".into(), "en".into()],
        locale: "en-US".into(),
        timezone: "America/New_York".into(),
        platform: "Win32".into(),
        ua_platform: "Windows".into(),
        ua_platform_version: "15.0.0".into(),
        ua_architecture: "x86".into(),
        ua_model: String::new(),
        ua_mobile: false,
        ua_bitness: "64".into(),
        ua_brands: brands_windows(),
        ua_full_version_list: full_versions_windows(),
        hardware_concurrency: 8,
        device_memory: 8.0,
        viewport_width: 1280,
        viewport_height: 720,
        screen_width: 1920,
        screen_height: 1080,
        screen_color_depth: 24,
        device_pixel_ratio: 1.0,
        webgl_vendor: "Google Inc. (Intel)".into(),
        webgl_renderer: "ANGLE (Intel, Intel(R) UHD Graphics 630 Direct3D11 vs_5_0 ps_5_0, D3D11)"
            .into(),
        max_touch_points: 0,
    }
}

/// macOS desktop on Chrome 128, Apple Silicon GPU.
pub fn profile_mac_chrome() -> FingerprintProfile {
    FingerprintProfile {
        id: "mac-chrome-128".into(),
        user_agent:
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like \
             Gecko) Chrome/128.0.0.0 Safari/537.36"
                .into(),
        accept_language: "en-US,en;q=0.9".into(),
        languages: vec!["en-US".into(), "en".into()],
        locale: "en-US".into(),
        timezone: "America/Los_Angeles".into(),
        platform: "MacIntel".into(),
        ua_platform: "macOS".into(),
        ua_platform_version: "14.5.0".into(),
        ua_architecture: "arm".into(),
        ua_model: String::new(),
        ua_mobile: false,
        ua_bitness: "64".into(),
        ua_brands: brands_windows(),
        ua_full_version_list: full_versions_windows(),
        hardware_concurrency: 10,
        device_memory: 8.0,
        viewport_width: 1280,
        viewport_height: 800,
        screen_width: 1728,
        screen_height: 1117,
        screen_color_depth: 30,
        device_pixel_ratio: 2.0,
        webgl_vendor: "Google Inc. (Apple)".into(),
        webgl_renderer: "ANGLE (Apple, ANGLE Metal Renderer: Apple M2, Unspecified Version)".into(),
        max_touch_points: 0,
    }
}

/// Linux desktop on Chrome 128, NVIDIA discrete GPU.
pub fn profile_linux_chrome() -> FingerprintProfile {
    FingerprintProfile {
        id: "linux-chrome-128".into(),
        user_agent: "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) \
                     Chrome/128.0.0.0 Safari/537.36"
            .into(),
        accept_language: "en-US,en;q=0.9".into(),
        languages: vec!["en-US".into(), "en".into()],
        locale: "en-US".into(),
        timezone: "America/Chicago".into(),
        platform: "Linux x86_64".into(),
        ua_platform: "Linux".into(),
        ua_platform_version: "6.5.0".into(),
        ua_architecture: "x86".into(),
        ua_model: String::new(),
        ua_mobile: false,
        ua_bitness: "64".into(),
        ua_brands: brands_windows(),
        ua_full_version_list: full_versions_windows(),
        hardware_concurrency: 12,
        device_memory: 8.0,
        viewport_width: 1280,
        viewport_height: 720,
        screen_width: 1920,
        screen_height: 1080,
        screen_color_depth: 24,
        device_pixel_ratio: 1.0,
        webgl_vendor: "Google Inc. (NVIDIA Corporation)".into(),
        webgl_renderer:
            "ANGLE (NVIDIA Corporation, NVIDIA GeForce RTX 3060/PCIe/SSE2, OpenGL 4.6.0)".into(),
        max_touch_points: 0,
    }
}

/// Apply `profile` to a CDP session. Order matters — UA + headers + emulation
/// land first, then `Page.enable` so the new-document script registers before
/// the next navigation.
pub async fn apply_profile(cdp: &CdpClient, profile: &FingerprintProfile) -> Result<()> {
    debug!(profile_id = %profile.id, "applying stealth profile");

    let _: NetworkEnableResult = send(cdp, NetworkEnable::new())
        .await
        .context("Network.enable")?;
    let _: PageEnableResult = send(cdp, PageEnable::new()).await.context("Page.enable")?;

    let _: EmulationSetUserAgentOverrideResult = send(
        cdp,
        EmulationSetUserAgentOverride::new(&profile.user_agent)
            .with_accept_language(&profile.accept_language)
            .with_platform(&profile.ua_platform)
            .with_metadata(profile.metadata()),
    )
    .await
    .context("Emulation.setUserAgentOverride")?;

    let _: EmulationSetHardwareConcurrencyOverrideResult = send(
        cdp,
        EmulationSetHardwareConcurrencyOverride::new(profile.hardware_concurrency),
    )
    .await
    .context("Emulation.setHardwareConcurrencyOverride")?;

    let _: EmulationSetLocaleOverrideResult =
        send(cdp, EmulationSetLocaleOverride::new(&profile.locale))
            .await
            .context("Emulation.setLocaleOverride")?;

    let _: EmulationSetTimezoneOverrideResult =
        send(cdp, EmulationSetTimezoneOverride::new(&profile.timezone))
            .await
            .context("Emulation.setTimezoneOverride")?;

    let _: EmulationSetDeviceMetricsOverrideResult = send(
        cdp,
        EmulationSetDeviceMetricsOverride::new(
            profile.viewport_width,
            profile.viewport_height,
            profile.device_pixel_ratio,
            profile.ua_mobile,
        )
        .with_screen(profile.screen_width, profile.screen_height),
    )
    .await
    .context("Emulation.setDeviceMetricsOverride")?;

    // Chrome rejects `maxTouchPoints: 0`; only send the field when touch is on.
    let touch_max = if profile.max_touch_points > 0 {
        Some(profile.max_touch_points)
    } else {
        None
    };
    let _: EmulationSetTouchEmulationEnabledResult = send(
        cdp,
        EmulationSetTouchEmulationEnabled::new(profile.max_touch_points > 0, touch_max),
    )
    .await
    .context("Emulation.setTouchEmulationEnabled")?;

    let headers = json!({
        "Accept-Language": profile.accept_language,
        "Sec-CH-UA-Platform": format!("\"{}\"", profile.ua_platform),
        "Sec-CH-UA-Mobile": if profile.ua_mobile { "?1" } else { "?0" },
    });
    let _: NetworkSetExtraHttpHeadersResult = send(cdp, NetworkSetExtraHttpHeaders::new(headers))
        .await
        .context("Network.setExtraHTTPHeaders")?;

    let shim = build_init_script(profile)?;
    let _: PageAddScriptToEvaluateOnNewDocumentResult =
        send(cdp, PageAddScriptToEvaluateOnNewDocument::new(shim))
            .await
            .context("Page.addScriptToEvaluateOnNewDocument")?;

    Ok(())
}

/// Render the JS shim with the profile's values baked in.
fn build_init_script(profile: &FingerprintProfile) -> Result<String> {
    let profile_json = serde_json::to_string(&InitScriptProfile {
        languages: &profile.languages,
        platform: &profile.platform,
        device_memory: profile.device_memory,
        screen_width: profile.screen_width,
        screen_height: profile.screen_height,
        color_depth: profile.screen_color_depth,
        webgl_vendor: &profile.webgl_vendor,
        webgl_renderer: &profile.webgl_renderer,
        max_touch_points: profile.max_touch_points,
    })
    .map_err(|e| anyhow!("serializing init-script profile: {e}"))?;

    Ok(STEALTH_TEMPLATE.replace("__PROFILE_JSON__", &profile_json))
}

#[derive(Serialize)]
struct InitScriptProfile<'a> {
    languages: &'a [String],
    platform: &'a str,
    device_memory: f32,
    screen_width: u32,
    screen_height: u32,
    color_depth: u32,
    webgl_vendor: &'a str,
    webgl_renderer: &'a str,
    max_touch_points: u8,
}

/// JS shim that runs on every new document.
const STEALTH_TEMPLATE: &str = r#"
(() => {
  const P = __PROFILE_JSON__;
  const def = (obj, prop, val) => {
    try { Object.defineProperty(obj, prop, { get: () => val, configurable: true }); }
    catch (_) {}
  };

  // navigator.webdriver — the canary every detector checks.
  def(Navigator.prototype, 'webdriver', false);

  // navigator.languages — array form (sec-ch-ua-language already covered by header).
  def(Navigator.prototype, 'languages', P.languages);

  // navigator.platform / oscpu.
  def(Navigator.prototype, 'platform', P.platform);

  // navigator.deviceMemory — Chrome rounds to {0.25, 0.5, 1, 2, 4, 8}.
  const memBuckets = [0.25, 0.5, 1, 2, 4, 8];
  let mem = P.device_memory;
  mem = memBuckets.reduce((a, b) => Math.abs(b - mem) < Math.abs(a - mem) ? b : a, 8);
  def(Navigator.prototype, 'deviceMemory', mem);

  // navigator.maxTouchPoints — coherent with Emulation.setTouchEmulationEnabled.
  def(Navigator.prototype, 'maxTouchPoints', P.max_touch_points);

  // window.chrome shape — bot detectors check chrome.runtime existence on all
  // non-extension contexts.
  if (!window.chrome) {
    Object.defineProperty(window, 'chrome', {
      writable: true, configurable: true,
      value: { runtime: {}, app: { isInstalled: false }, csi: () => {}, loadTimes: () => {} },
    });
  } else if (!window.chrome.runtime) {
    try { window.chrome.runtime = {}; } catch (_) {}
  }

  // permissions.query — headless Chrome returns 'denied' for notifications even
  // when Notification.permission is 'default'. Reconcile.
  const permsProto = (navigator.permissions || {}).constructor && (navigator.permissions || {}).constructor.prototype;
  if (navigator.permissions && navigator.permissions.query) {
    const orig = navigator.permissions.query.bind(navigator.permissions);
    navigator.permissions.query = (params) =>
      params && params.name === 'notifications'
        ? Promise.resolve({ state: Notification.permission, onchange: null })
        : orig(params);
  }
  void permsProto; // silence unused

  // plugins / mimeTypes — empty PluginArray is a strong bot signal. Install a
  // realistic-looking trio (Chrome PDF Viewer + native client + native helper)
  // matching real Chrome's defaults.
  const fakePdfPlugin = Object.create(Plugin.prototype, {
    name: { value: 'Chrome PDF Plugin', enumerable: true },
    description: { value: 'Portable Document Format', enumerable: true },
    filename: { value: 'internal-pdf-viewer', enumerable: true },
    length: { value: 1, enumerable: true },
  });
  const fakeMime = Object.create(MimeType.prototype, {
    type: { value: 'application/pdf', enumerable: true },
    suffixes: { value: 'pdf', enumerable: true },
    description: { value: '', enumerable: true },
    enabledPlugin: { value: fakePdfPlugin, enumerable: true },
  });
  const pluginArr = Object.create(PluginArray.prototype, {
    length: { value: 1, enumerable: true },
    0: { value: fakePdfPlugin, enumerable: true },
  });
  const mimeArr = Object.create(MimeTypeArray.prototype, {
    length: { value: 1, enumerable: true },
    0: { value: fakeMime, enumerable: true },
  });
  def(Navigator.prototype, 'plugins', pluginArr);
  def(Navigator.prototype, 'mimeTypes', mimeArr);

  // screen.* — match the device profile so screen != viewport gives nothing away.
  def(Screen.prototype, 'width', P.screen_width);
  def(Screen.prototype, 'height', P.screen_height);
  def(Screen.prototype, 'availWidth', P.screen_width);
  def(Screen.prototype, 'availHeight', P.screen_height - 40);
  def(Screen.prototype, 'colorDepth', P.color_depth);
  def(Screen.prototype, 'pixelDepth', P.color_depth);

  // WebGL VENDOR / RENDERER / UNMASKED_*.
  const patchGetParameter = (proto) => {
    if (!proto || !proto.getParameter) return;
    const orig = proto.getParameter;
    proto.getParameter = function (p) {
      // UNMASKED_VENDOR_WEBGL = 0x9245, UNMASKED_RENDERER_WEBGL = 0x9246
      if (p === 0x9245) return P.webgl_vendor;
      if (p === 0x9246) return P.webgl_renderer;
      // VENDOR = 0x1F00, RENDERER = 0x1F01 — Chrome's stock answers.
      if (p === 0x1F00) return 'WebKit';
      if (p === 0x1F01) return 'WebKit WebGL';
      return orig.call(this, p);
    };
  };
  if (typeof WebGLRenderingContext !== 'undefined') patchGetParameter(WebGLRenderingContext.prototype);
  if (typeof WebGL2RenderingContext !== 'undefined') patchGetParameter(WebGL2RenderingContext.prototype);

  // Hide the patches themselves: toString() should report native code so a
  // detector dumping `Function.prototype.toString.call(navigator.permissions.query)`
  // doesn't see our shim.
  const nativeToString = Function.prototype.toString;
  const patched = new WeakSet();
  const tagNative = (fn) => { try { patched.add(fn); } catch (_) {} return fn; };
  Function.prototype.toString = new Proxy(nativeToString, {
    apply(target, thisArg, args) {
      if (patched.has(thisArg)) return 'function ' + (thisArg.name || '') + '() { [native code] }';
      return Reflect.apply(target, thisArg, args);
    }
  });
  tagNative(navigator.permissions && navigator.permissions.query);
})();
"#;

async fn send<C, R>(cdp: &CdpClient, command: C) -> Result<R>
where
    C: CdpCommand,
    R: serde::de::DeserializeOwned,
{
    let method = command.method();
    cdp.send::<_, R>(command)
        .await?
        .into_result()
        .map_err(|e| anyhow!("CDP {method} failed: {}", e.message))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_profiles_are_lookupable() {
        for id in FingerprintProfile::builtin_ids() {
            assert!(FingerprintProfile::by_id(id).is_some(), "missing {id}");
        }
        assert!(FingerprintProfile::by_id("nonsense").is_none());
    }

    #[test]
    fn init_script_substitutes_profile_json() {
        let p = profile_windows_chrome();
        let script = build_init_script(&p).unwrap();
        assert!(!script.contains("__PROFILE_JSON__"));
        assert!(script.contains("Win32"));
        assert!(script.contains("UHD Graphics"));
    }

    #[test]
    fn windows_profile_has_consistent_ua_and_metadata() {
        let p = profile_windows_chrome();
        assert!(p.user_agent.contains("Windows NT 10.0"));
        assert_eq!(p.platform, "Win32");
        assert_eq!(p.ua_platform, "Windows");
        assert_eq!(p.hardware_concurrency, 8);
    }

    #[test]
    fn mac_profile_marks_arm_architecture() {
        let p = profile_mac_chrome();
        assert!(p.user_agent.contains("Mac OS X"));
        assert_eq!(p.platform, "MacIntel");
        assert_eq!(p.ua_architecture, "arm");
    }
}
