[English](overview.md)

# 产品定位

OCG Manager 把供应商 API Key 保存在本地 SQLite（官方分发的 OpenCode Go Key，以及受信的 Custom API 目的地），并通过回环 Gateway `http://127.0.0.1:9042/v1` 暴露给客户端。每张账号卡对应一个 **Plan**（provider + offering）。客户端发送本地注册表里的 **别名** 或合格 Custom 模型 ID；当前可路由的是 OpenCode Go、OpenCode Zen Free 与 Custom API。同一个 Gateway 同时承载 Vue 3 管理面板（路径 `/dashboard/`）。当前面板 SPA 通过 `/dashboard/api/v3` 读写 JSON。每个节点都独立运行——项目不提供远端同步、 Admin API、遥测。

Gateway 的四件事：

1. 用面板签发的 **Key** 验证客户端。
2. 用本地 Alias 注册表（以及合格 Custom 声明 ID）解析客户端模型名，再经能力过滤、适配器上限、已保存的供应商合约，以及 Chat Completions / Responses / Messages 开关后挑一张可用账号卡。
3. 把请求转换到所选 Plan 的有效上游协议，再把响应转回客户端协议。客户端请求路径不会发现或探测。
4. 把请求日志（`requested_model`、`resolved_alias`、`upstream_model`）、用量、冷却全部写回 SQLite，并在面板里呈现。

---

[用户指南索引](../USER.zh-CN.md) · [English](overview.md) · [文档索引](../README.zh-CN.md)
