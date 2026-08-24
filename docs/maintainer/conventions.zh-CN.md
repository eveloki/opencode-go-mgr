[English](conventions.md)

# 编码约定

- **Ponytail 原则**：能删就删，能复用现有代码就复用。代码库偏向扁平调用点，只为真实需求引入抽象，同时保留必需的 CAS、墓碑与 fail-closed 检查。
- **保持 crate DAG**。domain 与 gateway 保持无 I/O。门面按条目再导出。适配器返回 `AttemptSpec`。`forward_once` 是一次上游调用。Dashboard V3 不导入 `gateway`。
- **前端 Tauri `invoke()` 路径不新增**。Vue 主数据路径是 HTTP `/dashboard/api/v3`；`generate_handler` 不在当前设计中使用。
- **受保护的 V2 REST 保持退役状态**。新 JSON 属于 V3。410 墓碑挡在已退役 `/dashboard/api/...` 路径前面。
- **安全边界需要保持完整**。Gateway 鉴权、Key 混淆、URL 校验、冷却写入、SSE 透传以及 ConnectionInfo 密钥边界不应为了简化而被移除。
- **远端同步不在当前设计中**。每个节点由自己的面板管理。
- **`auto_start` 与 `show_dock_icon` 受能力门控**。只有 Windows release / 已安装的 Tauri 进程注入注册表同步钩子；Dock 仅 macOS Tauri。
- **本地 Alias 列表保持本地**。带鉴权的 `GET /v1/models` 与面板 `application-models` 不在请求时增加上游发现。供应商页上的显式 Zen Free 刷新是唯一目录抓取例外，且只访问固定官方 endpoint。两份列表保持独立；`requested_alias` 不是已定义的日志字段。
- **沿用现有测试的并发约定**。CLI 与 core 用 `parking_lot::Mutex`，不可重入；函数需要调用另一个持锁函数时，先 `drop` 掉外层 guard。
- **风格与周围一致**。修改某段代码时，新代码要像旧代码：注释密度、命名风格、惯用法保持一致。

---

[维护者指南索引](../MAINTAINER.zh-CN.md) · [English](conventions.md) · [文档索引](../README.zh-CN.md)
