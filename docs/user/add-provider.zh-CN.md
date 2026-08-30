[English](add-provider.md)

# 新增供应商

当你希望 OCG Manager 把请求路由到另一个上游服务时，先判断走哪条路径：

| 目标 | 路径 | 是否修改仓库 |
| --- | --- | --- |
| 给自己的节点接入 OpenAI 或 Anthropic 兼容端点 | 新增 **Custom API** 账号 | 否 |
| 让所有 OCG Manager 用户获得一个具名 Provider/Plan | 新增密封的内置供应商 | 是，需要经过审查的代码与测试 |

OCG Manager 不提供运行时 Provider 插件 API。内置 Provider Registry 是静态密封注册表，未知的 `provider_id` + `offering_id` 组合会 fail closed。Custom API 是面向用户的可配置 HTTP 接入路径，不是内置供应商的基类。

## 立即接入兼容上游

1. 打开 **账号**，选择 **新增账号** → **Custom API**。
2. 填写名称、上游 API Key、一个 API 地址，以及一个上游协议：**Chat Completions**、**Responses** 或 **Messages**。
3. 至少添加一个准确的上游模型 ID。若上游实现了下文的可选模型目录接口，可用 **获取模型** 填充当前草稿。
4. 保存账号。合法的新账号默认启用；**测试连接** 会通过这一张账号发送一次可产生费用的真实请求，属于可选诊断。
5. 调用 OCG Manager 上带鉴权的 `GET /v1/models`，确认声明模型已经公布，再发送一次推理请求。

一张 Custom 账号卡的所有模型共用一个上游协议。同协议客户端请求直接透传，其他受支持客户端格式会转换到所选上游协议。声明的模型 ID 同时是客户端可见 ID 与准确上游 ID。

## 上游 HTTP 接口

模型发现、连接验证和正式推理使用相同的常见基址解析规则：

| 配置的 API 地址 | 推理地址 | 可选模型目录地址 |
| --- | --- | --- |
| `https://api.example.com` | 追加 `/v1/chat/completions`、`/v1/responses` 或 `/v1/messages` | `https://api.example.com/v1/models` |
| `https://api.example.com/v1` | 追加 `/chat/completions`、`/responses` 或 `/messages` | `https://api.example.com/v1/models` |
| 完整标准推理地址 | 完全按填写值使用 | 同级 `/models` |
| 非标准完整路径 | 完全按填写值使用 | 不猜测；手动填写模型 ID |

配置地址必须是带主机的 HTTP 或 HTTPS URL。内嵌凭据、query 与 fragment 会被拒绝。受信管理员可以主动选择回环、局域网或公网目的地。OCG 不跟随重定向。

所选协议决定线协议契约：

| 协议 | 标准路径 | 发给上游的鉴权 | 必须实现的行为 |
| --- | --- | --- | --- |
| OpenAI Chat Completions | `/v1/chat/completions` | `Authorization: Bearer <upstream-key>` | 接受 Chat 请求 JSON，返回 Chat JSON 或 Chat SSE |
| OpenAI Responses | `/v1/responses` | `Authorization: Bearer <upstream-key>` | 接受 Responses 请求 JSON，返回 Responses JSON 或 Responses SSE |
| Anthropic Messages | `/v1/messages` | `x-api-key: <upstream-key>`，并带 `anthropic-version: 2023-06-01` | 接受 Messages 请求 JSON，返回 Messages JSON 或 Messages SSE |

OCG 根据协议派生鉴权。它不会同时发送两类鉴权头，不会在 `401` 后换头重试，也不会把面板或客户端 Key 转发给上游。响应必须充分遵守所选协议，能由 OCG 解析并转换；这包括标准错误体，以及流式请求中的 `text/event-stream` 帧。

### 可选模型发现

**获取模型** 会对解析后的模型目录地址发送带鉴权的 `GET`。返回带 `data` 数组的 OpenAI/Anthropic 风格对象：

```json
{
  "data": [
    { "id": "model-a" },
    { "id": "model-b" }
  ],
  "has_more": false
}
```

每个可用条目需要非空字符串 `id`。需要分页时，将 `has_more` 设为 `true`，返回 `last_id`（或确保最后一个可用条目带 ID），并接受下一次请求的 `after_id` query 参数。模型发现只更新未保存表单，不会保存、验证或启用账号。

## 新增内置供应商

只有当供应商需要产品持有的身份、目录、账号生命周期、路由、价格/用量或 Custom API 无法表达的其他语义时，才适合新增内置集成。以当前代码为准，不要从旧需求文档推断实现。

1. 在 `crates/ocg-domain/src/ids.rs` 与 `provider.rs` 定义稳定的 Provider/Offering 身份、Plan 行、凭据/额度语义，并穷尽扩展 `ProviderAdapterKind` 映射。
2. 只把已经验证的协议事实加入 `crates/ocg-domain/src/protocol.rs`。请求路由绝不能通过可计费端点猜测协议。
3. 在 `crates/ocg-gateway/src/alias.rs` 添加由代码持有的客户端 Alias 映射。保留准确上游 ID，拒绝有歧义的 raw ID；发现的新目录行不能擅自创造公开 Alias。
4. 在 `ocg-core` 实现宿主路由 resolver。适配器只返回 `AttemptSpec`；数据库访问、Key 解密、代理选择和出站 HTTP 继续由宿主持有。
5. 补齐账号与 **供应商** 控制面/UI 流程；只在该供应商真实支持时加入目录刷新、启停、验证、错误、冷却、价格和用量。Dashboard 变更统一走带 CAS 的 `/dashboard/api/v3`。
6. 更新成对用户文档与测试。至少运行 `cargo test -p ocg-domain`、`cargo test -p ocg-gateway`、`cargo test -p ocg-core`、相关前端测试与 `pnpm run build:web`。契约变化还要运行 `pnpm run contract:v3:check`。

提交贡献前，请写清上游来源、鉴权方式、目录来源、支持的模型/协议组合、流式行为、错误语义、额度/价格来源，以及不产生费用的验证方案。在完整路由与控制面路径真正存在前，让新家族保持 fail closed。

仓库架构细节继续阅读[扩展 OCG Manager](../maintainer/extending.zh-CN.md)与[运行时不变量](../maintainer/runtime-invariants.zh-CN.md)。

---

[用户指南索引](../USER.zh-CN.md) · [English](add-provider.md) · [新增应用](add-application.zh-CN.md) · [文档索引](../README.zh-CN.md)
