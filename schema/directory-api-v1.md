# directory-api v1（目录对外 API）

## 端点

### `POST /api/v1/register` — 注册/更新节点元数据

```json
{
  "node_id": "my-node",
  "name": "东京公益节点",
  "region": "jp-tokyo",
  "signal_url": "https://relay.example.com:8443",
  "protocol_version": "1.1",
  "owner_contact": "https://t.me/example"
}
```

响应 `201 {"node_id":"my-node","ok":true}`。`None` 字段不覆盖已有值。
**TODO（生产化）**：MVP 开放注册；正式版加 deployer token 与收录审核门槛（规划 §8 待拍板）。

### `POST /api/v1/report` — 遥测上报

见 `telemetry-report-v1.md`（自动注册，无需先 register）。

### `GET /api/v1/nodes` — 节点列表

```json
{
  "nodes": [
    {
      "node_id": "my-node",
      "name": "东京公益节点",
      "region": "jp-tokyo",
      "signal_url": "https://relay.example.com:8443",
      "protocol_version": "1.1",
      "owner_contact": null,
      "first_seen": 1760000000,
      "last_seen": 1760003600,
      "reported": { "uptime_s": 86400, "metrics": {"session_requests_total": 34}, "at": 1760003600 },
      "probed": { "status": "up", "latency_ms": 42, "status_code": 200, "checked_at": 1760003540 },
      "probe_failures_streak": 0
    }
  ]
}
```

- `reported` = 节点自报；`probed` = 目录主动探测。**两者分列展示，不合并**（铁律：自报 ≠ 真相）。
- `probed.status`：`up`（探测成功或连败 <3）/ `down`（连续失败 ≥3）。

### `GET /api/v1/nodes/{node_id}` — 单节点
### `GET /healthz` — 存活检查（也是探测器对节点的探测端点约定）

## 运行配置（目录自身）

| 变量 | 默认 | 说明 |
|---|---|---|
| `HC_DIRECTORY_BIND` | `0.0.0.0:8080` | 监听地址 |
| `HC_DIRECTORY_DATA_FILE` | 无（纯内存） | 状态落盘路径（JSON，原子写） |
| `HC_DIRECTORY_PROBE_ENABLED` | `true` | 关闭探测 |
| `HC_DIRECTORY_PROBE_INTERVAL_SECS` | `60` | 探测周期（下限 10） |

## v1.1（规划中，向后兼容）

`register` / `report` 增加 `capabilities` 字段：

```json
"capabilities": { "turn": true }
```

- `turn=false` = 仅信令节点（如 CF Workers 部署）：打洞失败无中继兜底，目录网页必须标注「无中继兜底」，禁止被误读为完整中继。
- 未携带该字段的历史节点按 `turn=true` 处理（向后兼容）。
