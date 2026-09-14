# hypercast-relay-directory

Hypercast 社区中继目录：收录、探测、展示自托管中继节点，供用户发现并扫码使用。可由官方运营，也可社区自托管。

**状态：MVP（M2）** —— 收录 / 遥测接收 / 主动探测 / 展示网页已可用。详见 [生态仓库规划（在 hypercast-relay 仓库）](../hypercast-relay/docs/2026-09-15-ecosystem-repo-plan.md)。

## 快速运行

```bash
cargo run --release
# 默认监听 0.0.0.0:8080；打开 http://127.0.0.1:8080
```

Docker：

```bash
docker build -t hypercast-relay-directory -f deploy/Dockerfile .
docker run -d -p 8080:8080 -v $PWD/data:/data \
  -e HC_DIRECTORY_DATA_FILE=/data/nodes.json \
  hypercast-relay-directory
```

## 产品铁律

1. **收录 ≠ 背书**：所有条目标注「社区中继 · 第三方运营」，禁止官方认证暗示。
2. **自报 ≠ 真相**：官方主动探测（GET `signal_url/healthz` 测延迟，连败 ≥3 记 down），自报值与探测值**分列展示**。
3. **遥测只收聚合计数**：schema 层禁止用户粒度字段；节点上报 opt-in 默认关。
4. **防枯竭**：展示实时负载，引导用户分散使用。

## 结构

```
src/       目录服务单体（MVP）
  api.rs     registry + 遥测接收 + 查询 API
  prober.rs  主动探测器
  state.rs   节点表（内存 + JSON 原子落盘）
schema/    ★ 跨仓库协议真源：directory-api / telemetry-report / config-code（v1）
web/       目录网页（深色主题，QR 用本地 qrcode-generator，无外部依赖）
deploy/    Dockerfile
```

规划中的 `registry/ prober/ web/ ingress/` 独立目录在 MVP 阶段收敛为上述单体模块；服务长大后按规划拆分。

## API 与协议

见 `schema/` 三份规范——它们是**跨仓库唯一真源**：hypercast-relay 的遥测上报、闭源客户端的扫码消费都以此对齐（版本纪律同源）。
