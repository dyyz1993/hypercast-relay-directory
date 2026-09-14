# config-code v1（服务配置码）

客户端扫码导入一个中继服务时，二维码承载的内容格式。

## v1（简化版，MVP）

```
hcrelay1://<signal_url>#<node_id>
```

示例：`hcrelay1://https://relay.example.com:8443#my-node`

| 段 | 说明 |
|---|---|
| `hcrelay1://` | scheme：`hcrelay` + 版本数字；版本不匹配时客户端提示不兼容（§18 门禁精神） |
| `<signal_url>` | 节点信令地址（rendezvous base URL） |
| `#<node_id>` | 节点标识（用于展示与溯源，非秘密） |

## v2（规划中，未实施）

完整形态将增加：TURN 地址列表、目录签名（防部署页被篡改投毒）、节点公钥指纹。
**v1 不含签名属已知妥协**：导入前用户应核对 signal_url 与目录网页展示一致。

## 消费方

- 目录网页「配置码」按钮渲染 QR（本仓库 `web/index.html`）。
- 客户端（闭源）扫码导入 → 按 §17 存为独立服务配置，与官方服务隔离。
