[English](providers.md)

# 供应商

**供应商** 是供应商控制面。旧书签 `?view=pricing` 会打开这个视图。

公开基座是静态的 Provider Registry，再配按能力拆分的适配器。Custom API 是其中一个 Configurable HTTP 适配器，不是基类。合约范围是：

- 内置家族使用 `Provider(provider_id)`。SCNet 的三个 Token Plan offering 共用一个 SCNet 范围。
- 每个 Custom 目的地使用 `CustomEndpoint(account_id)`。Custom 端点彼此隔离，也与内置家族隔离。

左侧列出这些范围。主区依次是 **概览**、**模型目录**、**上游协议策略**、 **模型合约**、**协议探测**，以及范围内的 **价格**。

**概览** 展示所选供应商、范围修订、生产推理状态、目录可路由状态、禁用原因，以及每个套餐下绑定的账号（启用/禁用与验证状态）。Command Code GOAT 与 SCNet 仍是不可路由草稿：探测不能把它们提升为生产路由。

**模型目录** 是本地的。来源标签为静态目录、官方 Zen 目录、自定义发现或账号声明。有来源地址时会展示，并显示上次成功刷新时间（或尚未刷新）。刷新绝不会自动发生：

- OpenCode Go 使用静态协议目录，不刷新。
- Zen Free 的 **刷新模型目录**（选择 Zen Free 账号）会请求官方无鉴权目录 `https://opencode.ai/zen/v1/models`。失败或结果为空时保留旧快照。
- Custom 的 **刷新模型目录**（选择该 Custom 账号）按已配置的 base URL 发现模型，不改声明能力。发现结果截断会提示；失败时保留旧快照。账号表单里的 **获取模型** 仍是另一项显式编辑，只把 ID 合并进尚未保存的能力列表。
- Command Code GOAT 与 SCNet 不刷新；它们的目录只作适配器输入，不会公布为客户端别名。

刷新目录不会自动公布新的稳定别名。Zen Free 仍会为每个已保存的 `-free` ID 额外生成去掉后缀的 Alias，见 [Zen Free 模型](routing.zh-CN.md#zen-free-模型)。探测确认的额外模型留在合约里，直到它们也匹配已公布别名或合格 Custom 声明 ID。

**上游协议策略** 有三个开关：Chat Completions、Responses、Messages。关闭或开启任一协议会立即作用于该范围下的全部账号，并影响生产路由。开关优先于探测证据和静态支持。禁用账号不会删除已保存的合约；重新启用会恢复已保存的目录、证据和开关。

**模型合约** 列出每个本地模型的首选协议，以及各协议状态：全局关闭（开关关闭）、不可用、不支持、静态、预设、探测已确认，或最近探测失败（带脱敏错误和上次探测时间）。探测成功只能在适配器结构上限内确认或新增支持；探测失败会被记录，但不会删除静态能力。

**协议探测** 是显式动作。它使用所选测试账号，向供应商发送最小真实请求，可能消耗额度——发送前必须确认该警告。客户端请求不会探测。GOAT 与 SCNet 显示该方案暂不支持协议探测。

**价格** 按所选供应商限定范围。刷新仍只由用户手动发起：

- OpenCode Go 展示 revision、文档更新时间、窗口额度、token 单价、`Usage` 和额度扣减倍率，点击刷新后才会访问 `https://opencode.ai/docs/go/`。抓取或校验失败时继续使用最后一次成功快照。allowance 不是额度池、不会参与路由，只用于推导扣减倍率（“月额度 / Usage”）。临时覆盖会创建新的持久化 revision，供后续估算使用。
- Command Code GOAT 与 SCNet Token Plans 显示截至 `2026-08-22` 核对的官方套餐参考，但仍没有已上线的计价或用量路径，也不会因此变为可路由。
- Zen Free 无价格（额度按出口 IP 共享）。
- Custom API 为 unpriced：成功转发记 `cost_state=unknown`，不扣额度，也没有官方用量刷新。

不存在按模型划分的额度池。

请求路径不会发现或探测。流程是：别名 → 账号资格 → 适配器上限 → 已保存合约 → 协议开关 → 透传或转换。带鉴权的 `GET /v1/models` 与受保护的 `GET /dashboard/api/v3/application-models` 只公布当前可路由且有有效启用协议的模型。应用选择器仍是 Go 别名 ∩ 当前价格快照，不含 Custom。

---

[用户指南索引](../USER.zh-CN.md) · [English](providers.md) · [文档索引](../README.zh-CN.md)
