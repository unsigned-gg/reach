use clap::Args;
use reach_cli::docker::{
    AuthHandoffOptions, DockerClient, PageTextOptions, ProfileMount, novnc_url,
};
use reach_cli::mcp::{
    JsonRpcRequest, JsonRpcResponse, McpInitializeResult, RequestId, ToolResponse, tool_definitions,
};
use std::io::{BufRead, Write};

#[derive(Args)]
pub struct ConnectArgs {
    /// Sandbox name or container ID
    pub target: String,
}

pub async fn run(args: ConnectArgs) -> anyhow::Result<()> {
    let docker = DockerClient::new()?;
    let _sandbox = docker.find(&args.target).await?;

    tracing::info!(target = args.target, "MCP stdio bridge started");

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let request: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let resp = JsonRpcResponse::error(
                    RequestId::Number(0),
                    -32700,
                    format!("parse error: {e}"),
                );
                writeln!(stdout, "{}", serde_json::to_string(&resp)?)?;
                continue;
            }
        };

        let response = handle_request(&docker, &args.target, &request).await;
        writeln!(stdout, "{}", serde_json::to_string(&response)?)?;
        stdout.flush()?;
    }

    Ok(())
}

async fn handle_request(
    docker: &DockerClient,
    target: &str,
    req: &JsonRpcRequest,
) -> JsonRpcResponse {
    match req.method.as_str() {
        "initialize" => {
            let init = McpInitializeResult::default();
            JsonRpcResponse::success(req.id.clone(), serde_json::to_value(init).unwrap())
        }
        "tools/list" => {
            let tools = tool_definitions();
            JsonRpcResponse::success(req.id.clone(), serde_json::json!({ "tools": tools }))
        }
        "tools/call" => {
            let tool_name = req
                .params
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let arguments = req.params.get("arguments").cloned().unwrap_or_default();
            dispatch_tool(docker, target, req, tool_name, &arguments).await
        }
        "notifications/initialized" | "ping" => {
            JsonRpcResponse::success(req.id.clone(), serde_json::json!({}))
        }
        _ => JsonRpcResponse::error(
            req.id.clone(),
            -32601,
            format!("unknown method: {}", req.method),
        ),
    }
}

async fn dispatch_tool(
    docker: &DockerClient,
    target: &str,
    req: &JsonRpcRequest,
    tool: &str,
    args: &serde_json::Value,
) -> JsonRpcResponse {
    let result = match tool {
        "screenshot" => match docker.screenshot(target).await {
            Ok(bytes) => {
                use base64::Engine;
                let data = base64::engine::general_purpose::STANDARD.encode(&bytes);
                ToolResponse::image(data, "image/png")
            }
            Err(e) => ToolResponse::error(e.to_string()),
        },
        "click" => {
            let x = args.get("x").and_then(|v| v.as_i64()).unwrap_or(0);
            let y = args.get("y").and_then(|v| v.as_i64()).unwrap_or(0);
            let btn = match args.get("button").and_then(|v| v.as_str()) {
                Some("right") => "3",
                Some("middle") => "2",
                _ => "1",
            };
            exec_cmd(
                docker,
                target,
                &format!("xdotool mousemove {x} {y} click {btn}"),
            )
            .await
        }
        "type" => {
            let text = args.get("text").and_then(|v| v.as_str()).unwrap_or("");
            exec_cmd(
                docker,
                target,
                &format!("xdotool type -- '{}'", text.replace('\'', "'\\''")),
            )
            .await
        }
        "key" => {
            let combo = args
                .get("combo")
                .and_then(|v| v.as_str())
                .unwrap_or("Return");
            exec_cmd(docker, target, &format!("xdotool key {combo}")).await
        }
        "browse" => {
            let url = args
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("about:blank");
            exec_cmd(
                docker,
                target,
                &format!(
                    "google-chrome --no-sandbox --disable-gpu '{}' &",
                    url.replace('\'', "%27")
                ),
            )
            .await
        }
        "scrape" => {
            let url = args.get("url").and_then(|v| v.as_str()).unwrap_or("");
            let selector = args
                .get("selector")
                .and_then(|v| v.as_str())
                .unwrap_or("body");
            let stealth = args
                .get("stealth")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let fetcher = if stealth {
                "StealthyFetcher"
            } else {
                "Fetcher"
            };
            let script = format!(
                "from scrapling import {fetcher}; r = {fetcher}().get('{url}'); \
                 elems = r.css('{selector}'); \
                 import json; print(json.dumps([{{'content': e.text, 'tag': e.tag}} for e in elems]))"
            );
            exec_python(docker, target, &script).await
        }
        "playwright_eval" => {
            let script = args.get("script").and_then(|v| v.as_str()).unwrap_or("");
            exec_python(docker, target, script).await
        }
        "exec" => {
            let cmd = args
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("echo");
            exec_cmd(docker, target, cmd).await
        }
        "page_text" => {
            let url = match args.get("url").and_then(|v| v.as_str()) {
                Some(u) if !u.is_empty() => u.to_string(),
                _ => {
                    return JsonRpcResponse::success(
                        req.id.clone(),
                        serde_json::to_value(ToolResponse::error(
                            "page_text: missing required `url`",
                        ))
                        .unwrap(),
                    );
                }
            };
            let opts = PageTextOptions {
                url,
                wait_for: args
                    .get("wait_for")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                selector: args
                    .get("selector")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                timeout_ms: args
                    .get("timeout_ms")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(30_000),
                user_data_dir: args
                    .get("use_profile")
                    .and_then(|v| v.as_str())
                    .map(ProfileMount::container_path_for),
            };
            match docker.page_text(target, &opts).await {
                Ok(out) => match serde_json::to_string_pretty(&out) {
                    Ok(s) => ToolResponse::text(s),
                    Err(e) => ToolResponse::error(e.to_string()),
                },
                Err(e) => ToolResponse::error(e.to_string()),
            }
        }
        "auth_handoff" => {
            let url = match args.get("url").and_then(|v| v.as_str()) {
                Some(u) if !u.is_empty() => u.to_string(),
                _ => {
                    return JsonRpcResponse::success(
                        req.id.clone(),
                        serde_json::to_value(ToolResponse::error(
                            "auth_handoff: missing required `url`",
                        ))
                        .unwrap(),
                    );
                }
            };
            let opts = AuthHandoffOptions {
                url,
                wait_for_selector: args
                    .get("wait_for_selector")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                wait_for_url_contains: args
                    .get("wait_for_url_contains")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                timeout_seconds: args
                    .get("timeout_seconds")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(300),
                user_data_dir: args
                    .get("use_profile")
                    .and_then(|v| v.as_str())
                    .map(ProfileMount::container_path_for),
            };

            let vnc = match docker.find(target).await {
                Ok(sandbox) => sandbox
                    .ports
                    .novnc
                    .map(|p| novnc_url("localhost", p))
                    .unwrap_or_else(|| novnc_url("localhost", 6080)),
                Err(_) => novnc_url("localhost", 6080),
            };

            match docker.auth_handoff(target, &opts).await {
                Ok(out) => {
                    let body = serde_json::json!({
                        "status": out.status,
                        "vnc_url": vnc,
                        "url": out.url,
                        "message": out.message,
                        "instructions": "Open the vnc_url in your browser to log in. Re-call \
                                          `auth_handoff` (with wait_for_*) or `page_text` once done.",
                    });
                    match serde_json::to_string_pretty(&body) {
                        Ok(s) => ToolResponse::text(s),
                        Err(e) => ToolResponse::error(e.to_string()),
                    }
                }
                Err(e) => {
                    let body = serde_json::json!({
                        "status": "error",
                        "vnc_url": vnc,
                        "message": e.to_string(),
                    });
                    ToolResponse::error(
                        serde_json::to_string_pretty(&body).unwrap_or_else(|_| e.to_string()),
                    )
                }
            }
        }
        "browser_cdp" => {
            let method = args.get("method").and_then(|v| v.as_str()).unwrap_or("");
            let params = args.get("params").cloned().unwrap_or(serde_json::json!({}));
            cdp(docker, target, method, params).await
        }
        "browser_js" => {
            let expression = args
                .get("expression")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            cdp(
                docker,
                target,
                "Runtime.evaluate",
                serde_json::json!({
                    "expression": expression,
                    "returnByValue": true
                }),
            )
            .await
        }
        "browser_click" => {
            let x = args.get("x").and_then(|v| v.as_i64()).unwrap_or(0);
            let y = args.get("y").and_then(|v| v.as_i64()).unwrap_or(0);
            cdp(
                docker,
                target,
                "Input.dispatchMouseEvent",
                serde_json::json!({
                    "type": "mousePressed",
                    "x": x,
                    "y": y,
                    "button": "left",
                    "clickCount": 1
                }),
            )
            .await;
            cdp(
                docker,
                target,
                "Input.dispatchMouseEvent",
                serde_json::json!({
                    "type": "mouseReleased",
                    "x": x,
                    "y": y,
                    "button": "left",
                    "clickCount": 1
                }),
            )
            .await
        }
        "browser_type" => {
            let text = args.get("text").and_then(|v| v.as_str()).unwrap_or("");
            for ch in text.chars() {
                cdp(
                    docker,
                    target,
                    "Input.dispatchKeyEvent",
                    serde_json::json!({
                        "type": "char",
                        "text": ch.to_string()
                    }),
                )
                .await;
            }
            ToolResponse::text("ok")
        }
        "browser_key" => {
            let key = args.get("key").and_then(|v| v.as_str()).unwrap_or("");
            cdp(
                docker,
                target,
                "Input.dispatchKeyEvent",
                serde_json::json!({
                    "type": "keyDown",
                    "key": key
                }),
            )
            .await;
            cdp(
                docker,
                target,
                "Input.dispatchKeyEvent",
                serde_json::json!({
                    "type": "keyUp",
                    "key": key
                }),
            )
            .await
        }
        _ => ToolResponse::error(format!("unknown tool: {tool}")),
    };

    JsonRpcResponse::success(req.id.clone(), serde_json::to_value(result).unwrap())
}

async fn exec_cmd(docker: &DockerClient, target: &str, cmd: &str) -> ToolResponse {
    match docker
        .exec(target, &["bash".into(), "-c".into(), cmd.into()])
        .await
    {
        Ok(out) if out.exit_code == 0 => ToolResponse::text(if out.stdout.is_empty() {
            "ok".into()
        } else {
            out.stdout
        }),
        Ok(out) => ToolResponse::error(format!("exit {}: {}", out.exit_code, out.stderr)),
        Err(e) => ToolResponse::error(e.to_string()),
    }
}

async fn exec_python(docker: &DockerClient, target: &str, script: &str) -> ToolResponse {
    match docker
        .exec(target, &["python3".into(), "-c".into(), script.into()])
        .await
    {
        Ok(out) if out.exit_code == 0 => ToolResponse::text(out.stdout),
        Ok(out) => ToolResponse::error(format!("exit {}: {}", out.exit_code, out.stderr)),
        Err(e) => ToolResponse::error(e.to_string()),
    }
}

async fn cdp(
    docker: &DockerClient,
    target: &str,
    method: &str,
    params: serde_json::Value,
) -> ToolResponse {
    let port = match docker.find(target).await {
        Ok(sandbox) => match sandbox.ports.browserd {
            Some(p) => p,
            None => return ToolResponse::error("browserd port not exposed"),
        },
        Err(e) => return ToolResponse::error(e.to_string()),
    };

    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{port}/cdp");
    let payload = serde_json::json!({
        "method": method,
        "params": params
    });

    match client.post(&url).json(&payload).send().await {
        Ok(res) => match res.json::<serde_json::Value>().await {
            Ok(json) => {
                if let Some(err) = json.get("error") {
                    ToolResponse::error(err.to_string())
                } else {
                    ToolResponse::text(
                        serde_json::to_string_pretty(&json).unwrap_or_else(|_| "success".into()),
                    )
                }
            }
            Err(e) => ToolResponse::error(e.to_string()),
        },
        Err(e) => ToolResponse::error(e.to_string()),
    }
}
