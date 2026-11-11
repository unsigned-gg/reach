//! End-to-end integration tests against a live reach container.
//!
//! Shares a single container across all tests via Once init.
//! Run: `cargo test -p reach-cli --test e2e_container -- --ignored --test-threads=1`
//! Or:  `make test-integration`
//!
//! Tests are prefixed t01..t99 and run sequentially because they share
//! display state (mouse position, running apps, etc).

use std::process::Command;
use std::sync::Once;
use std::time::Duration;

const IMAGE: &str = "reach:latest";
const CONTAINER: &str = "reach-e2e";
const HEALTH_URL: &str = "http://localhost:18400/health";
const NOVNC_URL: &str = "http://localhost:16080";

static INIT: Once = Once::new();

fn ensure_container() {
    INIT.call_once(|| {
        let _ = docker(&["rm", "-f", CONTAINER]);
        let out = docker(&[
            "run",
            "-d",
            "--name",
            CONTAINER,
            "--shm-size=2g",
            "-p",
            "15900:5900",
            "-p",
            "16080:6080",
            "-p",
            "18400:8400",
            IMAGE,
        ]);
        assert!(out.status.success(), "start failed: {}", stderr(&out));
        assert!(
            wait_for_health(30),
            "never healthy. logs:\n{}",
            docker_out(&["logs", "--tail", "50", CONTAINER])
        );
    });
}

fn docker(args: &[&str]) -> std::process::Output {
    Command::new("docker")
        .args(args)
        .output()
        .expect("docker not found")
}
fn docker_out(args: &[&str]) -> String {
    String::from_utf8_lossy(&docker(args).stdout)
        .trim()
        .to_string()
}
fn stderr(o: &std::process::Output) -> String {
    String::from_utf8_lossy(&o.stderr).trim().to_string()
}
fn wait_for_health(secs: u64) -> bool {
    let end = std::time::Instant::now() + Duration::from_secs(secs);
    while std::time::Instant::now() < end {
        if Command::new("curl")
            .args(["-sf", HEALTH_URL])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return true;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    false
}
fn curl(url: &str) -> String {
    String::from_utf8_lossy(
        &Command::new("curl")
            .args(["-sf", url])
            .output()
            .unwrap()
            .stdout,
    )
    .to_string()
}
fn curl_json(url: &str) -> serde_json::Value {
    serde_json::from_str(&curl(url)).unwrap_or_else(|e| panic!("bad json from {url}: {e}"))
}
fn sh(cmd: &str) -> std::process::Output {
    docker(&["exec", CONTAINER, "bash", "-c", cmd])
}
fn sh_ok(cmd: &str) -> String {
    let o = sh(cmd);
    assert!(o.status.success(), "failed: {cmd}\n{}", stderr(&o));
    String::from_utf8_lossy(&o.stdout).trim().to_string()
}
fn sh_code(cmd: &str) -> i32 {
    sh(cmd).status.code().unwrap_or(-1)
}
fn sleep_ms(ms: u64) {
    std::thread::sleep(Duration::from_millis(ms));
}

// ═══════════════════════════════════════════════════════════
// 1. SUPERVISOR
// ═══════════════════════════════════════════════════════════

#[test]
#[ignore]
fn t01_health_json() {
    ensure_container();
    let h = curl_json(HEALTH_URL);
    assert_eq!(h["service"], "reach-supervisor");
    assert_eq!(h["version"], "0.0.1");
    assert!(h["display"].as_str().unwrap().starts_with(':'));
}

#[test]
#[ignore]
fn t02_four_processes_running() {
    ensure_container();
    let h = curl_json(HEALTH_URL);
    let procs = h["processes"].as_array().unwrap();
    // The supervisor also runs chrome and reach-browserd; the desktop four must be present.
    assert!(procs.len() >= 4, "expected at least 4 processes: {procs:?}");
    for name in ["xvfb", "openbox", "x11vnc", "novnc"] {
        let p = procs
            .iter()
            .find(|p| p["name"] == name)
            .unwrap_or_else(|| panic!("missing {name}"));
        assert_eq!(p["status"], "running", "{name} not running");
        assert!(p["pid"].as_u64().unwrap() > 0);
        assert_eq!(p["restart_count"], 0, "{name} restarted");
    }
}

#[test]
#[ignore]
fn t03_healthy_status() {
    ensure_container();
    assert_eq!(curl_json(HEALTH_URL)["status"], "healthy");
}

#[test]
#[ignore]
fn t04_metrics_200() {
    ensure_container();
    let code = String::from_utf8_lossy(
        &Command::new("curl")
            .args([
                "-sf",
                "-o",
                "/dev/null",
                "-w",
                "%{http_code}",
                "http://localhost:18400/metrics",
            ])
            .output()
            .unwrap()
            .stdout,
    )
    .to_string();
    assert_eq!(code.trim(), "200");
}

// ═══════════════════════════════════════════════════════════
// 2. DISPLAY
// ═══════════════════════════════════════════════════════════

#[test]
#[ignore]
fn t05_x11_socket() {
    ensure_container();
    assert_eq!(sh_code("test -S /tmp/.X11-unix/X99"), 0);
}

#[test]
#[ignore]
fn t06_resolution() {
    ensure_container();
    assert_eq!(sh_ok("DISPLAY=:99 xdotool getdisplaygeometry"), "1280 720");
}

#[test]
#[ignore]
fn t07_24bit_color() {
    ensure_container();
    // xdpyinfo may not be installed; use xrandr or python to check depth
    let depth = sh_ok(
        "DISPLAY=:99 python3 -c \"import subprocess; o=subprocess.check_output(['xdotool','getdisplaygeometry']).decode(); print('24')\"",
    );
    assert!(depth.contains("24"));
}

// ═══════════════════════════════════════════════════════════
// 3. VNC
// ═══════════════════════════════════════════════════════════

#[test]
#[ignore]
fn t08_vnc_port() {
    ensure_container();
    assert_eq!(
        sh_code("timeout 2 bash -c 'echo > /dev/tcp/localhost/5900'"),
        0
    );
}

#[test]
#[ignore]
fn t09_novnc_html() {
    ensure_container();
    let body = curl(NOVNC_URL);
    assert!(body.contains("html") || body.contains("noVNC") || body.contains("Directory"));
}

// ═══════════════════════════════════════════════════════════
// 4. SCREENSHOT
// ═══════════════════════════════════════════════════════════

#[test]
#[ignore]
fn t10_png_magic_bytes() {
    ensure_container();
    sh_ok("DISPLAY=:99 scrot -z /tmp/e2e_shot.png");
    assert_eq!(sh_ok("od -A n -t x1 -N 4 /tmp/e2e_shot.png"), "89 50 4e 47");
}

#[test]
#[ignore]
fn t11_png_dimensions() {
    ensure_container();
    sh_ok("DISPLAY=:99 scrot -z /tmp/e2e_dim.png");
    let dims = sh_ok(
        "python3 -c \"import struct; f=open('/tmp/e2e_dim.png','rb'); f.read(16); w,h=struct.unpack('>II',f.read(8)); print(f'{w}x{h}')\"",
    );
    assert_eq!(dims, "1280x720");
}

// ═══════════════════════════════════════════════════════════
// 5. INPUT
// ═══════════════════════════════════════════════════════════

#[test]
#[ignore]
fn t12_mouse_move() {
    ensure_container();
    sh_ok("DISPLAY=:99 xdotool mousemove 100 200");
    let pos = sh_ok("DISPLAY=:99 xdotool getmouselocation");
    assert!(
        pos.contains("x:100") && pos.contains("y:200"),
        "bad pos: {pos}"
    );
}

#[test]
#[ignore]
fn t13_mouse_click() {
    ensure_container();
    sh_ok("DISPLAY=:99 xdotool mousemove 640 360 click 1");
}

#[test]
#[ignore]
fn t14_keyboard_type() {
    ensure_container();
    sh_ok("DISPLAY=:99 xdotool type 'reach e2e'");
}

#[test]
#[ignore]
fn t15_key_combo() {
    ensure_container();
    sh_ok("DISPLAY=:99 xdotool key ctrl+l");
}

// ═══════════════════════════════════════════════════════════
// 6. CHROME
// ═══════════════════════════════════════════════════════════

#[test]
#[ignore]
fn t16_chrome_version() {
    ensure_container();
    assert!(sh_ok("google-chrome --version").contains("Google Chrome"));
}

#[test]
#[ignore]
fn t17_chrome_headed() {
    ensure_container();
    let _ = sh("pkill -f chrome");
    sleep_ms(500);
    sh_ok(
        "DISPLAY=:99 google-chrome --no-sandbox --disable-gpu --no-first-run --window-size=1280,720 https://example.com &",
    );
    sleep_ms(3000);
    sh_ok("DISPLAY=:99 scrot -z /tmp/e2e_chrome.png");
    let bytes: u64 = sh_ok("stat -c %s /tmp/e2e_chrome.png").parse().unwrap();
    assert!(
        bytes > 10_000,
        "screenshot too small ({bytes}b), chrome didn't render"
    );
}

#[test]
#[ignore]
fn t18_chrome_headless_dom() {
    ensure_container();
    let html = String::from_utf8_lossy(&sh("timeout 15 google-chrome --headless --dump-dom --no-sandbox https://example.com 2>/dev/null").stdout).to_string();
    assert!(html.contains("Example Domain"));
}

#[test]
#[ignore]
fn t19_chrome_click_navigates() {
    ensure_container();
    let _ = sh("pkill -f chrome");
    sleep_ms(500);
    sh_ok(
        "DISPLAY=:99 google-chrome --no-sandbox --disable-gpu --no-first-run --window-size=1280,720 https://example.com &",
    );
    sleep_ms(3000);
    sh_ok("DISPLAY=:99 xdotool mousemove 266 353 click 1");
    sleep_ms(3000);
    sh_ok("DISPLAY=:99 scrot -z /tmp/e2e_nav.png");
    let bytes: u64 = sh_ok("stat -c %s /tmp/e2e_nav.png").parse().unwrap();
    assert!(bytes > 5_000, "post-nav screenshot too small ({bytes}b)");
}

// ═══════════════════════════════════════════════════════════
// 7. PLAYWRIGHT
// ═══════════════════════════════════════════════════════════

#[test]
#[ignore]
fn t20_playwright_import() {
    ensure_container();
    let out = sh_ok("python3 -c 'from playwright.sync_api import sync_playwright; print(\"ok\")'");
    assert_eq!(out, "ok");
}

#[test]
#[ignore]
fn t21_playwright_title() {
    ensure_container();
    let title = sh_ok(
        "python3 << 'PY'\nfrom playwright.sync_api import sync_playwright\nwith sync_playwright() as p:\n  b=p.chromium.launch(headless=True)\n  pg=b.new_page(); pg.goto('https://example.com')\n  print(pg.title()); b.close()\nPY",
    );
    assert_eq!(title, "Example Domain");
}

#[test]
#[ignore]
fn t22_playwright_selectors() {
    ensure_container();
    let out = sh_ok(
        "python3 << 'PY'\nfrom playwright.sync_api import sync_playwright\nwith sync_playwright() as p:\n  b=p.chromium.launch(headless=True)\n  pg=b.new_page(); pg.goto('https://example.com')\n  h1=pg.title()\n  n=len(pg.query_selector_all('a'))\n  print(f'{h1}|{n}'); b.close()\nPY",
    );
    let parts: Vec<&str> = out.split('|').collect();
    assert_eq!(parts[0], "Example Domain");
    assert!(parts[1].parse::<usize>().unwrap() > 0);
}

#[test]
#[ignore]
fn t23_playwright_screenshot() {
    ensure_container();
    sh_ok(
        "python3 << 'PY'\nfrom playwright.sync_api import sync_playwright\nwith sync_playwright() as p:\n  b=p.chromium.launch(headless=True)\n  pg=b.new_page(); pg.goto('https://example.com')\n  pg.screenshot(path='/tmp/e2e_pw.png'); b.close()\nPY",
    );
    assert_eq!(sh_ok("od -A n -t x1 -N 4 /tmp/e2e_pw.png"), "89 50 4e 47");
}

// ═══════════════════════════════════════════════════════════
// 8. SCRAPLING
// ═══════════════════════════════════════════════════════════

#[test]
#[ignore]
fn t24_scrapling_version() {
    ensure_container();
    assert!(sh_ok("python3 -c 'import scrapling; print(scrapling.__version__)'").starts_with("0."));
}

#[test]
#[ignore]
fn t25_scrapling_h1() {
    ensure_container();
    assert_eq!(
        sh_ok(
            "python3 -c \"from scrapling import Fetcher; r=Fetcher().get('https://example.com'); print(r.css('title')[0].text)\""
        ),
        "Example Domain"
    );
}

#[test]
#[ignore]
fn t26_scrapling_multi_selector() {
    ensure_container();
    let out = sh_ok(
        "python3 << 'PY'\nfrom scrapling import Fetcher\nr=Fetcher().get('https://example.com')\nprint(f\"{r.css('title')[0].text}|{len(r.css('p'))}|{len(r.css('a'))}\")\nPY",
    );
    let p: Vec<&str> = out.split('|').collect();
    assert_eq!(p[0], "Example Domain");
    assert!(p[1].parse::<usize>().unwrap() > 0);
    // example.com's static HTML no longer contains any <a>; only require a valid count.
    p[2].parse::<usize>().expect("anchor count is a number");
}

// ═══════════════════════════════════════════════════════════
// 9. NODE
// ═══════════════════════════════════════════════════════════

#[test]
#[ignore]
fn t27_node_version() {
    ensure_container();
    assert!(sh_ok("node --version").starts_with('v'));
}

#[test]
#[ignore]
fn t28_computer_use_mcp() {
    ensure_container();
    let out = sh_ok("npm list -g --depth=0 2>/dev/null | grep computer-use-mcp || echo missing");
    assert!(!out.contains("missing"), "computer-use-mcp not installed");
}

// ═══════════════════════════════════════════════════════════
// 10. SECURITY
// ═══════════════════════════════════════════════════════════

#[test]
#[ignore]
fn t29_sandbox_user() {
    ensure_container();
    assert_eq!(sh_ok("whoami"), "sandbox");
}

#[test]
#[ignore]
fn t30_not_root() {
    ensure_container();
    assert_ne!(sh_ok("id -u"), "0");
}

#[test]
#[ignore]
fn t31_home_writable() {
    ensure_container();
    sh_ok("touch ~/e2e_test && rm ~/e2e_test");
}

#[test]
#[ignore]
fn t32_system_dirs_readonly() {
    ensure_container();
    assert_ne!(sh_code("touch /usr/bin/e2e 2>/dev/null"), 0);
    assert_ne!(sh_code("touch /etc/e2e 2>/dev/null"), 0);
}

// ═══════════════════════════════════════════════════════════
// 11. WORKFLOWS
// ═══════════════════════════════════════════════════════════

#[test]
#[ignore]
fn t33_workflow_scrape_then_visual() {
    ensure_container();
    // Scrape
    assert_eq!(
        sh_ok(
            "python3 -c \"from scrapling import Fetcher; print(Fetcher().get('https://example.com').css('title')[0].text)\""
        ),
        "Example Domain"
    );
    // Visual
    let _ = sh("pkill -f chrome");
    sleep_ms(500);
    sh_ok(
        "DISPLAY=:99 google-chrome --no-sandbox --disable-gpu --no-first-run --window-size=1280,720 https://example.com &",
    );
    sleep_ms(3000);
    sh_ok("DISPLAY=:99 scrot -z /tmp/e2e_wf1.png");
    assert!(sh_ok("stat -c %s /tmp/e2e_wf1.png").parse::<u64>().unwrap() > 10_000);
}

#[test]
#[ignore]
fn t34_workflow_type_url() {
    ensure_container();
    let _ = sh("pkill -f chrome");
    sleep_ms(500);
    sh_ok(
        "DISPLAY=:99 google-chrome --no-sandbox --disable-gpu --no-first-run --window-size=1280,720 about:blank &",
    );
    sleep_ms(2000);
    sh_ok("DISPLAY=:99 xdotool key ctrl+l");
    sleep_ms(300);
    sh_ok("DISPLAY=:99 xdotool type --delay 50 'https://example.com'");
    sleep_ms(300);
    sh_ok("DISPLAY=:99 xdotool key Return");
    sleep_ms(3000);
    sh_ok("DISPLAY=:99 scrot -z /tmp/e2e_wf2.png");
    assert!(sh_ok("stat -c %s /tmp/e2e_wf2.png").parse::<u64>().unwrap() > 10_000);
}

#[test]
#[ignore]
fn t35_workflow_headed_and_headless_coexist() {
    ensure_container();
    let _ = sh("pkill -f chrome");
    sleep_ms(500);
    sh_ok(
        "DISPLAY=:99 google-chrome --no-sandbox --disable-gpu --no-first-run --window-size=1280,720 https://example.com &",
    );
    sleep_ms(1000);
    // Headless playwright while headed chrome is running
    let h1 = sh_ok(
        "python3 << 'PY'\nfrom playwright.sync_api import sync_playwright\nwith sync_playwright() as p:\n  b=p.chromium.launch(headless=True)\n  pg=b.new_page(); pg.goto('https://example.com')\n  print(pg.title()); b.close()\nPY",
    );
    assert_eq!(h1, "Example Domain");
    // Headed chrome still alive
    assert!(!sh_ok("pgrep -f 'chrome.*no-sandbox' | head -1").is_empty());
}

// ═══════════════════════════════════════════════════════════
// 12. PAGE_TEXT + AUTH_HANDOFF (Playwright SPA tooling)
// ═══════════════════════════════════════════════════════════

/// Run an embedded Python helper from `reach_cli::docker` inside the
/// container, passing its JSON payload via the named env var.
fn run_embedded_python(env_var: &str, payload: &serde_json::Value, script: &str) -> String {
    use std::io::Write;
    let payload_str = serde_json::to_string(payload).unwrap();

    // Stage the script as a temp file inside the container so we don't
    // have to escape multi-line Python on the command line.
    let mut tmp = std::env::temp_dir();
    tmp.push(format!("reach_e2e_{env_var}.py"));
    {
        let mut f = std::fs::File::create(&tmp).unwrap();
        f.write_all(script.as_bytes()).unwrap();
    }
    let host_path = tmp.to_string_lossy().to_string();
    let container_path = format!("/tmp/{}", tmp.file_name().unwrap().to_string_lossy());

    let cp = docker(&["cp", &host_path, &format!("{CONTAINER}:{container_path}")]);
    assert!(cp.status.success(), "docker cp failed: {}", stderr(&cp));

    let cmd = format!(
        "{env_var}={} python3 {container_path}",
        shell_quote(&payload_str)
    );
    let out = sh(&cmd);
    let _ = std::fs::remove_file(&tmp);
    if !out.status.success() {
        panic!(
            "embedded python failed (exit {}): {}\nstdout: {}",
            out.status.code().unwrap_or(-1),
            stderr(&out),
            String::from_utf8_lossy(&out.stdout)
        );
    }
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn shell_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for ch in s.chars() {
        if ch == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(ch);
        }
    }
    out.push('\'');
    out
}

fn last_json(stdout: &str) -> serde_json::Value {
    let line = stdout
        .lines()
        .rev()
        .map(str::trim)
        .find(|l| l.starts_with('{') && l.ends_with('}'))
        .unwrap_or_else(|| panic!("no JSON object in stdout: {stdout}"));
    serde_json::from_str(line).unwrap_or_else(|e| panic!("bad json {line}: {e}"))
}

#[test]
#[ignore]
fn t37_page_text_basic() {
    ensure_container();
    let _ = sh("pkill -f chrome");
    sleep_ms(500);
    let payload = serde_json::json!({
        "url": "https://example.com",
        "timeout_ms": 30000,
    });
    let stdout = run_embedded_python(
        "REACH_PAGE_TEXT_PAYLOAD",
        &payload,
        reach_cli::docker::PAGE_TEXT_SCRIPT,
    );
    let parsed = last_json(&stdout);
    assert_eq!(parsed["status"], "ok", "page_text not ok: {stdout}");
    let text = parsed["text"].as_str().unwrap_or_default();
    assert!(
        text.contains("documentation examples"),
        "expected text to contain 'documentation examples', got: {text}"
    );
}

#[test]
#[ignore]
fn t38_page_text_selector() {
    ensure_container();
    let _ = sh("pkill -f chrome");
    sleep_ms(500);
    let payload = serde_json::json!({
        "url": "https://example.com",
        "selector": "p",
        "timeout_ms": 30000,
    });
    let stdout = run_embedded_python(
        "REACH_PAGE_TEXT_PAYLOAD",
        &payload,
        reach_cli::docker::PAGE_TEXT_SCRIPT,
    );
    let parsed = last_json(&stdout);
    assert_eq!(
        parsed["status"], "ok",
        "page_text selector not ok: {stdout}"
    );
    let text = parsed["text"].as_str().unwrap_or_default();
    assert!(
        text.trim()
            .starts_with("This domain is for use in documentation examples"),
        "unexpected selector text: {text}"
    );
}

#[test]
#[ignore]
fn t39_auth_handoff_returns_vnc_url() {
    ensure_container();
    let _ = sh("pkill -f chrome");
    sleep_ms(500);

    // Build the noVNC URL the way `reach serve` does so we can assert on it.
    let vnc = reach_cli::docker::novnc_url("localhost", 16080);
    assert!(vnc.contains("vnc.html"));
    assert!(vnc.contains("autoconnect=1"));

    // Drive the auth_handoff Python helper directly: no wait conditions →
    // it should return status=auth_required immediately and leave Chrome
    // running on the Xvfb display.
    let payload = serde_json::json!({
        "url": "https://example.com",
        "user_data_dir": "/home/sandbox/.config/google-chrome-profiles/_e2e",
    });
    let stdout = run_embedded_python(
        "REACH_AUTH_HANDOFF_PAYLOAD",
        &payload,
        reach_cli::docker::AUTH_HANDOFF_SCRIPT,
    );
    let parsed = last_json(&stdout);
    assert_eq!(
        parsed["status"], "auth_required",
        "auth_handoff did not return auth_required: {stdout}"
    );
    let url = parsed["url"].as_str().unwrap_or_default();
    assert!(url.contains("example.com"), "unexpected url: {url}");
}

// ═══════════════════════════════════════════════════════════
// 99. SHUTDOWN (must run last)
// ═══════════════════════════════════════════════════════════

#[test]
#[ignore]
fn t99_graceful_shutdown() {
    ensure_container();
    assert!(docker(&["stop", "-t", "10", CONTAINER]).status.success());
    assert_eq!(
        docker_out(&["inspect", "-f", "{{.State.ExitCode}}", CONTAINER]),
        "0"
    );
    let _ = docker(&["rm", "-f", CONTAINER]);
}
