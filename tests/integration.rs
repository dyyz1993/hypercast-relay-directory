//! 跨组件集成：真 axum serve（随机端口）+ ureq 客户端模拟节点上报与查询。
//! ureq 与 hypercast-relay 的 telemetry.rs 使用同一 HTTP 客户端栈——
//! 此处锁定的是两个开源组件之间的真实线协议契约。

use std::sync::Arc;

use hypercast_relay_directory::api;
use hypercast_relay_directory::state::Directory;

async fn spawn_directory() -> (Arc<Directory>, String) {
    let dir = Arc::new(Directory::in_memory());
    let app = api::router(dir.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (dir, format!("http://{addr}"))
}

fn get_json(url: &str) -> serde_json::Value {
    let body = ureq::get(url)
        .timeout(std::time::Duration::from_secs(5))
        .call()
        .unwrap()
        .into_string()
        .unwrap();
    serde_json::from_str(&body).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn ureq_report_flow_matches_relay_telemetry_contract() {
    let (_dir, base) = spawn_directory().await;

    // 与 relay telemetry.rs 完全一致的调用形态（含 Content-Type）
    let body = serde_json::json!({
        "schema": "hc-telemetry/1",
        "node_id": "oss-node-x",
        "signal_url": "http://127.0.0.1:19999",
        "protocol_version": "1.1",
        "uptime_s": 61,
        "metrics": {"mailbox_registrations_total": 2, "session_requests_total": 1}
    })
    .to_string();
    let resp = ureq::post(&format!("{base}/api/v1/report"))
        .timeout(std::time::Duration::from_secs(5))
        .set("Content-Type", "application/json")
        .send_string(&body)
        .expect("report accepted");
    assert_eq!(resp.status(), 204);

    // 查询：nodes 与单节点端点一致
    let nodes = get_json(&format!("{base}/api/v1/nodes"));
    assert_eq!(nodes["nodes"].as_array().unwrap().len(), 1);
    assert_eq!(nodes["nodes"][0]["node_id"], "oss-node-x");
    assert_eq!(nodes["nodes"][0]["reported"]["uptime_s"], 61);

    let one = get_json(&format!("{base}/api/v1/nodes/oss-node-x"));
    assert_eq!(one["signal_url"], "http://127.0.0.1:19999");
}

#[tokio::test(flavor = "multi_thread")]
async fn register_endpoint_upserts_metadata() {
    let (_dir, base) = spawn_directory().await;

    let resp = ureq::post(&format!("{base}/api/v1/register"))
        .set("Content-Type", "application/json")
        .send_string(
            r#"{"node_id":"n1","name":"公益节点","region":"cn-bj","signal_url":"http://x","protocol_version":"1.1"}"#,
        )
        .unwrap();
    assert_eq!(resp.status(), 201);

    // 二次注册：region 覆盖，未提供的 name 保持不变
    ureq::post(&format!("{base}/api/v1/register"))
        .set("Content-Type", "application/json")
        .send_string(r#"{"node_id":"n1","region":"cn-sh"}"#)
        .unwrap();
    let one = get_json(&format!("{base}/api/v1/nodes/n1"));
    assert_eq!(one["region"], "cn-sh");
    assert_eq!(one["name"], "公益节点");
}
