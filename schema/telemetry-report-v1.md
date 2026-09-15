# telemetry-report v1（节点 → 目录）

节点（hypercast-relay）向目录上报聚合计数的协议。**唯一允许的方向：节点 → 目录；唯一允许的内容：聚合计数。**

## 请求

`POST {DIRECTORY_URL}/api/v1/report`（Content-Type: application/json）

```json
{
  "schema": "hc-telemetry/1",
  "node_id": "my-node",
  "signal_url": "https://relay.example.com:8443",
  "protocol_version": "1.1",
  "uptime_s": 86400,
  "metrics": {
    "mailbox_registrations_total": 12,
    "session_requests_total": 34
  }
}
```

| 字段 | 必填 | 说明 |
|---|---|---|
| `schema` | 否 | 固定 `hc-telemetry/1`（服务端忽略未知字段） |
| `node_id` | 是 | 1..=64 字符；首次上报即自动注册 |
| `signal_url` | 否 | 节点对外信令地址；供目录展示与探测，None 不覆盖已有值 |
| `protocol_version` | 是 | 线协议版本（`crates/protocol-kit version.rs` 镜像语义，§18） |
| `uptime_s` | 是 | 进程存活秒数 |
| `metrics` | 否 | 聚合计数表（键 → 单调递增计数） |

## 铁律（服务端拒绝越界演进）

1. **只收聚合计数**：任何用户粒度字段（IP、会话标识、时间线、指纹）进入 schema 演进提案 = 直接拒绝。
2. metrics 值必须是 `u64` 单调计数或 0/1 布尔语义，不允许任意文本。
3. 上报频次 ≤ 1 次/60s（服务端可限流）。
4. 响应：`204 No Content`（成功）/ `400`（node_id 非法）/ `429`（限流，未来）。

## 实现状态

- hypercast-relay `server/src/telemetry.rs`：每 60s 上报，`HYPERCAST_DIRECTORY_URL` 未设 = 完全关闭（opt-in）。
- 本目录 `src/api.rs::report`：接收端。

## v1.1 变更（已实现）

`report` 新增可选字段 `turn_urls: string[]`——节点自报配置的 TURN 地址（如 `["turn:turn.example.com:3478"]`）。
目录据此对 TURN 端口做 **UDP STUN Binding 实测**，探测通过 = 完整中继节点（网页亮「中继 ✓」）；
未配置或探测失败 = 仅信令节点（标注「无中继兜底」）。未携带该字段按空处理（向后兼容）。
