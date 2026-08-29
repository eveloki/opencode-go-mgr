[English](extending.md)

# 扩展 OCG Manager

## 新增或修改供应商（封闭）

1. 在 `ocg-domain`（`ids.rs`、`provider.rs`）加入身份与目录事实。穷尽扩展 `ProviderAdapterKind`（`ALL`、`from_offering`、能力组合）。Custom 保持 `ConfigurableHttp`，它不是超类。
2. 若该家族需要协议行，加在 `ocg-domain::protocol`。请求路径不会用来试探协议。
3. 若该家族需要别名，加在 `ocg-gateway::alias`。不可路由 mapping 可以被识别但不产出生产路由。
4. 在 `ocg-core` 为新 kind 实现 `resolve_route`，只返回 `AttemptSpec`。 **适配器不能持有 DB、`CoreState` 或原始 reqwest 客户端。** 解密与 HTTP 留在宿主 resolver / `forward_once`。
5. 在所需路由与控制面语义真正实现之前 fail closed。Command Code 是「固定官方源已上线，公开目录不等于账号 Key 验证」的模板。
6. 跑 `cargo test -p ocg-domain`、`cargo test -p ocg-gateway` 与 `cargo test -p ocg-core`。纯度/依赖守卫会拦截越界导入。

Provider 注册表是静态密封的。不提供插件加载器、动态库或用户提供的适配器脚本。

## 新增或修改 Dashboard V3 端点

1. 在 `dashboard_v3/types.rs` 增加或扩展 DTO，并把新名字追加到 `CATALOG_TYPE_NAMES`。既有 `$defs` 对象保持不变。
2. 在 `dashboard_v3/mod.rs` 挂路由。变更走 `parse_mutation_json` + `check_expectation`。保持密钥边界。
3. 优先复用 `account_control` / `gateway_keys` / `control::observability`；持久化逻辑不应被复制，`dashboard_v3` 也不应依赖 `gateway`。
4. 在 `crates/ocg-core/tests/dashboard_v3_*.rs` 补集成测试。
5. 跑 `pnpm run contract:v3:generate`（CI 用 `--check`），并更新 `src/api/dashboard-v3.ts` 手写客户端。只有现有页面仍需要旧形状时才改 `dashboard.ts` / `dashboard-presenters.ts`。
6. `/dashboard/api` REST 保持退役状态；新的受保护 JSON 属于 V3。

## 新增宿主能力

桌面能力加入 `src-tauri/src/host/` 并注册到 `CoreState`。不要重新引入 `#[tauri::command]`；Vue 面板只走 HTTP。

## 故障模式

| 现象 | 可能原因 | 处理 |
| --- | --- | --- |
| 面板 JSON `410` `dashboardV2Removed` | 客户端仍在调用 `/dashboard/api/...` REST | 刷新/升级页面；改用 `/dashboard/api/v3` |
| 面板 JSON `409` `revisionConflict` | 过期的 `expectedRevision` / `processGeneration` / `expectedPricingRevision` | 重新加载资源；客户端不会自动重放变更 |
| 面板 JSON `400` `missingExpectedRevision` | 变更体漏了 CAS | 发送 `expectedRevision` + `processGeneration` |
| `/dashboard/api/...` 上空 body `401` | 匿名已退役 REST 或缺少会话 | 登录；回环只对 **直接** 请求跳过登录 |
| Gateway `400` `ambiguous_model_id` | 原始 ID 映射到多个家族（含 Custom 重叠） | 改名/避开冲突的 Custom ID；系统不会调用上游 |
| Gateway `400` 未知模型 | 名称既非已公布别名也非合格 Custom ID | 使用 `/v1/models`；协议探测不在请求路径进行 |
| 推理 `401` 原样返回、不换号 | Go `ModelError`/未知错误体或任意 Zen 401 | 预期行为；只有精确的 Go `CreditsError` 会换号并记录 `auth_error` |
| Zen `429` 冷却所有 Free 卡 | 出口 IP 共享池 | 等待 `cooldown_free_until`；后续非 Free 卡仍可能运行 |
| `success_no_usage` | 上游未发出 usage chunk | Chat 流式会请求 `include_usage`；没有 chunk 时该行用量缺失 |
| 打开失败：schema 新于 29 | 数据目录来自更新的二进制 | 恢复匹配备份；旧二进制无法打开 v29 |
| 打开失败：cipher / 密文 | 错误的 `.encryption-key` 或机器绑定上下文 | 恢复匹配密钥；密文不会被改写 |
| 中断的 v29 open | 事务已回滚；pre-v3 备份可能已存在 | 见 [storage-migration.zh-CN.md](storage-migration.zh-CN.md) |
| 设置改端口后仍绑旧端口 | 重绑失败；补偿恢复了配置 | 查 gateway 日志；并发写入由 `settings_host_effects` 串行 |
| `stop_gateway` 后用量循环仍在跑 | 监听器停止不会取消 `ControlPlaneWorkers` | drop `CoreState`（进程退出） |

---

[维护者指南索引](../MAINTAINER.zh-CN.md) · [English](extending.md) · [文档索引](../README.zh-CN.md)
