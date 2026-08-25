[English](conventions.md)

# 编码约定

- **Ponytail 原则——能删就不加。** 优先复用现有 helper，只在真实需求出现时引入抽象。调用点保持扁平，但保留必需的 CAS、墓碑与 fail-closed 检查。
- **保持 crate DAG**。domain 与 gateway 保持无 I/O。门面按条目再导出。适配器返回 `AttemptSpec`。`forward_once` 是一次上游调用。Dashboard V3 不导入 `gateway`。
- **前端不新增 Tauri `invoke()` 路径**。Vue 主数据路径是 HTTP `/dashboard/api/v3`；不注册 `generate_handler`。
- **受保护的 V2 REST 保持退役状态**。新 JSON 属于 V3。410 墓碑挡在已退役 `/dashboard/api/...` 路径前面。
- **安全边界不能为了简化而削弱**。Gateway 鉴权、Key 混淆、URL 校验、冷却写入、SSE 透传以及 ConnectionInfo 密钥边界均不可移除。
- **不引入远端同步**。每个节点由自己的面板管理。
- **`auto_start` 与 `show_dock_icon` 受能力门控**。只有 Windows release / 已安装的 Tauri 进程注入注册表同步钩子；Dock 仅 macOS Tauri。
- **本地 Alias 列表保持本地**。带鉴权的 `GET /v1/models` 与面板 `application-models` 不在请求时增加上游发现。供应商页上的显式 Zen Free 刷新是唯一目录抓取例外，且只访问固定官方 endpoint。两份列表保持独立；不发明 `requested_alias` 日志字段。
- **尊重 `parking_lot::Mutex` 不可重入**。CLI 与 core 均使用。函数需要调用另一个持锁函数时，先 `drop` 外层 guard。
- **风格与周围一致**。注释密度、命名、惯用法跟现有代码保持一致。

---

[维护者指南索引](../MAINTAINER.zh-CN.md) · [English](conventions.md) · [文档索引](../README.zh-CN.md)
