[English](providers.md)

# 供应商

**供应商** 是供应商控制面——如果你的旧书签还挂着 `?view=pricing`，进来的就是这个视图。

底层是静态 Provider Registry 加几个按能力拆分的适配器。Custom API 只是其中一个 Configurable HTTP 适配器，不是基类，其他方案不会继承它。范围划分如下：

- 内置家族使用 `Provider(provider_id)`。
- 每个 Custom 目的地使用 `CustomEndpoint(account_id)`。Custom 端点彼此隔离，也与内置家族隔离。

左侧列出这些范围。主区有两个子页签：**模型目录** 与 **价格**。原来的目录与模型合约视图合并为模型目录页签上的一张矩阵表。

**模型目录** 是本地的。矩阵以模型为行、三个上游协议（Chat Completions、Responses、Messages）为列。每格展示该模型/协议对的 effective 状态，并带一个三态控件：**自动**（无覆盖行，跟随底层证据）、**强制开启**（在该模型上启用该协议，但不突破适配器安全上限）、**强制关闭**（禁用）。你可以逐格调整，也可以按整行或整列批量设置。

每格的底层证据为：不可用、不支持、静态、预设（Custom 声明协议）、探测已确认，或最近探测失败。矩阵格展示应用覆盖后的 effective 结果：`force_on` 即使证据缺失也会启用，但绝不突破适配器上限；`force_off` 即使证据支持也会关闭；`auto` 跟随证据。探测失败只被记录，不会删除静态能力。

刷新绝不会自动发生：

- OpenCode Go 的 **刷新模型目录** 使用所选 Go 账号 Key 调用官方 `GET /zen/go/v1/models`。成功后保存的 Provider 目录替代静态兜底；失败或结果为空时保留旧快照。
- Zen Free 的 **刷新模型目录**（选择 Zen Free 账号）会请求官方无鉴权目录 `https://opencode.ai/zen/v1/models`。失败或结果为空时保留旧快照。
- Custom 的 **刷新模型目录**（选择该 Custom 账号）按已配置的 base URL 发现模型，不改声明能力。发现结果截断会提示；失败时保留旧快照。账号表单里的 **获取模型** 仍是另一项显式编辑，只把 ID 合并进尚未保存的能力列表。
- Command Code GOAT 的 **刷新模型目录** 使用所选已验证 GOAT 账号调用官方 `GET /provider/v1/models`。成功会同时更新该账号的允许目录和共享 Provider 目录；失败保留最后成功快照。

已保存的 Go/GOAT 目录会进入本地 Alias 解析，不会再触发上游调用。只有 effective、已知、已启用协议的模型才会对客户端公布；陌生的 Go ID 会显示在 Provider 目录中，但在协议未知时仍会封闭路由。Zen Free 仍会为每个已保存的 `-free` ID 额外生成去掉后缀的 Alias，见 [Zen Free 模型](routing.zh-CN.md#zen-free-模型)。

每行都有 **测试** 按钮。它会自动选择该范围内的第一个账号，并探测该模型的全部协议。Popconfirm 会提示探测可能消耗额度。Custom 端点范围不显示测试按钮，因为 Custom 账号级协议探测没有 V3 对应端点。探测成功只能在适配器结构上限内确认或新增支持。

**价格** 按所选供应商限定范围。**刷新价格表** 只抓取并校验当前所选 Provider 自己的官方来源。OpenCode 与 Command Code 的 revision 和最后成功快照彼此独立；一个失败不会动另一个。以后某个 Provider 若包含多个有价格的 Plan，一次操作也只刷新该 Provider 内的 Plan。刷新仍只能手动发起：

- OpenCode Go 展示 revision、文档更新时间、窗口额度、token 单价、`Usage` 和额度扣减倍率，点击刷新后才会访问 `https://opencode.ai/docs/go/`。抓取或校验失败时继续使用最后一次成功快照。allowance 不是额度池、不会参与路由，只用于推导扣减倍率（“月额度 / Usage”）。临时覆盖会创建新的持久化 revision，供后续估算使用。
- Command Code GOAT 展示从 `https://commandcode.ai/docs/plans/goat` 保存的官方订阅/费率快照。它只作展示参考，不进入 OpenCode Go 额度扣减，也不会虚构 GOAT 用量 API。
- Zen Free 无价格（额度按出口 IP 共享）。
- Custom API 为 unpriced：成功转发记 `cost_state=unknown`，不扣额度，也没有官方用量刷新。

不存在按模型划分的额度池。

客户端请求不会探测：请求路径不会发现或探测。流程是：别名 → 账号资格 → 适配器上限 → 已保存合约 → 按模型/按协议 effective 状态 → 透传或转换。带鉴权的 `GET /v1/models` 与受保护的 `GET /dashboard/api/v3/application-models` 只公布当前可路由且 effective 协议已启用的模型。应用选择器仍是 Go 别名 ∩ 当前价格快照，不含 Custom。

---

[用户指南索引](../USER.zh-CN.md) · [English](providers.md) · [文档索引](../README.zh-CN.md)
