use anyhow::{Context, Result};
use axum::{Json, Router, extract::State, http::StatusCode, routing::post};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::{mpsc, oneshot};
use tokio::time::{Duration, sleep};
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[derive(Clone)]
struct AppState {
    actor_tx: mpsc::Sender<ActorMessage>,
}

#[derive(Deserialize, Debug)]
struct CdpRequest {
    method: String,
    #[serde(default)]
    params: Value,
    /// Optional explicit sessionId. When omitted the daemon routes to the
    /// default healing session (the one attached to the first page target).
    #[serde(default)]
    session_id: Option<String>,
}

#[derive(Deserialize, Debug)]
struct CreateContextRequest {
    #[serde(default)]
    proxy_server: Option<String>,
    #[serde(default)]
    proxy_bypass_list: Option<String>,
    #[serde(default)]
    url: Option<String>,
}

#[derive(Serialize, Debug)]
struct CreateContextResponse {
    browser_context_id: String,
    target_id: String,
    session_id: String,
}

#[derive(Deserialize, Debug)]
struct DisposeContextRequest {
    browser_context_id: String,
}

/// Routing target for a CDP command.
#[derive(Debug, Clone)]
enum SessionTarget {
    /// Send through the healing default page session (back-compat path).
    Default,
    /// Send to a specific session id (returned from `/cdp/context/create`).
    Specific(String),
    /// Send to the browser-level session (no `sessionId` on the wire).
    /// Required for `Target.*` commands that mint or destroy contexts.
    Browser,
}

enum ActorMessage {
    SendCommand {
        method: String,
        params: Value,
        target: SessionTarget,
        resp_tx: oneshot::Sender<Value>,
    },
    HealSession {
        resp_tx: oneshot::Sender<()>,
    },
}

#[derive(Deserialize)]
struct VersionResponse {
    #[serde(rename = "webSocketDebuggerUrl")]
    web_socket_debugger_url: String,
}

async fn get_debugger_url() -> String {
    let client = reqwest::Client::new();
    loop {
        if let Ok(resp) = client
            .get("http://127.0.0.1:9222/json/version")
            .send()
            .await
        {
            if let Ok(version) = resp.json::<VersionResponse>().await {
                return version.web_socket_debugger_url;
            }
        }
        tracing::warn!("Waiting for Chrome CDP to be ready at 127.0.0.1:9222...");
        sleep(Duration::from_secs(2)).await;
    }
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
fn get_next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::SeqCst)
}

struct CdpActor {
    req_rx: mpsc::Receiver<ActorMessage>,
}

impl CdpActor {
    async fn run(mut self) {
        loop {
            tracing::info!("Connecting to CDP...");
            let (ws_stream, default_session) = match Self::connect_and_attach().await {
                Ok(tuple) => tuple,
                Err(e) => {
                    tracing::error!("Failed to connect and attach: {}. Retrying...", e);
                    sleep(Duration::from_secs(2)).await;
                    continue;
                }
            };

            tracing::info!("Connected to CDP, default session: {}", default_session);
            let (mut ws_tx, mut ws_rx) = ws_stream.split();
            let mut pending = HashMap::<u64, oneshot::Sender<Value>>::new();
            let mut owned_contexts: HashSet<String> = HashSet::new();

            'inner: loop {
                tokio::select! {
                    msg = ws_rx.next() => {
                        match msg {
                            Some(Ok(Message::Text(text))) => {
                                if let Ok(json) = serde_json::from_str::<Value>(&text) {
                                    if let Some(id) = json.get("id").and_then(|i| i.as_u64()) {
                                        // Default-session loss is the only thing
                                        // that warrants tearing down the WS.
                                        // Errors scoped to ad-hoc contexts are
                                        // delivered to the caller as-is.
                                        let session_lost_default = json
                                            .get("error")
                                            .and_then(|e| e.get("message"))
                                            .and_then(|m| m.as_str())
                                            .map(|s| s.contains("Session with given id not found"))
                                            .unwrap_or(false)
                                            && json
                                                .get("sessionId")
                                                .and_then(|s| s.as_str())
                                                .map(|s| s == default_session)
                                                .unwrap_or(true);

                                        if let Some(tx) = pending.remove(&id) {
                                            let _ = tx.send(json.clone());
                                        }

                                        if session_lost_default {
                                            tracing::warn!("Default session lost; reconnecting...");
                                            break 'inner;
                                        }
                                    }
                                }
                            }
                            Some(Err(e)) => {
                                tracing::warn!("Websocket error: {}", e);
                                break 'inner;
                            }
                            None => {
                                tracing::warn!("Websocket closed");
                                break 'inner;
                            }
                            _ => {}
                        }
                    }
                    req = self.req_rx.recv() => {
                        match req {
                            Some(ActorMessage::SendCommand { method, params, target, resp_tx }) => {
                                let id = get_next_id();
                                pending.insert(id, resp_tx);

                                let mut payload = json!({
                                    "id": id,
                                    "method": method,
                                    "params": params,
                                });
                                match &target {
                                    SessionTarget::Default => {
                                        payload["sessionId"] = json!(default_session);
                                    }
                                    SessionTarget::Specific(sid) => {
                                        payload["sessionId"] = json!(sid);
                                    }
                                    SessionTarget::Browser => {
                                        // No sessionId — browser routes the
                                        // command to the browser session.
                                    }
                                }

                                if method == "Target.disposeBrowserContext" {
                                    if let Some(id) = params.get("browserContextId").and_then(|v| v.as_str()) {
                                        owned_contexts.remove(id);
                                    }
                                }

                                if ws_tx.send(Message::Text(payload.to_string())).await.is_err() {
                                    tracing::warn!("Failed to send command to WS, breaking...");
                                    break 'inner;
                                }
                            }
                            Some(ActorMessage::HealSession { resp_tx }) => {
                                tracing::info!("Healing requested explicitly. Forcing reconnect...");
                                let _ = resp_tx.send(());
                                break 'inner;
                            }
                            None => {
                                tracing::info!("Actor channel closed, exiting.");
                                return;
                            }
                        }
                    }
                }
            }

            // Best-effort: drop pending (cancels oneshots) and try to dispose
            // any ad-hoc browser contexts that we minted so they don't leak
            // across the reconnect. We can't await on the WS we just dropped,
            // so this happens after the next successful reconnect via a
            // synthetic in-band command if needed. For now, just log.
            if !owned_contexts.is_empty() {
                tracing::warn!(
                    contexts = owned_contexts.len(),
                    "ad-hoc browser contexts may have been orphaned by the reconnect"
                );
            }
            drop(pending);
            drop(owned_contexts);
            tracing::info!("Connection dropped or healing requested. Reconnecting in 2 seconds...");
            sleep(Duration::from_secs(2)).await;
        }
    }

    async fn connect_and_attach() -> Result<(
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        String,
    )> {
        let ws_url = get_debugger_url().await;
        let (mut ws_stream, _) = connect_async(&ws_url)
            .await
            .context("Failed to connect_async")?;

        // 1. Get targets
        let id_targets = get_next_id();
        let msg = json!({
            "id": id_targets,
            "method": "Target.getTargets",
        });
        ws_stream.send(Message::Text(msg.to_string())).await?;

        let mut target_id = String::new();
        while let Some(Ok(Message::Text(txt))) = ws_stream.next().await {
            let v: Value = serde_json::from_str(&txt)?;
            if v.get("id").and_then(|i| i.as_u64()) == Some(id_targets) {
                if let Some(targets) = v.pointer("/result/targetInfos").and_then(|t| t.as_array()) {
                    for t in targets {
                        if let (Some(t_type), Some(t_id)) = (
                            t.get("type").and_then(|s| s.as_str()),
                            t.get("targetId").and_then(|s| s.as_str()),
                        ) {
                            if t_type == "page"
                                && !t
                                    .get("url")
                                    .and_then(|s| s.as_str())
                                    .unwrap_or("")
                                    .starts_with("chrome://")
                            {
                                target_id = t_id.to_string();
                                break;
                            }
                        }
                    }
                }
                break;
            }
        }

        if target_id.is_empty() {
            tracing::info!("No valid page target found. Creating about:blank...");
            let id_create = get_next_id();
            let msg = json!({
                "id": id_create,
                "method": "Target.createTarget",
                "params": {"url": "about:blank"}
            });
            ws_stream.send(Message::Text(msg.to_string())).await?;
            while let Some(Ok(Message::Text(txt))) = ws_stream.next().await {
                let v: Value = serde_json::from_str(&txt)?;
                if v.get("id").and_then(|i| i.as_u64()) == Some(id_create) {
                    target_id = v
                        .pointer("/result/targetId")
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .to_string();
                    break;
                }
            }
        }

        if target_id.is_empty() {
            anyhow::bail!("Failed to find or create a valid target");
        }

        // 2. Attach to target
        let id_attach = get_next_id();
        let msg = json!({
            "id": id_attach,
            "method": "Target.attachToTarget",
            "params": {"targetId": target_id, "flatten": true}
        });
        ws_stream.send(Message::Text(msg.to_string())).await?;

        let mut session_id = String::new();
        while let Some(Ok(Message::Text(txt))) = ws_stream.next().await {
            let v: Value = serde_json::from_str(&txt)?;
            if v.get("id").and_then(|i| i.as_u64()) == Some(id_attach) {
                session_id = v
                    .pointer("/result/sessionId")
                    .and_then(|s| s.as_str())
                    .unwrap_or("")
                    .to_string();
                break;
            }
        }

        if session_id.is_empty() {
            anyhow::bail!("Failed to attach to target");
        }

        Ok((ws_stream, session_id))
    }
}

/// CDP methods that must be sent at the browser session level (no `sessionId`).
/// Anything that mints, lists, or destroys targets/contexts belongs here.
fn is_browser_level_method(method: &str) -> bool {
    matches!(
        method,
        "Target.createBrowserContext"
            | "Target.disposeBrowserContext"
            | "Target.getBrowserContexts"
            | "Target.createTarget"
            | "Target.closeTarget"
            | "Target.getTargets"
            | "Target.attachToTarget"
            | "Target.detachFromTarget"
            | "Browser.getVersion"
            | "Browser.close"
            | "Browser.getWindowForTarget"
    )
}

async fn send_actor_command(
    state: &AppState,
    method: &str,
    params: Value,
    target: SessionTarget,
) -> Result<Value, String> {
    let (resp_tx, resp_rx) = oneshot::channel();
    let req = ActorMessage::SendCommand {
        method: method.to_string(),
        params,
        target,
        resp_tx,
    };
    state
        .actor_tx
        .send(req)
        .await
        .map_err(|_| "actor disconnected".to_string())?;
    let resp = resp_rx
        .await
        .map_err(|_| "request dropped during reconnect".to_string())?;
    if let Some(err) = resp.get("error") {
        return Err(err.to_string());
    }
    Ok(resp.get("result").cloned().unwrap_or(Value::Null))
}

async fn handle_cdp_request(
    State(state): State<AppState>,
    Json(payload): Json<CdpRequest>,
) -> Json<Value> {
    loop {
        let (resp_tx, resp_rx) = oneshot::channel();
        // Browser-level methods (Target.* lifecycle, Browser.*) must skip
        // sessionId so Chrome routes them to the browser. Page-level methods
        // honor an explicit session_id or fall back to the default.
        let target = if payload.session_id.is_some() {
            SessionTarget::Specific(payload.session_id.clone().unwrap())
        } else if is_browser_level_method(&payload.method) {
            SessionTarget::Browser
        } else {
            SessionTarget::Default
        };

        let req = ActorMessage::SendCommand {
            method: payload.method.clone(),
            params: payload.params.clone(),
            target,
            resp_tx,
        };

        if state.actor_tx.send(req).await.is_err() {
            return Json(json!({"error": "Internal error: actor disconnected"}));
        }

        match resp_rx.await {
            Ok(resp) => {
                // Default-session loss explicitly heals; ad-hoc context errors
                // (sessionId provided by caller) are returned as-is so the
                // caller can decide whether to retry.
                if payload.session_id.is_none() {
                    if let Some(error) = resp.get("error") {
                        if let Some(msg) = error.get("message").and_then(|m| m.as_str()) {
                            if msg.contains("Session with given id not found") {
                                tracing::warn!("Default session lost; healing...");
                                let (heal_tx, heal_rx) = oneshot::channel();
                                let _ = state
                                    .actor_tx
                                    .send(ActorMessage::HealSession { resp_tx: heal_tx })
                                    .await;
                                let _ = heal_rx.await;
                                continue;
                            }
                        }
                    }
                }
                return Json(resp);
            }
            Err(_) => {
                if payload.session_id.is_some() {
                    return Json(json!({
                        "error": {"message": "ad-hoc session request dropped during reconnect"}
                    }));
                }
                tracing::warn!("Request dropped due to actor reconnect, retrying...");
                sleep(Duration::from_millis(500)).await;
                continue;
            }
        }
    }
}

async fn handle_create_context(
    State(state): State<AppState>,
    Json(payload): Json<CreateContextRequest>,
) -> Result<Json<CreateContextResponse>, (StatusCode, String)> {
    // 1. Mint browser context with optional proxy.
    let mut params = json!({});
    if let Some(server) = &payload.proxy_server {
        params["proxyServer"] = json!(server);
    }
    if let Some(bypass) = &payload.proxy_bypass_list {
        params["proxyBypassList"] = json!(bypass);
    }
    let result = send_actor_command(
        &state,
        "Target.createBrowserContext",
        params,
        SessionTarget::Browser,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            format!("createBrowserContext: {e}"),
        )
    })?;

    let browser_context_id = result
        .get("browserContextId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            (
                StatusCode::BAD_GATEWAY,
                "createBrowserContext returned no browserContextId".to_string(),
            )
        })?
        .to_string();

    // 2. Create a target inside the new context.
    let create_url = payload.url.unwrap_or_else(|| "about:blank".to_string());
    let target_result = send_actor_command(
        &state,
        "Target.createTarget",
        json!({
            "url": create_url,
            "browserContextId": browser_context_id,
        }),
        SessionTarget::Browser,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            format!("createTarget in new context: {e}"),
        )
    })?;

    let target_id = target_result
        .get("targetId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            (
                StatusCode::BAD_GATEWAY,
                "createTarget returned no targetId".to_string(),
            )
        })?
        .to_string();

    // 3. Attach (flatten) to obtain a sessionId for this target.
    let attach = send_actor_command(
        &state,
        "Target.attachToTarget",
        json!({"targetId": target_id, "flatten": true}),
        SessionTarget::Browser,
    )
    .await
    .map_err(|e| (StatusCode::BAD_GATEWAY, format!("attachToTarget: {e}")))?;

    let session_id = attach
        .get("sessionId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            (
                StatusCode::BAD_GATEWAY,
                "attachToTarget returned no sessionId".to_string(),
            )
        })?
        .to_string();

    Ok(Json(CreateContextResponse {
        browser_context_id,
        target_id,
        session_id,
    }))
}

async fn handle_dispose_context(
    State(state): State<AppState>,
    Json(payload): Json<DisposeContextRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let result = send_actor_command(
        &state,
        "Target.disposeBrowserContext",
        json!({"browserContextId": payload.browser_context_id}),
        SessionTarget::Browser,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            format!("disposeBrowserContext: {e}"),
        )
    })?;
    Ok(Json(result))
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let (actor_tx, actor_rx) = mpsc::channel(100);
    let actor = CdpActor { req_rx: actor_rx };
    tokio::spawn(actor.run());

    let state = AppState { actor_tx };

    let app = Router::new()
        .route("/cdp", post(handle_cdp_request))
        .route("/cdp/context/create", post(handle_create_context))
        .route("/cdp/context/dispose", post(handle_dispose_context))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8401").await?;
    tracing::info!("reach-browserd listening on 0.0.0.0:8401");
    axum::serve(listener, app).await?;

    Ok(())
}
