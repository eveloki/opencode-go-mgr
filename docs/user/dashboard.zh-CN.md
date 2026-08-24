[English](dashboard.md)

# 管理面板

管理面板是 Gateway 提供的单页 Vue 3 应用。左侧边栏（宽度低于 1024px 时改为顶部横向菜单）共七个视图：**仪表盘**、**接入 Key**、**账号**、**供应商**、 **应用**、**日志**、**设置**。顶栏右侧是主题切换、语言切换、退出登录。

面板原生支持十种语言：简体中文、繁體中文、English、日本語、한국어、Español、 Français、Deutsch、Português (Brasil)、Русский，默认简体中文。语言选择持久化在 `localStorage` 的 `ocg-manager.locale`；如果浏览器拒绝持久化（例如隐私窗口），当前会话仍能正常使用。

## 面板 V3

当前面板 SPA **只** 访问 `/dashboard/api/v3`。接入中心、接入 Key、账号、供应商、应用、日志、设置，以及登录 / 注册 / 退出，都走这条数据路径。写入会带上最近一次见到的 `expectedRevision` 与 `processGeneration`。若同一运行进程中的另一个标签页先保存，服务端会以 HTTP 409（`revisionConflict`）拒绝陈旧写入。当前 SPA 尚不能用这个真实错误码自动恢复冲突，因此请手动刷新受影响的页面并重新提交。这些 token 只属于当前进程；多个进程共用一个数据目录时不构成统一 CAS 域。OpenCode Go 价格快照另有独立的 `pricingRevision`，与这些设置 token 不是同一套计数。

明文 Key 只出现在接入中心载荷（`GET /dashboard/api/v3/connection`）里。Settings 资源从不包含 Key 值。浏览器只把这些秘密留在内存；退出登录或 401 会话失效会立即清除。

七个视图在切换标签时保持缓存（`KeepAlive`）。回到某个视图会刷新该页的服务端数据；仪表盘还会在浏览器标签重新回到前台时刷新。目录、价格与供应商模型列表不会自动轮询。官方用量同步是服务端调度，不是面板轮询。从设置页开始签名桌面安装后，该页可能轮询安装进度，直到进程重启。

仍在调用已退役 `/dashboard/api` REST（而不是 `/dashboard/api/v3`）的旧版缓存页面会收到 HTTP 410，错误码 `dashboardV2Removed`，并提示先刷新页面，不够再升级。未登录的退役 REST 会先返回 401。两类 V2 路径仅作为兼容例外保留，**不是**当前 SPA 数据路径：`/dashboard/api/auth/status`、`/dashboard/api/auth/register`、 `/dashboard/api/auth/login`、`/dashboard/api/auth/logout`，以及 `/dashboard/api/browser/sessions/{token}/ws`。当前面板改用 V3 的鉴权与浏览器 WebSocket 路由。

面板上没有 **Ping** 按钮。要从本产品探测 OpenCode Go Key，请用 CLI `key ping` 或发一次真实客户端请求。Custom 卡片仍有 **验证连接**；托管注册仍有 Key 验证。

## 接入中心

首屏第一个面板——也是始终在最上方的面板——是 **接入中心**，它集中展示客户端需要的全部信息：

- **Key**：支持重新生成、一键复制，以及打开 **接入 Key** 页的「管理接入 Key」。重新生成只让当前选中 Key 的旧值立即失效，其他 Key 不受影响；存在多把启用 Key 时会出现选择器，用于切换展示（脱敏）值、复制目标与重新生成目标。复制会把完整明文 Key 写入剪贴板，在公用或共享设备上使用后请清除剪贴板历史。新建、改名、启停、删除在 **接入 Key** 页，不在接入中心。主 Key 与子 Key 一样只用重置生成新值，没有自定义主 Key 输入框。
- **API Base URL**（例如 `http://127.0.0.1:9042/v1`）：一键复制，另附 Chat Completions、Responses、Messages 的完整端点。
- **Gateway 转发到的上游地址** 与复制按钮。
- **HTTP 警告**：当解析出的根地址是非回环的明文 `http://` 时出现，提醒 Key 与请求内容会明文传输。

**设置 → 下游访问根地址（Downstream Access Root）** 只控制面板展示的 URL 和教程里复制的 URL。有效值按以下顺序决定：

1. 非空的 `OCG_CLIENT_ROOT_URL` 环境变量。
2. 面板保存的手工值。
3. 自动推导值：生产面板使用当前 origin，开发面板使用 `http://127.0.0.1:<Gateway 端口>`。

环境变量接管时输入框为只读，修改变量并重启后生效，变量值不会写入 SQLite。自动值会显示在输入框中，但不会被保存。

如果客户端通过反向代理或别的主机访问 Gateway，就设置外部可访问的根地址，例如 `https://ocg.example.com`。尾部的 `/v1` 会被自动识别并去掉。**这个设置不会** 改变 Gateway 的监听地址、配置 DNS、也不会创建反向代理——这些必须已经指向正在运行的 Gateway。明文 HTTP 允许用于局域网部署，但会暴露 Key 与请求内容。

## 接入 Key

**接入 Key** 视图是客户端凭证的主名单。主 Key 与子 Key 一起存放在 `access_keys`（schema v27）。新建、改名、启停、重新生成和删除都走 Dashboard V3；成功变更会 bump settings revision。变更回执不含明文——页面会重新加载接入中心以展示新值。

- **主 Key** 恒为有效，不能停用或删除；用重置生成新值。其 id 为 `00000000-0000-0000-0000-000000000001`。它也是客户端教程默认展示的凭证。没有自定义主 Key 输入框。
- **子 Key** 是额外创建的凭证，可命名、重命名、启用/停用、重新生成或删除——适合每台设备分发一把。删除子 Key 是软删除：立即失效且明文被清除，但转发日志仍能按名称归因。子 Key 的值不能与主 Key 或其他子 Key 相同；未删除的子 Key 最多 64 把。

接入中心和应用页只消费已启用的 Key。按 Key 看用量在日志页过滤。

---

[用户指南索引](../USER.zh-CN.md) · [English](dashboard.md) · [文档索引](../README.zh-CN.md)
