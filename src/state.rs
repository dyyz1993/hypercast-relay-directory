//! 目录数据：内存表 + JSON 文件原子落盘。
//! 指标口径：`probed`/`probe_history`/`bandwidth_*` 均为**目录主动实测**；
//! `reported` 为节点自报——两组数据严格分列，不合并（自报 ≠ 真相）。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// 探测历史保留条数（稳定性 = 最近 N 次成功率）。
pub const PROBE_HISTORY_LEN: usize = 20;
/// 带宽样本保留条数（平均带宽 = 最近 N 次实测均值）。
pub const BANDWIDTH_SAMPLES_LEN: usize = 10;

pub fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TelemetrySnapshot {
    pub uptime_s: u64,
    pub metrics: HashMap<String, u64>,
    pub at: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ProbeSnapshot {
    /// `up` | `down`
    pub status: String,
    pub latency_ms: Option<u64>,
    pub status_code: Option<u16>,
    pub checked_at: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NodeRecord {
    pub node_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signal_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_contact: Option<String>,
    pub first_seen: i64,
    pub last_seen: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reported: Option<TelemetrySnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub probed: Option<ProbeSnapshot>,
    /// 最近 N 次探测是否成功（新→旧）；稳定性据此计算。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub probe_history: Vec<bool>,
    /// 最近一次实测带宽（kbps，目录 → 节点方向）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bandwidth_recent_kbps: Option<u64>,
    /// 最近 N 次实测带宽样本（新→旧），平均带宽据此计算。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bandwidth_samples_kbps: Vec<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bandwidth_at: Option<i64>,
    #[serde(default)]
    pub probe_failures_streak: u32,
}

impl NodeRecord {
    /// 稳定性：最近探测历史成功率（0-100）。无历史返回 None。
    pub fn availability_pct(&self) -> Option<u64> {
        if self.probe_history.is_empty() {
            None
        } else {
            let ok = self.probe_history.iter().filter(|b| **b).count() as u64;
            Some(ok * 100 / self.probe_history.len() as u64)
        }
    }

    /// 平均带宽：最近样本均值（kbps）。无样本返回 None。
    pub fn bandwidth_avg_kbps(&self) -> Option<u64> {
        if self.bandwidth_samples_kbps.is_empty() {
            None
        } else {
            let sum: u64 = self.bandwidth_samples_kbps.iter().sum();
            Some(sum / self.bandwidth_samples_kbps.len() as u64)
        }
    }
}

/// 注册/上报入参的元数据合并（None 不覆盖已有值）。
#[derive(Clone, Debug, Default)]
pub struct NodeMeta {
    pub name: Option<String>,
    pub region: Option<String>,
    pub signal_url: Option<String>,
    pub protocol_version: Option<String>,
    pub owner_contact: Option<String>,
}

pub struct Directory {
    nodes: RwLock<HashMap<String, NodeRecord>>,
    file: Option<PathBuf>,
}

impl Directory {
    /// 空目录（测试用）。
    pub fn in_memory() -> Self {
        Self {
            nodes: RwLock::new(HashMap::new()),
            file: None,
        }
    }

    /// 从文件加载（不存在则新建空表）。
    pub fn load(path: Option<&Path>) -> anyhow::Result<Self> {
        let nodes = match path {
            Some(p) if p.exists() => {
                let raw = std::fs::read_to_string(p)?;
                serde_json::from_str(&raw)?
            }
            _ => HashMap::new(),
        };
        Ok(Self {
            nodes: RwLock::new(nodes),
            file: path.map(Path::to_path_buf),
        })
    }

    fn new_record(node_id: &str, now: i64) -> NodeRecord {
        NodeRecord {
            node_id: node_id.to_owned(),
            name: None,
            region: None,
            signal_url: None,
            protocol_version: None,
            owner_contact: None,
            first_seen: now,
            last_seen: now,
            reported: None,
            probed: None,
            probe_history: Vec::new(),
            bandwidth_recent_kbps: None,
            bandwidth_samples_kbps: Vec::new(),
            bandwidth_at: None,
            probe_failures_streak: 0,
        }
    }

    /// upsert：注册元数据 + 触碰时间。返回节点快照。
    pub fn upsert(&self, node_id: &str, meta: NodeMeta) -> anyhow::Result<NodeRecord> {
        let mut nodes = self.nodes.write().unwrap();
        let now = now_secs();
        let rec = nodes
            .entry(node_id.to_owned())
            .or_insert_with(|| Self::new_record(node_id, now));
        rec.last_seen = now;
        if meta.name.is_some() {
            rec.name = meta.name;
        }
        if meta.region.is_some() {
            rec.region = meta.region;
        }
        if meta.signal_url.is_some() {
            rec.signal_url = meta.signal_url;
        }
        if meta.protocol_version.is_some() {
            rec.protocol_version = meta.protocol_version;
        }
        if meta.owner_contact.is_some() {
            rec.owner_contact = meta.owner_contact;
        }
        let snapshot = rec.clone();
        drop(nodes);
        self.persist()?;
        Ok(snapshot)
    }

    /// 记录一次遥测上报（自动注册，无需先 register）。
    pub fn apply_report(
        &self,
        node_id: &str,
        meta: NodeMeta,
        uptime_s: u64,
        metrics: HashMap<String, u64>,
    ) -> anyhow::Result<NodeRecord> {
        let mut nodes = self.nodes.write().unwrap();
        let now = now_secs();
        let rec = nodes
            .entry(node_id.to_owned())
            .or_insert_with(|| Self::new_record(node_id, now));
        rec.last_seen = now;
        if rec.name.is_none() {
            rec.name = Some(node_id.to_owned());
        }
        if meta.signal_url.is_some() {
            rec.signal_url = meta.signal_url;
        }
        if meta.protocol_version.is_some() {
            rec.protocol_version = meta.protocol_version;
        }
        rec.reported = Some(TelemetrySnapshot {
            uptime_s,
            metrics,
            at: now,
        });
        let snapshot = rec.clone();
        drop(nodes);
        self.persist()?;
        Ok(snapshot)
    }

    /// 记录一次探测结果；连续失败 ≥3 记 down；同时滚动保留探测历史。
    pub fn set_probe(
        &self,
        node_id: &str,
        ok: bool,
        latency_ms: Option<u64>,
        status_code: Option<u16>,
    ) -> anyhow::Result<()> {
        let mut nodes = self.nodes.write().unwrap();
        let Some(rec) = nodes.get_mut(node_id) else {
            return Ok(());
        };
        rec.probe_failures_streak = if ok {
            0
        } else {
            rec.probe_failures_streak.saturating_add(1)
        };
        rec.probed = Some(ProbeSnapshot {
            status: if ok || rec.probe_failures_streak < 3 {
                "up".into()
            } else {
                "down".into()
            },
            latency_ms: if ok { latency_ms } else { None },
            status_code,
            checked_at: now_secs(),
        });
        rec.probe_history.insert(0, ok);
        rec.probe_history.truncate(PROBE_HISTORY_LEN);
        drop(nodes);
        self.persist()?;
        Ok(())
    }

    /// 记录一次带宽实测（kbps）：最近值 + 滚动样本。
    pub fn set_bandwidth(&self, node_id: &str, kbps: u64) -> anyhow::Result<()> {
        let mut nodes = self.nodes.write().unwrap();
        if let Some(rec) = nodes.get_mut(node_id) {
            rec.bandwidth_recent_kbps = Some(kbps);
            rec.bandwidth_samples_kbps.insert(0, kbps);
            rec.bandwidth_samples_kbps.truncate(BANDWIDTH_SAMPLES_LEN);
            rec.bandwidth_at = Some(now_secs());
        }
        drop(nodes);
        self.persist()?;
        Ok(())
    }

    pub fn list(&self) -> Vec<NodeRecord> {
        let mut v: Vec<NodeRecord> = self.nodes.read().unwrap().values().cloned().collect();
        v.sort_by(|a, b| a.node_id.cmp(&b.node_id));
        v
    }

    pub fn get(&self, node_id: &str) -> Option<NodeRecord> {
        self.nodes.read().unwrap().get(node_id).cloned()
    }

    /// 原子落盘：先写 tmp 再 rename。
    fn persist(&self) -> anyhow::Result<()> {
        let Some(path) = &self.file else {
            return Ok(());
        };
        let tmp = path.with_extension("json.tmp");
        let json = serde_json::to_string_pretty(&*self.nodes.read().unwrap())?;
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(signal: Option<&str>) -> NodeMeta {
        NodeMeta {
            name: None,
            region: None,
            signal_url: signal.map(str::to_owned),
            protocol_version: None,
            owner_contact: None,
        }
    }

    #[test]
    fn report_auto_registers_and_merges() {
        let dir = Directory::in_memory();
        dir.apply_report("a", meta(Some("http://x:1")), 60, HashMap::new())
            .unwrap();
        let rec = dir.get("a").unwrap();
        assert_eq!(rec.signal_url.as_deref(), Some("http://x:1"));
        assert_eq!(rec.reported.as_ref().unwrap().uptime_s, 60);

        // signal_url=None 不覆盖已有值
        dir.apply_report("a", meta(None), 120, HashMap::new())
            .unwrap();
        let rec = dir.get("a").unwrap();
        assert_eq!(rec.signal_url.as_deref(), Some("http://x:1"));
        assert_eq!(rec.reported.as_ref().unwrap().uptime_s, 120);
    }

    #[test]
    fn probe_down_requires_three_streak_and_keeps_history() {
        let dir = Directory::in_memory();
        dir.upsert("a", meta(Some("http://x:1"))).unwrap();
        dir.set_probe("a", true, Some(5), Some(200)).unwrap();
        dir.set_probe("a", false, None, None).unwrap();
        dir.set_probe("a", false, None, None).unwrap();
        assert_eq!(dir.get("a").unwrap().probed.unwrap().status, "up"); // 连败 2 仍 up
        dir.set_probe("a", false, None, None).unwrap();
        assert_eq!(dir.get("a").unwrap().probed.unwrap().status, "down"); // 连败 3 down
        let rec = dir.get("a").unwrap();
        assert_eq!(rec.probe_history, vec![false, false, false, true]);
        assert_eq!(rec.availability_pct(), Some(25)); // 1/4

        dir.set_probe("a", true, Some(12), Some(200)).unwrap();
        assert_eq!(dir.get("a").unwrap().probed.unwrap().status, "up");
        assert_eq!(dir.get("a").unwrap().probed.unwrap().latency_ms, Some(12));
        assert_eq!(dir.get("a").unwrap().availability_pct(), Some(40)); // 2/5
    }

    #[test]
    fn bandwidth_rolling_average() {
        let dir = Directory::in_memory();
        dir.upsert("b", meta(Some("http://x:1"))).unwrap();
        assert_eq!(dir.get("b").unwrap().bandwidth_avg_kbps(), None);

        for kbps in [100u64, 200, 300] {
            dir.set_bandwidth("b", kbps).unwrap();
        }
        let rec = dir.get("b").unwrap();
        assert_eq!(rec.bandwidth_recent_kbps, Some(300));
        assert_eq!(rec.bandwidth_avg_kbps(), Some(200));
        assert_eq!(rec.bandwidth_samples_kbps, vec![300, 200, 100]);
    }

    #[test]
    fn history_and_samples_are_capped() {
        let dir = Directory::in_memory();
        dir.upsert("c", meta(Some("http://x:1"))).unwrap();
        for i in 0..(PROBE_HISTORY_LEN as u32 + 10) {
            dir.set_probe("c", i % 2 == 0, None, None).unwrap();
        }
        for i in 0..(BANDWIDTH_SAMPLES_LEN as u32 + 10) {
            dir.set_bandwidth("c", 100 + i as u64).unwrap();
        }
        let rec = dir.get("c").unwrap();
        assert_eq!(rec.probe_history.len(), PROBE_HISTORY_LEN);
        assert_eq!(rec.bandwidth_samples_kbps.len(), BANDWIDTH_SAMPLES_LEN);
    }

    #[test]
    fn persistence_roundtrip() {
        let tmp = std::env::temp_dir().join(format!("hc-dir-test-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&tmp);
        {
            let dir = Directory::load(Some(&tmp)).unwrap();
            dir.apply_report("a", meta(Some("http://x:1")), 5, HashMap::new())
                .unwrap();
            dir.set_probe("a", true, Some(9), Some(200)).unwrap();
            dir.set_bandwidth("a", 4096).unwrap();
        }
        let dir2 = Directory::load(Some(&tmp)).unwrap();
        let rec = dir2.get("a").unwrap();
        assert!(rec.probe_history == vec![true]);
        assert_eq!(rec.bandwidth_recent_kbps, Some(4096));
        let _ = std::fs::remove_file(&tmp);
    }
}
