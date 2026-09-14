//! 探测器 E2E：真实 prober tokio 任务 → mock 节点 TCP 服务。
//! 锁定：可达节点记 up+latency；连败 ≥3 记 down。

use std::io::{Read, Write};
use std::sync::Arc;
use std::time::{Duration, Instant};

use hypercast_relay_directory::prober;
use hypercast_relay_directory::state::{Directory, NodeMeta};

/// 返回 200 的健康 mock 节点，并支持 /speedtest（回 64 KiB 响应体）。
fn spawn_healthy_node() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for sock in listener.incoming().flatten() {
            let mut sock = sock;
            let mut buf = [0u8; 2048];
            let n = sock.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            if req.starts_with("GET /speedtest") {
                let payload = vec![0x5Au8; 64 * 1024];
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\n\r\n",
                    payload.len()
                );
                let _ = sock.write_all(head.as_bytes());
                let _ = sock.write_all(&payload);
            } else {
                let _ = sock.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok");
            }
        }
    });
    format!("http://127.0.0.1:{port}")
}

/// accept 后立即断开的故障 mock 节点。
fn spawn_broken_node() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for sock in listener.incoming().flatten() {
            drop(sock); // 立刻断开 → 探测失败
        }
    });
    format!("http://127.0.0.1:{port}")
}

fn meta(signal: &str) -> NodeMeta {
    NodeMeta {
        name: None,
        region: None,
        signal_url: Some(signal.to_owned()),
        protocol_version: None,
        owner_contact: None,
    }
}

async fn wait_for(dir: &Directory, node_id: &str, want_status: &str) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if let Some(rec) = dir.get(node_id) {
            if let Some(p) = &rec.probed {
                if p.status == want_status {
                    return;
                }
            }
        }
        assert!(
            Instant::now() < deadline,
            "timeout waiting {node_id} -> {want_status}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn prober_marks_healthy_node_up_with_latency() {
    let dir = Arc::new(Directory::in_memory());
    dir.upsert("healthy", meta(&spawn_healthy_node())).unwrap();
    prober::spawn(dir.clone(), Duration::from_millis(100));

    wait_for(&dir, "healthy", "up").await;
    let p = dir.get("healthy").unwrap().probed.unwrap();
    assert_eq!(p.status_code, Some(200));
    assert!(p.latency_ms.is_some(), "latency recorded");

    // 带宽实测在同一轮探测成功后执行
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if let Some(rec) = dir.get("healthy") {
            if rec.bandwidth_recent_kbps.is_some() {
                assert!(rec.bandwidth_recent_kbps.unwrap() > 0);
                assert!(rec.availability_pct().unwrap_or(0) > 0);
                break;
            }
        }
        assert!(
            Instant::now() < deadline,
            "timeout waiting bandwidth measurement"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn prober_marks_node_down_after_three_consecutive_failures() {
    let dir = Arc::new(Directory::in_memory());
    dir.upsert("broken", meta(&spawn_broken_node())).unwrap();
    prober::spawn(dir.clone(), Duration::from_millis(100));

    // 连败 1-2 次仍 up（防抖），第 3 次转 down
    wait_for(&dir, "broken", "down").await;
    let rec = dir.get("broken").unwrap();
    assert!(rec.probe_failures_streak >= 3);
    assert_eq!(rec.probed.as_ref().unwrap().latency_ms, None);
}
