//! Hypercast 社区中继目录服务（MVP：收录 / 遥测接收 / 主动探测 / 展示网页）。

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use hypercast_relay_directory::{api, prober, state::Directory};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();

    let bind = std::env::var("HC_DIRECTORY_BIND").unwrap_or_else(|_| "0.0.0.0:8080".to_owned());
    let data_file = std::env::var_os("HC_DIRECTORY_DATA_FILE").map(std::path::PathBuf::from);
    let probe_enabled = std::env::var("HC_DIRECTORY_PROBE_ENABLED")
        .map(|v| v != "0" && v.to_lowercase() != "false")
        .unwrap_or(true);
    let probe_interval: u64 = std::env::var("HC_DIRECTORY_PROBE_INTERVAL_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60);

    let directory =
        Arc::new(Directory::load(data_file.as_deref()).context("load directory state")?);
    if probe_enabled {
        prober::spawn(
            directory.clone(),
            Duration::from_secs(probe_interval.max(10)),
        );
        tracing::info!(interval_s = probe_interval, "prober enabled");
    }

    let app = api::router(directory.clone());
    let listener = tokio::net::TcpListener::bind(&bind)
        .await
        .context("bind directory")?;
    tracing::info!(%bind, "Hypercast relay directory listening");
    axum::serve(listener, app).await.context("serve directory")
}
