[English](docker.md)

# Docker

GHCR 上的公开无头镜像无需登录即可拉取。它是 Linux 容器，发布 `linux/amd64` 与 `linux/arm64`；直接 `docker pull` 会在对应架构上自动选择原生变体。每个 Release 也会附带只拉取镜像的 `compose.example.yaml`；把它保存为 `compose.yaml`，并按需在同目录创建 `.env`。示例默认固定对应的发布版本，也可用 `OCG_IMAGE` 覆盖。或者在包含 `compose.yaml` 与 `.env.example` 的仓库目录中运行（建议检出对应 Release tag）：

```bash
git clone --branch v1.8.2 --depth 1 https://github.com/klarkxy/opencode-go-mgr.git
cd opencode-go-mgr
cp .env.example .env
# PowerShell: Copy-Item .env.example .env
# Edit .env before exposing the service outside the host.
docker compose pull
docker compose up -d --no-build
docker compose ps
```

## 选择镜像

- 仓库内支持源码构建的 `compose.yaml` 默认使用 `ghcr.io/klarkxy/opencode-go-mgr:latest`；Release 中的 `compose.example.yaml` 默认固定对应的完整版本。
- 生产部署建议在 `.env` 中用 `OCG_IMAGE` 固定完整版本标签，例如 `ghcr.io/klarkxy/opencode-go-mgr:1.8.2`。
- 完整版本与 `sha-<commit>` 标签用于标识单次发布，按发布策略不应移动；`1.5` 与 `latest` 会继续移动。技术上只有 `ghcr.io/klarkxy/opencode-go-mgr@sha256:...` digest 真正不可变。
- 需要调试当前源码时，设置 `OCG_IMAGE=ocg-manager:local`，再执行 `docker compose up -d --build`。`NPM_REGISTRY` 与 `CARGO_REGISTRY` 只属于源码构建参数，不会改变已拉取镜像。

| 变量 | 作用范围 | 含义 |
| --- | --- | --- |
| `OCG_IMAGE` | Compose | 镜像标签、镜像站、本地名称或不可变 digest。 |
| `OCG_BROWSER_IMAGE` | Compose | 可选 Chromium/noVNC Sidecar 的镜像标签、镜像站、本地名称或 digest。 |
| `OCG_PORT` | Compose | 宿主机回环端口；容器内仍监听 `9042`。 |
| `OCG_ADMIN_USERNAME` + `OCG_ADMIN_PASSWORD` | 首次启动 | 可选管理员引导；必须同时设置或都不设置。 |
| `OCG_CLIENT_ROOT_URL` | 运行时 | 只读覆盖外部客户端根地址。 |
| `HTTP_PROXY` / `HTTPS_PROXY` / `ALL_PROXY` / `NO_PROXY` | 运行时 | “自动（系统 / 环境）”出站代理模式使用的标准代理变量。 |
| `OCG_MANAGER_ENCRYPTION_KEY` | 恢复时 | 原部署曾显式使用的混淆密钥。 |
| `NPM_REGISTRY` + `CARGO_REGISTRY` | 源码构建 | 仅 `--build` 使用的依赖注册表。 |

## 可选远程浏览器

默认 Gateway 部署不启动浏览器 Sidecar。要在 Linux 服务器或 Docker 上使用托管注册与官网登录，建议宿主机至少预留 2 CPU、2 GiB 内存和 1 GiB `/dev/shm`，然后执行：

```bash
docker compose --profile browser up -d
docker compose ps
```

`OCG_BROWSER_IMAGE` 可覆盖默认的 `ghcr.io/klarkxy/opencode-go-mgr-browser:<version>`。Sidecar 运行普通 Chromium、 Xvfb、轻量窗口管理器、x11vnc 与 noVNC；浏览画面会在 Dashboard 的独立完整标签页中通过同源 WebSocket 显示，键鼠输入也走该连接。远程剪贴板使用页面上明确的剪贴板区域复制或粘贴 Key。如果前面有反向代理，它必须支持 WebSocket 升级。 Sidecar 启动 Chromium 时使用 basic 密码存储，持久化 Profile 不依赖宿主机密钥环。

每个节点同一时刻只允许一个远程 Chromium。切换账号时会先正常关闭当前 Chromium、等待 Profile 写盘，再启动目标账号；之前打开的远程页面立即失效。Dashboard 浏览会话令牌只在主服务内存中保存，绑定当前管理员会话并校验 Origin；空闲 30 分钟或创建满 4 小时后失效。重新打开账号页面即可取得新会话。

Sidecar 不发布宿主机端口，也不挂载数据库。控制端口和 noVNC 只在 Compose 的 `browser-private` 项目私网内可见。该桥接网络不能设为 Docker `internal`，因为 Chromium 需要访问 Google/OpenCode 的 HTTPS 出站网络；Sidecar 的两个端点仍不会发布到宿主机。随机控制令牌存放在共享的 `ocg-browser-runtime` 运行时卷。账号 Cookie/Profile 则持久化在 `ocg-browser-profiles`；运行时卷不属于备份，`ocg-data` 与 `ocg-browser-profiles` 才是必须成对停止并备份的两个敏感卷。

Google 可能把数据中心出口 IP 视为高风险，要求额外验证，甚至拒绝注册或登录。 OCG Manager 不绕过这类风控；遇到时由用户完成 Google 要求的验证，或改用桌面端住宅网络完成注册。真实付款始终由用户在官网明确执行。

## 管理员引导

`OCG_ADMIN_USERNAME` 与 `OCG_ADMIN_PASSWORD` **只在数据库里还没有管理员时** 生效。

- 两个变量必须同时设置；只设一个会启动报错。
- 已有管理员后，后续修改环境变量不会再覆盖。
- 都不设置时，由首位访客在面板里创建管理员。
- 管理员创建后，只要保留卷，就可以移除这两个变量，数据库里的账号仍然有效。执行 `docker compose up -d --no-build --force-recreate` 把它们从容器环境中移除。

拥有 Docker daemon 权限的人可以看到容器环境变量；请保护 `.env`、使用长随机密码，并避免把未初始化的面板直接暴露到公网。

## 密钥与地址

`OCG_MANAGER_ENCRYPTION_KEY` 是高级恢复覆盖项。正常部署请留空，让生成的 `.encryption-key` 留在数据卷中。原部署如果显式使用了该变量，恢复时必须使用同一值；修改或丢失会导致已保存凭据无法读取。请把它当作密码保管。

可选的 `OCG_CLIENT_ROOT_URL` 等同于面板里的“下游访问根地址”，适合在反向代理或 Dashboard 与 Gateway 使用不同外部地址时显式指定客户端根地址。非空值必须是绝对 HTTP(S) URL；设置后优先于 SQLite 中的手工值，非法值会让进程启动失败。它不配置监听、DNS 或反向代理。一般填写 `https://ocg.example.com`，不需要填写 `/dashboard/` 或具体 API 端点；末尾 `/v1` 可省略或保留。

## 运行时行为

在 `.env` 中设置 `OCG_PORT` 可修改宿主机端口，容器内仍固定使用 `9042`。打开 `http://127.0.0.1:<OCG_PORT>/dashboard/` 并登录。请访问 `/dashboard/`，服务根路径 `/` 不是面板地址。

- 数据与生成的 `.encryption-key` 混淆密钥持久化在 `ocg-data` 卷中；账号浏览器 Cookie/Profile 持久化在独立的 `ocg-browser-profiles` 卷中。
- 容器进程监听 `0.0.0.0`，因此即使只发布到宿主机 `127.0.0.1`，管理面板也必须使用管理员登录；宿主机端口映射只限制可达范围，不会启用回环免登录。
- 容器的 `HEALTHCHECK` 每 30 秒对容器内 `127.0.0.1:9042` 做 TCP 探活，不存在 `/healthz` 路由。这个 TCP 检查只说明进程正在监听，不能证明面板 API、上游账号或真实模型请求可用。
- 两个镜像都以非特权 `ocg` 用户（UID/GID 10001）运行。随附 Compose 把根文件系统设为只读、把 `/tmp` 挂成 tmpfs，并丢弃全部 Linux capability。主服务另外启用 `no-new-privileges`；browser 服务改用 `seccomp=unconfined`，以便普通 Chromium 建立自身的 namespace 和 renderer seccomp 沙箱。Sidecar 不使用 `--no-sandbox`，另有 1 GiB 共享内存；命名卷 `ocg-data` 与 `ocg-browser-profiles` 是两类持久化应用状态。
- 启动日志会打印 Key，因此日志输出和 Docker daemon 权限都属于敏感信息。如果 Docker 主机默认没有限制日志大小，请由部署方配置日志轮转。

常用检查命令：

```bash
docker compose config --quiet
docker compose ps
docker compose logs --tail=100 -f ocg-manager
docker compose --profile browser logs --tail=100 -f browser
curl --fail http://127.0.0.1:9042/dashboard/
```

如果修改过 `OCG_PORT`，请把 curl 命令里的 `9042` 替换成实际宿主机端口。

## 校验镜像

主镜像与浏览器镜像都带 SPDX SBOM、BuildKit SLSA provenance 与 GitHub 签名的 provenance attestation。可这样检查发布版本：

```bash
docker buildx imagetools inspect ghcr.io/klarkxy/opencode-go-mgr:1.8.2
docker buildx imagetools inspect ghcr.io/klarkxy/opencode-go-mgr-browser:1.8.2
gh attestation verify \
  oci://ghcr.io/klarkxy/opencode-go-mgr:1.8.2 \
  --repo klarkxy/opencode-go-mgr
gh attestation verify \
  oci://ghcr.io/klarkxy/opencode-go-mgr-browser:1.8.2 \
  --repo klarkxy/opencode-go-mgr
```

两条 `gh attestation verify` 命令都要求 GitHub CLI 已登录。公开镜像可匿名拉取；如果 OCI 客户端仍要求 registry 凭据，请用具备 package 读取权限的 token 登录 `ghcr.io`。Provenance 证明产物如何构建，不等于漏洞扫描。

如果 Key 泄露，请重新生成。

## HTTPS

需要 HTTPS 时，把现有反向代理指向该回环端口即可，例如 Caddy：

```caddyfile
ocg.example.com {
    reverse_proxy 127.0.0.1:9042
}
```

登录后先在面板里设置一个非空的 Key，再发送 API 流量。用 `docker compose down` 停止服务；只有当你想彻底删除账号、凭据、Key、Cookie 与浏览器 Profile 时才追加 `-v`。

---

[用户指南索引](../USER.zh-CN.md) · [English](docker.md) · [文档索引](../README.zh-CN.md)
