[English](accounts.md)

# 账号

每张账号卡绑定一个 **Plan**（provider + offering）；该 Plan 需要时绑定一份凭据。额度权威随 Plan 而异：OpenCode Go 按账号 / Key 统计，Zen Free 按出口 IP 共享额度与冷却，Custom API 不做供应商额度核算。所有卡片共享一份可手动持久化的全局顺序；请求先经过能力过滤，严格优先级、全局粘性和轮询三种路由再复用这份顺序。没有按模型划分的额度池。面板仍是七个视图：**仪表盘**、**接入 Key**、**账号**、**供应商**、**应用**、**日志**、 **设置**。

**账号** 负责身份、账号 Key、验证、启用状态、卡片顺序、托管注册，以及账号用量 / 校准 / 冷却。每张卡展示只读的合约摘要（有效协议，或全部供应商协议已关闭 / 该范围当前不可路由），以及指向 **供应商** 对应范围的 **前往供应商** 深链。本地目录、协议探测、Chat Completions / Responses / Messages 开关和范围内价格都在 **供应商** 页，不在账号卡上。

内置 Plan 家族如下：

| 家族 | Plan | 可路由 | 说明 |
| --- | --- | --- | --- |
| OpenCode Go | `opencode` / `go` | 是 | 每张卡一份官方分发的 API Key；托管注册仍是 Beta |
| Zen Free | `opencode-zen-free` / `anonymous-free` | 是 | 一张不带鉴权头、无需凭据的匿名单例卡；可排序、可启停，不可删除；额度按出口 IP 共享 |
| Command Code GOAT | `command-code` / `goat` | 是 | 创建后禁用并为 `pending`；通过官方 `GET /models` 验证 Key，再显式启用；模型级别默认包含套餐 `goat`，可切换为 `all` |
| SCNet Token Plans | `scnet` / `token-plan-basic`、`token-plan-standard`、`token-plan-premium` | 否 | 已归档；历史行保持禁用，不能验证、启用、路由或报告用量 |
| Custom API | `custom` / `api` | 是 | 受信管理员目的地；创建/更新后仍为禁用 `pending`；先验证再显式启用；合格声明 ID 会出现在 `/v1/models`；费用 unpriced/unknown，不扣额度 |

所有持久化变更路径（数据库闸口，以及 dashboard / CLI 共用服务）都会在改动行、revision 或时间戳之前，拒绝为目录内 `routable=false` offering（全部 SCNet tier）设置 `enabled=true`。GOAT 与 Custom 在目录中可路由，但创建/更新后仍为禁用 `pending`；验证状态不是 `verified` 时拒绝启用。禁用草稿仍可保存。桌面 UI 只经 Dashboard V3 HTTP 变更，没有独立的 Tauri invoke 变更路径。

OpenCode Go 与 Command Code GOAT 只接受各自官方 Provider API Key；浏览器 Cookie 与反向代理凭据不是账号 Key。GOAT 是独立供应商映射，其 Key 只会发送到固定的 Command Code Provider API，绝不会发送到 OpenCode。SCNet 已归档，不再接受新凭据或路由。Custom API 同样不是 OpenCode offering，不会把 Key 发往 OpenCode endpoint；它是独立的受信管理员目的地。

历史 SCNet Token Plan 行及其旧确认记录会为兼容保留，但该家族已经归档。它没有新增、验证、启用、路由、模型公布、价格或用量路径。

GOAT 验证只执行一次带鉴权且不计费的 `GET /models`，保存返回的账号目录，但不会自动启用卡片。验证后必须显式启用。账号模型级别可选套餐内 `goat`（默认）或完整 `all` 目录。更换 Key 会使验证失效并禁用账号；切换 `goat`/`all` 不会。

Custom API 是已上线的受信管理员目的地。账号卡保存 base URL、一种上游协议（Chat Completions、Responses 或 Messages）、一种鉴权方案（Bearer 或 `x-api-key`），以及至少一条模型能力。受信管理员可配置任意语法合法的 HTTP 或 HTTPS 源，包括局域网、回环与其他自选目的地。URL 内嵌凭据、query 与 fragment 会被拒绝。Gateway 不会跟随重定向，也不会转发 dashboard 或客户端鉴权，只构造已配置的 Bearer 或 `x-api-key`。拼接后的 endpoint 必须保持已配置的 scheme、host、port 与 base-path 前缀。Custom HTTP 使用同一套进程级 Direct / Manual / Auto 代理策略；连接与请求超时按配置的连接超时夹到 5–60 秒。

**获取模型** 是表单中的显式操作：它仅使用当前配置的 base URL 发出 `GET /models`，并使用新建表单输入的 Key 或编辑时已保存的 Custom Key。返回的合法 ID 会合并到仍可编辑的模型列表；该操作不会保存、验证、启用或以其他方式修改账号。获取有边界，可能提示结果被截断；仍可手动填写模型 ID。

创建与更新后账号仍为禁用 `pending`。验证对第一个声明模型发送一次协议正确、非流式、 token 受限的 JSON 请求；只有 `2xx` JSON object 才算成功。验证不会发现或改写能力，也不会自动启用账号。验证成功后必须显式启用。合格账号（enabled + verified + ready + 非空 Key）会把声明的模型 ID 暴露在带鉴权的 `GET /v1/models` 上，并可为这些 ID 被选中。声明的能力 ID 既是客户端名称也是上游模型名；kebab ID 按大小写折叠匹配，含 `/`、`_` 或空白的名称不会折成 kebab 别名。Custom overlay 不会抢占已公布的 Go 或 Zen Free 别名。与另一 Plan 的唯一原始 ID 重叠时返回 `ambiguous_model_id`，且不调用上游。未声明名称仍视为未知（`400`）。更改 base URL、Key 或声明能力会使验证失效并禁用账号。上游协议与鉴权方案在创建后不可改。Custom 流量不计价：日志记 `cost_state=unknown`，不扣额度，也没有官方用量刷新。`MODEL_PROTOCOLS` 仍只服务 OpenCode Go；Custom 把客户端协议转换到该账号声明的上游协议。

**账号** 的 **新增账号** 是分组方案列表加详情（**可添加** / **草稿方案** / **暂不可用**），不是卡片网格。Zen Free 是系统管理的单例，不会出现在新增列表里，只在账号列表中启停。选中 OpenCode Go 后，详情里仍提供 **导入已有 Key** 与 **注册新账号（Beta）**：

- **Key 账号**直接保存一份官方分发的 OpenCode Go API Key。
- **托管账号**先创建一条禁用的可恢复草稿，再按向导完成登录身份（可选）、邀请注册、支付、Key 验证。草稿与当前步骤会立即写入 SQLite；关闭页面或重启服务后可继续。注册中账号不会参与 Gateway 路由，也不会显示用量、验证或启用控件。

托管注册与独立浏览器 Profile 是 **Beta** 功能，尚未经过充分测试；生产环境仍不建议依赖。

创建托管草稿时会直接展示 **邀请链接**（预填当前设置值；新安装默认带演示链接）。可当场编辑：须为无用户名/密码、最长 2048 字符的 HTTPS URL，主机只能是 `opencode.ai` 或 `console.opencode.ai`。与设置中不同时会写回 **设置 → OpenCode Go 邀请链接**；修改只影响以后打开邀请页，不会改写已完成账号。默认演示链接仅供试用，正式注册前请换成你自己的邀请链接，否则邀请收益归链接所有者。

托管向导是纯人工流程（不代填密码、不代点支付、不自动提取 Key）：

1. **登录身份（可选）**：需要新账号时再注册 Google 或 GitHub；已有任一账号可 **跳过此步**。OpenCode 登录也可在下一步完成。
2. **邀请注册**：在同一独立 Profile 中打开邀请链接，用 Google 或 GitHub 完成 OpenCode 登录/注册。
3. **完成支付**：在控制台确认套餐与金额；支付仅由你在页面上完成。
4. **验证 Key**：从控制台复制 Key，粘贴后由 OCG Manager 真实请求上游验证。

点击步骤条可 **回退到已完成的更早步骤**；前进仍靠各步主按钮。验证返回 `2xx` 时账号完成并启用；`429` 也表示 Key 有效，账号会完成并记录现有冷却；`401`/`403`、网络错误或 `5xx` 会停留在 Key 验证步骤，允许修正后重试。

每个账号都有长期独立的浏览器 Profile。桌面端会启动外部 Chromium 系浏览器： Windows 优先 Edge、其次 Chrome；macOS 查找 Chrome、Edge、Chromium；Linux 桌面从 `PATH` 查找 Chrome、Chromium 或 Edge。浏览器仅使用独立 `browser-profiles/<account_id>`、跳过首次运行提示并打开新窗口，不启用 CDP、自动化或降低安全性的参数。升级前旧的 `profiles/<account_id>` WebView 数据不会导入，因此首次需要重新登录。

所有完成账号都提供 **打开 OpenCode 控制台**（`https://opencode.ai/auth`）。旧账号第一次会打开空白的独立 Profile，用户登录一次后 Cookie 会长期保留。Google / GitHub 与 OpenCode 的 Cookie 分属不同域，但都保存在该账号的同一个 Profile 中。

重置浏览器身份会先关闭该账号的浏览器并删除新旧 Profile：完成注册的账号保留 Key，只退出控制台登录；注册中的托管账号还会回到登录身份步骤。删除账号也会删除浏览器 Cookie/Profile，确认框会明确提示这一点。之后这些登录状态无法从 OCG Manager 恢复，只能从备份恢复或重新登录。

每张已完成的 OpenCode Go 账号卡显示账号名、冷却状态，以及由本地估算驱动的 5 小时、本周、本月用量条。Zen Free 使用独立的匿名、按出口 IP 共享的 free 冷却，不使用某个 Key 的额度。

- **用量校准**：每个窗口都可以输入百分比或拖动进度条，将其保存为当前实际用量基线；保存后，OCG Manager 记录的成功请求成本会继续累加到该基线上。达到 100% 仍只是提示，不会阻止 Gateway 选择这个账号。手工校准对所有已完成账号都可用。
- **刷新额度（已完成的 Key / 托管账号）**：官方用量（`/zen/go/v1/usage`）是周期性校准基线；本节点转发日志成本在上次成功同步后仍做实时估算。已启用且近 24 小时有本地活动的 ready 账号约每小时自动对账，无活动约每天一次；禁用、未完成或空 Key 账号不会自动刷新，打开账号页不会触发请求。Gateway 启动时不会立即请求；尚无已保存调度的合格账号会分散到最初 0–15 分钟内，再按正常节奏对账。点击 **刷新额度** 仍走同一条安全路径，服务端每账号 15 秒节流（返回 Retry-After / next-allowed）。卡片会显示上次成功官方同步时间与临时重试等待，而不仅依赖按钮 loading。本地估算达到 ≥80% 时最多每 15 分钟加速对账一次。真实推理 `429` 仍写入现有冷却/选择器状态，并额外在约 1–2 分钟后调度一次官方对账；官方失败或 `status=rate-limited` 不会写推理冷却。失败保留上次基线与 last-success。请求走与其他面板出站相同的全局代理。
- **标识与凭据**：名称是必填的主要展示标识。登录账号可选；新增 Key 账号时如果先填写账号，它会自动同步为名称，手动修改名称后不再跟随。可选备注写在**编辑账号**里，可留空，不参与路由、不计入额度。面板保存账号 Key，但不收集或维护第三方登录密码。
- **购买日期**：新增账号默认使用浏览器当天，也可以在新增或展开编辑表单里修改。托管向导在确认支付进入 Key 验证时也会写入购买日期。到期日取下一个自然月同日；目标月份没有该日时取月末，例如 `2026-01-31` 的到期日是 `2026-02-28`。账号页与仪表盘显示剩余天数、今天到期或已到期天数。该信息只作提醒，不会自动禁用账号或阻止 Gateway 选择。
- **优先级顺序**：账号卡左侧的拖动手柄用于调整优先级，鼠标、触屏和触控笔都可以使用；聚焦手柄后也可以按上、下方向键移动。排序保存在当前节点的 SQLite 中，仪表盘、日志账号筛选、CLI 列表和 Gateway 选择器都使用同一顺序。
- **解除冷却**：冷却也可以在这个视图手动解除。解除后，进度条会立刻回到本地估算值。

---

[用户指南索引](../USER.zh-CN.md) · [English](accounts.md) · [文档索引](../README.zh-CN.md)
