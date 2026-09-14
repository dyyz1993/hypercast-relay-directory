# hypercast-relay-directory

Hypercast 社区中继目录：收录、探测、展示自托管中继节点，供用户发现并扫码使用。可由官方运营，也可社区自托管。

**状态：奠基期（M0）** —— 仓库骨架与规划已就位。详见 [生态仓库规划（在 hypercast-relay 仓库）](../hypercast-relay/docs/2026-09-15-ecosystem-repo-plan.md)。

## 产品铁律

1. **收录 ≠ 背书**：所有条目标注「社区中继 · 第三方运营」，禁止官方认证暗示。
2. **自报 ≠ 真相**：官方主动探测（uptime/RTT/丢包/TURN 分配），自报值与探测值**分列展示**。
3. **遥测只收聚合计数**：schema 层禁止用户粒度字段；节点上报 opt-in 默认关。
4. **防枯竭**：展示实时负载，引导用户分散使用。

## 结构（M2 起填充）

```
schema/   ★ 跨仓库协议真源：directory-api / telemetry-report / config-code
registry/ 收录 / 除名 / 查询 API
prober/   官方主动探测器
web/      目录网页（状态 / 地区 / 版本 / 指标 / 负载 / 二维码）
ingress/  遥测接收端
```
