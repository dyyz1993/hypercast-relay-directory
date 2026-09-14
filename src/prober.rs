//! 官方主动探测器：周期 GET 各节点 `signal_url/healthz` 测延迟与存活，
//! 并对在线节点做带宽实测（GET `/speedtest?bytes=` 全量读取计时）。
//! 铁律「自报 ≠ 真相」的执行者——探测值与自报值在网页分列展示。

use std::io::Read;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::state::Directory;

/// 带宽实测传输量（2 MiB：够得出稳定数值，又不至于给节点持续压力）。
const SPEEDTEST_BYTES: usize = 2 * 1024 * 1024;

pub fn spawn(directory: Arc<Directory>, interval: Duration) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(interval).await;
            let targets: Vec<(String, String)> = directory
                .list()
                .into_iter()
                .filter_map(|n| {
                    n.signal_url
                        .as_ref()
                        .map(|u| (n.node_id.clone(), u.trim_end_matches('/').to_owned()))
                })
                .collect();
            for (node_id, base) in targets {
                // 1) 存活探测
                let healthz_url = format!("{base}/healthz");
                let speed_base = base;
                let probe = tokio::task::spawn_blocking(move || {
                    let t0 = Instant::now();
                    match ureq::get(&healthz_url)
                        .timeout(Duration::from_secs(10))
                        .call()
                    {
                        Ok(resp) => Ok((t0.elapsed().as_millis() as u64, resp.status())),
                        Err(e) => Err(e),
                    }
                })
                .await;
                match probe {
                    Ok(Ok((ms, code))) => {
                        let _ = directory.set_probe(&node_id, true, Some(ms), Some(code));
                        // 2) 带宽实测（仅在线节点；旧版节点无此端点则静默跳过）
                        let nid = node_id.clone();
                        let speed = tokio::task::spawn_blocking(move || {
                            measure_bandwidth(&nid, &speed_base)
                        })
                        .await;
                        if let Ok(Some(kbps)) = speed {
                            let _ = directory.set_bandwidth(&node_id, kbps);
                        }
                    }
                    Ok(Err(e)) => {
                        tracing::warn!(node_id, "probe failed: {e}");
                        let _ = directory.set_probe(&node_id, false, None, None);
                    }
                    Err(e) => tracing::warn!(node_id, "probe task failed: {e}"),
                }
            }
        }
    });
}

/// 实测下载带宽：全量读取 /speedtest 响应体并计时。
/// 口径：信令口 TCP/HTTPS 吞吐（目录 → 节点方向），作为节点网络容量近似，
/// 非 TURN UDP 吞吐——网页展示须标注。
fn measure_bandwidth(node_id: &str, base: &str) -> Option<u64> {
    let url = format!("{base}/speedtest?bytes={SPEEDTEST_BYTES}");
    let t0 = Instant::now();
    let resp = match ureq::get(&url).timeout(Duration::from_secs(30)).call() {
        Ok(r) => r,
        Err(_) => return None, // 旧版节点无此端点
    };
    let mut reader = resp.into_reader();
    let mut sink = [0u8; 64 * 1024];
    let mut total: usize = 0;
    loop {
        match reader.read(&mut sink) {
            Ok(0) => break,
            Ok(n) => total += n,
            Err(_) => return None,
        }
    }
    let secs = t0.elapsed().as_secs_f64();
    if total == 0 || secs <= 0.0 {
        return None;
    }
    let kbps = (total as f64 * 8.0 / 1000.0 / secs) as u64;
    tracing::debug!(node_id, kbps, total_bytes = total, "bandwidth measured");
    Some(kbps)
}
