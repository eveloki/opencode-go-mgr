[English](providers.md)

# 供应商

**供应商** 是供应商控制面——如果你的旧书签还挂着 `?view=pricing`，进来的就是这个视图。

底层是静态 Provider Registry 加几个按能力拆分的适配器。Custom API 只是其中一个 Configurable HTTP 适配器，不是基类，其他方案不会继承它。范围划分如下：

- 内置家族使用 `Provider(provider_id)`。
- 每个 Custom 目的地使用 `CustomEndpoint(account_id)`。Custom 端点彼此隔离，也与内置家族隔离。

左侧列出这些范围。主区依次是 **概览**、**模型目录**、**上游协议策略**、 **模型合约**、**协议探测**，以及范围内的 **价格**。

**概览** 展示所选供应商、范围修订、生产推理状态、目录可路由状态、禁用原因，以及每个套餐下绑定的账号（启用/禁用与验证状态）。Command Code GOAT 只通过已验证且显式启用的账号路由。

**模型目录** 是本地的。来源标签分静态目录、官方 Zen 目录、自定义发现、账号声明四种；有来源地址和上次成功刷新时间时会展示，否则显示“尚未刷新”。刷新绝不会自动发生：

- OpenCode Go 的 **刷新模型目录** 使用所选 Go 账号 Key 调用官方 `GET /zen/go/v1/models`。成功后保存的 Provider 目录替代静态兜底；失败或结果为空时保留旧快照。
- Zen Free 的 **刷新模型目录**（选择 Zen Free 账号）会请求官方无鉴权目录 `https://opencode.ai/zen/v1/models`。失败或结果为空时保留旧快照。
- Custom 的 **刷新模型目录**（选择该 Custom 账号）按已配置的 base URL 发现模型，不改声明能力。发现结果截断会提示；失败时保留旧快照。账号表单里的 **获取模型** 仍是另一项显式编辑，只把 ID 合并进尚未保存的能力列表。
- Command Code GOAT 的 **刷新模型目录** 使用所选已验证 GOAT 账号调用官方 `GET /provider/v1/models`。成功会同时更新该账号的允许目录和共享 Provider 目录；失败保留最后成功快照。

已保存的 Go/GOAT 目录会进入本地 Alias 解析，不会再触发上游调用。只有已保存合约具有已启用、已知协议的模型才会对客户端公布；陌生的 Go ID 会显示在 Provider 目录中，但在协议未知时仍会封闭路由。Zen Free 仍会为每个已保存的 `-free` ID 额外生成去掉后缀的 Alias，见 [Zen Free 模型](routing.zh-CN.md#zen-free-模型)。

**上游协议策略** 按该范围的“结构协议集”渲染开关：有模型证据的协议，加上当前被禁用的开关。内置供应商固定为 Chat Completions、Responses、Messages 三个开关；Custom 端点的开关恰好等于该账号声明的协议集。每个开关显示其下可用模型的数量。切换开关会立即作用于该范围下的全部账号并影响生产路由；Custom 端点范围的开关也可直接写入。开关优先级高于探测证据与静态支持。禁用账号不会删除已保存合约；重新启用会恢复目录、证据和开关。

**模型合约** 列出每个本地模型及其协议证据。单协议模型直接显示该协议；启用协议 ≥2 的模型才在列表中显示首选协议。各协议状态包括：全局关闭、不可用、不支持、静态、预设、探测已确认，或最近探测失败（附脱敏错误与探测时间）。探测成功只能在适配器结构上限内确认或新增支持；探测失败只被记录，不会删除静态能力。

**协议探测** 是显式动作：选择测试账号，向供应商发送最小真实请求，并承认它可能消耗额度。客户端请求不会探测。GOAT 显示该方案暂不支持协议探测。

**价格** 按所选供应商限定范围。**刷新价格表** 只抓取并校验当前所选 Provider 自己的官方来源。OpenCode 与 Command Code 的 revision 和最后成功快照彼此独立；一个失败不会动另一个。以后某个 Provider 若包含多个有价格的 Plan，一次操作也只刷新该 Provider 内的 Plan。刷新仍只能手动发起：

- OpenCode Go 展示 revision、文档更新时间、窗口额度、token 单价、`Usage` 和额度扣减倍率，点击刷新后才会访问 `https://opencode.ai/docs/go/`。抓取或校验失败时继续使用最后一次成功快照。allowance 不是额度池、不会参与路由，只用于推导扣减倍率（“月额度 / Usage”）。临时覆盖会创建新的持久化 revision，供后续估算使用。
- Command Code GOAT 展示从 `https://commandcode.ai/docs/plans/goat` 保存的官方订阅/费率快照。它只作展示参考，不进入 OpenCode Go 额度扣减，也不会虚构 GOAT 用量 API。
- Zen Free 无价格（额度按出口 IP 共享）。
- Custom API 为 unpriced：成功转发记 `cost_state=unknown`，不扣额度，也没有官方用量刷新。

不存在按模型划分的额度池。

请求路径不会发现或探测。流程是：别名 → 账号资格 → 适配器上限 → 已保存合约 → 协议开关 → 透传或转换。带鉴权的 `GET /v1/models` 与受保护的 `GET /dashboard/api/v3/application-models` 只公布当前可路由且有有效启用协议的模型。应用选择器仍是 Go 别名 ∩ 当前价格快照，不含 Custom。

---

[用户指南索引](../USER.zh-CN.md) · [English](providers.md) · [文档索引](../README.zh-CN.md)
