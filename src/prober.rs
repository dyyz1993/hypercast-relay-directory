//! 官方主动探测器：周期 GET 各节点 `signal_url/healthz`，测延迟与存活。
//! 铁律「自报 ≠ 真相」的执行者——探测值与自报值在网页分列展示。

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::state::Directory;

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
                        .map(|u| (n.node_id.clone(), format!("{u}/healthz")))
                })
                .collect();
            for (node_id, url) in targets {
                // ureq 是阻塞客户端，放 blocking 池避免卡 runtime
                let probe = tokio::task::spawn_blocking(move || {
                    let t0 = Instant::now();
                    match ureq::get(&url).timeout(Duration::from_secs(10)).call() {
                        Ok(resp) => Ok((t0.elapsed().as_millis() as u64, resp.status())),
                        Err(e) => Err(e),
                    }
                })
                .await;
                match probe {
                    Ok(Ok((ms, code))) => {
                        let _ = directory.set_probe(&node_id, true, Some(ms), Some(code));
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
