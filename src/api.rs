//! HTTP API：注册 / 遥测上报 / 查询 / 健康。
//! Directory 为纯内存微秒级操作（std RwLock），handler 内直接同步调用。

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;

use crate::state::{Directory, NodeMeta};

pub fn router(directory: Arc<Directory>) -> Router {
    Router::new()
        .route("/", get(index_page))
        .route(
            "/healthz",
            get(|| async { Json(serde_json::json!({"status": "ok"})) }),
        )
        .route("/api/v1/register", post(register))
        .route("/api/v1/report", post(report))
        .route("/api/v1/nodes", get(list_nodes))
        .route("/api/v1/nodes/:node_id", get(get_node))
        .with_state(directory)
}

async fn index_page() -> (
    [(axum::http::header::HeaderName, &'static str); 1],
    &'static str,
) {
    use axum::http::header;
    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        include_str!("../web/index.html"),
    )
}

#[derive(Deserialize, Debug)]
pub struct RegisterRequest {
    pub node_id: String,
    pub name: Option<String>,
    pub region: Option<String>,
    pub signal_url: Option<String>,
    pub protocol_version: Option<String>,
    pub owner_contact: Option<String>,
}

/// 注册/更新节点元数据（MVP 开放注册；生产版加 deployer token——见 schema TODO）。
async fn register(
    State(dir): State<Arc<Directory>>,
    Json(req): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, String)> {
    let node_id = req.node_id.trim().to_owned();
    if node_id.is_empty() || node_id.len() > 64 {
        return Err((
            StatusCode::BAD_REQUEST,
            "node_id must be 1..=64 chars".into(),
        ));
    }
    let meta = NodeMeta {
        name: req.name,
        region: req.region,
        signal_url: req.signal_url,
        protocol_version: req.protocol_version,
        owner_contact: req.owner_contact,
    };
    let rec = dir
        .upsert(&node_id, meta)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({"node_id": rec.node_id, "ok": true})),
    ))
}

#[derive(Deserialize, Debug)]
pub struct ReportRequest {
    pub node_id: String,
    pub signal_url: Option<String>,
    pub protocol_version: Option<String>,
    pub uptime_s: u64,
    #[serde(default)]
    pub metrics: HashMap<String, u64>,
}

/// 遥测上报（自动注册；只收聚合计数——schema 铁律）。
async fn report(
    State(dir): State<Arc<Directory>>,
    Json(req): Json<ReportRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let node_id = req.node_id.trim().to_owned();
    if node_id.is_empty() || node_id.len() > 64 {
        return Err((
            StatusCode::BAD_REQUEST,
            "node_id must be 1..=64 chars".into(),
        ));
    }
    let meta = NodeMeta {
        name: None,
        region: None,
        signal_url: req.signal_url,
        protocol_version: req.protocol_version,
        owner_contact: None,
    };
    dir.apply_report(&node_id, meta, req.uptime_s, req.metrics)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_nodes(State(dir): State<Arc<Directory>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "nodes": dir.list() }))
}

async fn get_node(
    State(dir): State<Arc<Directory>>,
    Path(node_id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    dir.get(&node_id)
        .map(|n| Json(serde_json::json!(n)))
        .ok_or(StatusCode::NOT_FOUND)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    async fn post_json(router: &Router, uri: &str, body: serde_json::Value) -> StatusCode {
        let resp = router
            .clone()
            .oneshot(
                Request::post(uri)
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        resp.status()
    }

    async fn get_json(router: &Router, uri: &str) -> (StatusCode, serde_json::Value) {
        let resp = router
            .clone()
            .oneshot(Request::get(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    #[tokio::test]
    async fn report_auto_registers_then_listed() {
        let router = router(Arc::new(Directory::in_memory()));
        let st = post_json(
            &router,
            "/api/v1/report",
            serde_json::json!({
                "schema": "hc-telemetry/1",
                "node_id": "node-a",
                "signal_url": "http://10.0.0.1:8443",
                "protocol_version": "1.1",
                "uptime_s": 61,
                "metrics": {"mailbox_registrations_total": 2}
            }),
        )
        .await;
        assert_eq!(st, StatusCode::NO_CONTENT);

        let (st, v) = get_json(&router, "/api/v1/nodes").await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(v["nodes"][0]["node_id"], "node-a");
        assert_eq!(
            v["nodes"][0]["reported"]["metrics"]["mailbox_registrations_total"],
            2
        );

        let (st, v) = get_json(&router, "/api/v1/nodes/node-a").await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(v["signal_url"], "http://10.0.0.1:8443");
    }

    #[tokio::test]
    async fn reject_bad_node_id() {
        let router = router(Arc::new(Directory::in_memory()));
        let st = post_json(
            &router,
            "/api/v1/report",
            serde_json::json!({
                "node_id": "", "uptime_s": 1
            }),
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
    }
}
