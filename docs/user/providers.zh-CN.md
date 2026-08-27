[English](providers.md)

# 供应商

**供应商** 是供应商控制面——如果你的旧书签还挂着 `?view=pricing`，进来的就是这个视图。

底层是静态 Provider Registry 加几个按能力拆分的适配器。Custom API 只是其中一个 Configurable HTTP 适配器，不是基类，其他方案不会继承它。范围划分如下：

- 内置家族使用 `Provider(provider_id)`。
- 每个 Custom 目的地使用 `CustomEndpoint(account_id)`。Custom 端点彼此隔离，也与内置家族隔离。

左侧列出这些范围。主区有两个子页签：**模型目录** 与 **价格**。原来的目录与模型合约视图合并为模型目录页签上的一张矩阵表。

**模型目录** 是本地的。矩阵只列出当前目录中的模型，并以三个上游协议（Chat Completions、Responses、Messages）为列。每格是 effective 模型/协议状态的二态开关：打开写入 `force_on`，关闭写入 `force_off`；列菜单可以整列打开或关闭。开关会先立即更新显示，再在后台执行带 CAS 保护的保存，只有受影响的格子显示保存进度。

底层静态、预设与探测证据仍保留在合约中，但紧凑矩阵不再显示独立徽标。显式开关或成功探测写入覆盖前，存储默认仍是 `auto`。供应商级探测成功会固定为 `force_on`；账号尝试失败会报告并保留证据，但不会把共享协议固定为 `force_off`，只有显式关闭开关才会这样做。

内置 **OpenCode Go**、**Zen Free** 与 **Command Code GOAT** 的目录头部都提供 **恢复静态协议快照**。它不会请求上游，保留当前模型目录，清除手动开关和探测证据，并恢复日期为 **2026-08-27** 的静态协议快照。当前目录中未出现在该快照里的模型/协议对会被显式保持为关闭，因此新发现模型不会只因回退逻辑就变成可路由状态。

轻量来源信息、刷新动作与矩阵共用同一块内容区域，不再有独立的目录摘要卡片，也没有刷新账号选择器。所有可刷新的范围使用同一个动作：OpenCode Go 由后端选择符合条件的 Go 账号访问官方鉴权目录；Zen Free 访问固定的官方无鉴权目录 `https://opencode.ai/zen/v1/models`；Command Code 直接访问固定的公开官方 `/models` 目录，不选择账号。刷新始终由用户显式触发。

首次成功刷新前，内置静态目录只是初始预设；刷新成功后，保存的官方快照成为权威目录并替代静态预设。刷新新增的模型会出现在矩阵中，但 Chat Completions、Responses、Messages 三个协议默认全部关闭；只有手动打开单元格或测试成功后才会启用。仍留在目录中的模型会保留既有覆盖与探测结果；刷新失败或结果为空时继续保留旧快照。

Custom API 继续使用各账号声明的模型 ID，发现结果不会静默替换声明。账号表单里的 **获取模型** 只是未保存表单辅助，把选中的 ID 合并进正在编辑的声明。Command Code 使用官方公开的 `/models` 目录：GOAT 预设默认启用，后续发现的额外模型默认关闭，只有在矩阵中开启其受支持协议后才会供应；不再存在独立的 Max 或账号级 GOAT/全部模式。

本地目录会进入 Alias 解析，请求时不会再访问上游。只有 effective、已知、已启用协议的模型才会对客户端公布。Alias 名以 OpenCode Go 目录为基准；Zen Free 只公布去掉 `-free` 后缀的 Alias，原始 `-free` ID 仍可作为精确 raw pin 使用，见 [Zen Free 模型](routing.zh-CN.md#zen-free-模型)。

当某个供应商的模型/协议单元格全部关闭时，该供应商不再产生路由；当一个 Alias 在所有供应商都没有启用映射时，它会从下游 `GET /v1/models` 供应中移除。

每行都有 **测试** 按钮，不需要指定账号。对于该模型的每个协议，供应商会按已保存的路由顺序自动尝试符合条件的账号，并在首次成功后停止。Popconfirm 会提示这些真实最小请求可能消耗额度。Custom 端点范围不显示测试按钮，因为 Custom 账号级协议探测没有 V3 对应端点。模型必须属于当前供应商目录；通过校验后会真正测试三个协议端点，包括静态表尚未收录的新拉取模型。页面会在矩阵上方逐项展示成功、失败或跳过状态、HTTP 状态、可读的上游错误消息，以及上游给出时的安全帮助/计费链接；每个真实账号尝试都会写入脱敏的请求日志，协议探测内容不会进入运行日志。单个账号失败不会禁用其他符合条件账号可以服务的协议。

**价格** 按所选供应商限定范围。**刷新价格表** 只抓取并校验当前所选 Provider 自己的官方来源。OpenCode 与 Command Code 的 revision 和最后成功快照彼此独立；一个失败不会动另一个。以后某个 Provider 若包含多个有价格的 Plan，一次操作也只刷新该 Provider 内的 Plan。刷新仍只能手动发起：

- OpenCode Go 展示 revision、文档更新时间、窗口额度、token 单价、`Usage` 和额度扣减倍率，点击刷新后才会访问 `https://opencode.ai/docs/go/`。抓取或校验失败时继续使用最后一次成功快照。allowance 不是额度池、不会参与路由，只用于推导扣减倍率（“月额度 / Usage”）。临时覆盖会创建新的持久化 revision，供后续估算使用。
- Command Code GOAT 展示从 `https://commandcode.ai/docs/plans/goat` 保存的官方订阅/费率快照。它只作展示参考，不进入 OpenCode Go 额度扣减，也不会虚构 GOAT 用量 API。
- Zen Free 无价格（额度按出口 IP 共享）。
- Custom API 为 unpriced：成功转发记 `cost_state=unknown`，不扣额度，也没有官方用量刷新。

不存在按模型划分的额度池。

客户端请求不会探测：请求路径不会发现或探测。流程是：别名 → 账号资格 → 适配器上限 → 已保存合约 → 按模型/按协议 effective 状态 → 透传或转换。带鉴权的 `GET /v1/models` 与受保护的 `GET /dashboard/api/v3/application-models` 只公布当前可路由且 effective 协议已启用的模型。应用选择器仍是 Go 别名 ∩ 当前价格快照，不含 Custom。

---

[用户指南索引](../USER.zh-CN.md) · [English](providers.md) · [文档索引](../README.zh-CN.md)
