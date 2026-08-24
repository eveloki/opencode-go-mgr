[English](dashboard-api.md)

# Dashboard API

## Dashboard V3

当前面板 JSON 是 `/dashboard/api/v3`，与已退役 V2 REST 墓碑并列挂载。线协议 DTO 使用 camelCase，变更体 `deny_unknown_fields`，可空响应字段始终序列化为 `T | null`。

控制面身份：

- `settings_revision` — `CoreState` 上的内存 `AtomicU64`，成功持久化后 bump。 CAS 令牌本身不存 SQLite。
- `process_generation` — 每个 `CoreState` 赋值一次，不会持久化。上一进程的 CAS 令牌在重启后不能复用。
- `pricingRevision` — 不可变快照 id。价格变更还要带 `expectedPricingRevision`。

变更要求顶层 `expectedRevision` **和** `processGeneration`（包括 `/auth/register`、`/auth/login`、`/auth/logout` 以及 `POST /accounts/{id}/usage/refresh`）。缺少 `expectedRevision` 是专门的 `400` `missingExpectedRevision`。不匹配是 `409` `revisionConflict`，错误信封带 `currentRevision` / `processGeneration`。Vue `controlPlane` store 从每个 V3 载荷记录令牌。预期的 409 恢复是从 `GET /contract` 刷新令牌且不重放变更，但当前客户端仍检查旧的蛇形错误码 `revision_conflict`；见 [已知缺口](known-debt.zh-CN.md)。revision 与 generation token 只属于当前进程，不能协调共用同一数据目录的多个进程。

不是变更（无 CAS、不 bump）：`POST /settings/test-proxy`、 `POST /custom/models/discover` 这类操作探测。`GET /settings/check-update` 与 `GET /settings/update-status` 捕获 revision/generation 且不会 bump。 `POST /settings/install-update` 需要 CAS，原子启动，不 bump，不持有网络/DB 锁。

密钥边界：明文 Key 不会出现在 `Settings` 或供应商/Zen/合约 DTO 上。 `ConnectionInfo`（`GET /connection`）是 **唯一** 携带密钥的 V3 响应 DTO（主 Key 与所有未软删的子 Key 值，包括禁用子 Key，处于 dashboard 会话保护层）。只有启用的 Key 会进入鉴权快照。 `CustomModelDiscoveryRequest.apiKey` 只写。账号 list/get 载荷保持无密钥。日志与错误信封脱敏已知密钥。

冻结契约是 `schema/dashboard-api-v3.schema.json`，由 `dashboard_v3::contract_schema_pretty()` 经 `crates/ocg-core/examples/export_dashboard_v3_schema.rs` 生成。生成的 TypeScript（`src/api/generated/dashboard-v3.ts`）只有类型，没有 HTTP 封装。 `dashboard_v3/types.rs` 的 `CATALOG_TYPE_NAMES` 是有序 `$defs` 目录；追加时既有 definition 对象必须保持字节一致。

前端：Pinia store 直接调用 `dashboardV3`。仍使用旧字段名的现有页面走 `src/api/dashboard.ts` presenter。请保持 presenter 仅做字段映射，避免加入 V2 导入、路由回退或递归大小写转换。

`dashboard.rs` 仍包含历史 V2 REST 处理器，并提供面板 HTML/资源。这些受保护 REST 处理器 **不是** 现行 API：`host_router` 会先拦截已退役的 `/dashboard/api/...` 路径。

## 已退役的 V2 REST

受保护的 Dashboard V2 REST 已退役。

- 匿名已退役 REST：空 body 的 **401**（鉴权先于墓碑）。
- 已鉴权的已退役 REST（含回环本地模式）：**410**，body 为 `{ "code": "dashboardV2Removed", "message": "Dashboard API V2 has been removed; refresh the page and retry. " }`。
- 既非 V3 也非保留家族的未知 `/dashboard/api/...` 路径，在已鉴权时同样 410。

保留的 `/dashboard/api` 家族（精确路径，无尾斜杠，无额外段）：

- `auth/status`、`auth/register`、`auth/login`、`auth/logout`
- `browser/sessions/{token}/ws`（token 非空）

V3 在 `/dashboard/api/v3/...` 下有自己的鉴权与浏览器 WebSocket。当前 Vue 外壳使用 V3。推理路由、面板 HTML 与 `/dashboard/assets/...` 不在墓碑范围内。

---

[维护者指南索引](../MAINTAINER.zh-CN.md) · [English](dashboard-api.md) · [文档索引](../README.zh-CN.md)
