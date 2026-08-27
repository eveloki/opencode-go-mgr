# 协议探测快照 — 2026-08-27

本目录保存 2026-08-27 内置协议快照所依据的脱敏证据。它是维护证据，不是运行时状态，也不表示正常路由可以用付费请求试探协议。

## 范围与方法

- 当前已持久化目录：OpenCode Go 31 个模型、Zen Free 7 个模型、Command Code GOAT 58 个模型。
- 每个模型分别发送 Chat Completions、Responses、Messages 的最小非流式和流式请求。
- Go 与 GOAT 各使用一个已启用测试账号，Zen Free 匿名；本证据包有意省略账号名称与凭据。
- Go 和 GOAT 的外层并发为 4，Zen Free 为 1。
- 初次全量使用 8-token 输出上限。明确要求至少 16 tokens 的组合，以及两个临时异常组合，随后用 16 tokens 定向补测；`merged-latest.jsonl` 保存替换后的观测。
- 不做自动重试。12 次定向替换请求仍保留在 `all-attempts.jsonl` 中。

## 文件

- `all-attempts.jsonl`：初次全量和定向补测在内的全部 588 次请求。
- `merged-latest.jsonl`：每个 Provider/模型/协议/流式模式的最新观测，共 576 条。
- `classified-pairs.json`：288 个 Provider/模型/协议组合，包含两种流式模式的证据和派生分类。

## 分类规则

- `live_supported`：流式与非流式都返回可用、协议形状正确的 2xx 响应。
- `protocol_confirmed_plan_denied`：GOAT 在两种模式下都识别了预期协议路径和模型，但返回 `MODEL_NOT_IN_PLAN`。这能确认协议形状，但本次实测中通道并不可用，因此不进入静态支持并默认关闭。
- `explicit_unsupported`：上游明确拒绝该协议形状/模型组合，或明确表示路径不存在。
- `model_unavailable`、`rate_limited`、`transient_inconclusive`：只说明当前可用性，不足以证明协议形状不受支持。
- `failed_unclassified`：既没有成功，也没有足够具体的否定证据。恢复静态值时按快照未出现/默认关闭处理，但不抹掉这里记录的不确定性。

归一化快照包含：Go 的 23 个 Chat、7 个 Responses、12 个 Messages；Zen Free 的 2 个 Chat、1 个 Responses；GOAT 的 39 个 Chat。本次实测中 GOAT 没有可用的 Messages 或 Responses 组合；`stealth/ox-alpha` 在 GOAT 三条路径上都被明确拒绝。
