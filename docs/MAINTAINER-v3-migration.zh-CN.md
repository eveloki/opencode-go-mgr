[English](MAINTAINER-v3-migration.md)

# Schema v27（V3 接入 Key）— 运维手册

本手册是 schema v27 重写（V3 接入 Key）的运维约定，对应
`crates/ocg-core/src/db.rs` 中的 `CURRENT_SCHEMA_VERSION = 27`。请与
[MAINTAINER.zh-CN.md](MAINTAINER.zh-CN.md#升级与数据库迁移) 中的常规备份说明一起使用。

它不是通用备份策略，也不是用旧二进制打开已迁移数据库的许可。**本二进制没有向下迁移。**

## 目录

- [v27 改了什么](#v27-改了什么)
- [数据目录与密码器身份](#数据目录与密码器身份)
- [升级](#升级)
- [备份命名与哈希校验](#备份命名与哈希校验)
- [成功](#成功)
- [失败](#失败)
- [WAL 与 SHM](#wal-与-shm)
- [回滚](#回滚)
- [打开失败之后](#打开失败之后)
- [全新数据目录](#全新数据目录)
- [限制](#限制)

## v27 改了什么

- 历史数据库仍然**先按规范迁到 schema v26**（`migrate()`），然后再跑 v27 重写。已有的
  `data.sqlite.pre-v22.<timestamp>.bak` 与
  `data.sqlite.pre-v23.<timestamp>.bak` 回滚副本（若存在）对那些更早的重写仍然有效。
  加法式的 v24–v26 仍然不会单独生成 pre-v24 / pre-v25 / pre-v26 文件。
- 活动库到达 v26 之后，已有（非空）库会得到一份全新的**唯一同目录快照**
  `data.sqlite.pre-v3.<timestamp>.bak`，以及 SHA-256 旁路文件
  `data.sqlite.pre-v3.<timestamp>.bak.sha256`。该快照用 SQLite `VACUUM INTO`
  生成，发生在预检**之后**、任何 v27 写入**之前**。全新空库不会创建这份副本。
- 主 Key（`AppConfig.gateway_key`）以及 `sub_gateway_keys` 的每一行（含软删墓碑）
  都会复制进一张 `access_keys` 表，然后删除 `sub_gateway_keys`。活动主 Key 行使用固定 id
  `00000000-0000-0000-0000-000000000001`，名称为 `Primary`，保持启用，且不可禁用或删除。
  清洗后的配置 JSON 把 `gateway_key` 存成 `""`，不再是该值的数据库权威来源。
- 删除五列遗留的 `accounts.usage_sync_*`：
  `usage_sync_last_success_at`、`usage_sync_last_attempt_at`、
  `usage_sync_next_eligible_at`、`usage_sync_failure_streak`、
  `usage_sync_last_expedited_at`。官方用量同步元数据已经在
  `provider_usage_sync_state`。
- 账号的 `key_cipher` / `password_cipher` 字节会用 Host 加密密码器校验，并且**永不重新加密**。
  明文的接入 Key 值不做密码器探测。

## 数据目录与密码器身份

v27 打开路径始终使用 Host 已解析的密码器（CLI、桌面端、Docker 都走
`Database::open_with_cipher`）。密码器不一致会失败并保持关闭。不得靠改写密文字节来“修好”
不匹配。

| 入口 | 默认数据目录 | 密码器身份 |
| --- | --- | --- |
| Windows 桌面（Tauri） | `%USERPROFILE%\.ocg-mgr` | `MachineBoundCipher`，由 `USERNAME`、`COMPUTERNAME`、`APPDATA` 派生。数据目录不是密码器种子。这条路径没有 `.encryption-key`。 |
| macOS / Linux 桌面（Tauri） | `~/.ocg-mgr` | `StaticKeyCipher`，来自 `<data-dir>/.encryption-key`（首次启动时创建）。 |
| CLI | `~/.ocg-mgr-cli`，或 `--data-dir <path>` | 优先级（已测）：`--encryption-key` > `OCG_MANAGER_ENCRYPTION_KEY` > `<data-dir>/.encryption-key`。 |
| Docker | 容器 `--data-dir /data`（Compose 卷 `ocg-data`） | 与 CLI 相同的解析顺序。可选的 `OCG_MANAGER_ENCRYPTION_KEY` 是显式恢复覆盖；正常卷保留 `.encryption-key`。`/data` 中的文件必须继续允许 UID/GID `10001` 写入。 |

不要混用这些身份：

- Windows 桌面数据无法在另一个 Windows 用户或另一台机器上解密账号密文，也无法在 CLI/Docker 的静态密码器下解密。
- 把 GUI 目录复制到 CLI 默认路径（或反过来）会换目录；在 Windows 上还会换密码器。
- 若进程当时用 `--encryption-key` 或 `OCG_MANAGER_ENCRYPTION_KEY` 启动，只恢复 `.encryption-key` 不够，必须再次提供同一个显式秘密值。

## 升级

1. 停止每一个打开了该数据目录的 OCG Manager 进程（桌面托盘 **退出**、CLI 的 Ctrl+C / 服务停止、`docker compose stop`）。SQLite WAL 文件与 `data.sqlite` 同属一套。
2. 把**整个**数据目录（桌面：含已有的 `browser-profiles/`；CLI：`--data-dir` 树；Docker：`ocg-data` 与 `ocg-browser-profiles`）复制到活动目录之外。同时保留上表中对应的密码器材料。这份整目录副本才是运维备份；稍后的 pre-v3 同目录文件只是 v26 的 SQLite 快照。
3. 安装或解压具备 v27 能力的构建，用**同一**数据目录和密码器身份启动。迁移在 `open` 时原地执行。
4. 不要用具备 v26 能力的二进制去打开已经报告 schema 27 的目录。升级期间不要让两个写入方同时对着同一份 `data.sqlite`。

在**不**启动 OCG 二进制的情况下检查 schema（CLI 的 `status` / `serve` / 桌面启动都会尝试 v27）。先停进程，让 WAL 空闲：

```bash
sqlite3 data.sqlite "SELECT version FROM schema_version ORDER BY version DESC LIMIT 1;"
sqlite3 data.sqlite "SELECT name FROM sqlite_master WHERE type='table' AND name IN ('access_keys','sub_gateway_keys') ORDER BY name;"
```

## 备份命名与哈希校验

已有库的快照（全新空目录不会创建）：

```text
data.sqlite.pre-v3.<timestamp>.bak
data.sqlite.pre-v3.<timestamp>.bak.sha256
```

`<timestamp>` 是 UTC 的 `YYYYMMDDThhmmss`，加上 9 位小数秒和末尾 `Z`（chrono
`%Y%m%dT%H%M%S%9fZ`，25 个字符）。例如：
`data.sqlite.pre-v3.20260824T153045123456789Z.bak`。文件名唯一；写入方永不覆盖已有 `.bak`。
随后的重试，或 `VACUUM INTO` 期间的写入竞态，会再分配另一个唯一名（每次备份最多 8 次文件名尝试，整段 v27 预检/备份循环最多 8 次重试）。

快照是独立的 SQLite 文件。创建时实现会：

1. 对活动的 v26 源执行 `PRAGMA quick_check`。
2. 用 Host 密码器探测每一段非空的 `accounts.key_cipher` 与 `accounts.password_cipher`（不改写）。探测失败发生在这份快照**之前**。
3. `VACUUM INTO` 到唯一的 `.bak`（包含活动库里已提交到 WAL 的页）。
4. 以只读方式重新打开 `.bak`，要求 schema 为 **26**，并再次 `quick_check`。
5. `sync` `.bak`，流式计算 SHA-256，把 `{digest}  {backup-file-name}\n` 写进唯一的
   `*.sha256.<uuid>.tmp`，flush/sync 后原子改名为 `*.sha256`。父目录只在 Unix 上
   `sync`；Windows 仍会 sync 备份文件和旁路文件的内容。

旁路文件的第一个字段是 `.bak` 字节的小写十六进制 SHA-256。第二个字段只有基名（GNU
`sha256sum` 布局，两个空格）。

在任何恢复**之前**校验，并且要在数据目录内执行，以便基名能解析到文件：

```text
SHA-256(data.sqlite.pre-v3.<timestamp>.bak)
  == first whitespace-separated field of
     data.sqlite.pre-v3.<timestamp>.bak.sha256
```

公式与英文手册相同：第一个空白分隔字段是摘要。比较时不区分大小写。如果不匹配，不要恢复该文件。

Linux（在数据目录内）：

```bash
sha256sum -c data.sqlite.pre-v3.<timestamp>.bak.sha256
```

macOS：

```bash
shasum -a 256 -c data.sqlite.pre-v3.<timestamp>.bak.sha256
```

Windows PowerShell：

```powershell
$bak = ".\data.sqlite.pre-v3.<timestamp>.bak"
$actual = (Get-FileHash -Algorithm SHA256 $bak).Hash.ToLowerInvariant()
$expected = ((Get-Content -Raw "$bak.sha256") -split '\s+')[0].ToLowerInvariant()
if ($actual -ne $expected) { throw "hash mismatch; do not restore $bak" }
```

若存在多个 `data.sqlite.pre-v3.*.bak`，校验你打算恢复的那一份旁路文件。该文件还必须能以 schema 26 打开，并且仍含 `sub_gateway_keys`。产品里没有选择器。

## 成功

成功打开一份**已有**库之后：

- `schema_version` 为 `27`。
- 存在 `access_keys`；不存在 `sub_gateway_keys`。
- 上文列出的五列 `accounts.usage_sync_*` 已消失。
- 恰好一行活动主 Key：id 为
  `00000000-0000-0000-0000-000000000001`，已启用、未删除、值非空。
- 复制的子 Key 行数加上该主 Key 等于 `access_keys` 行数；账号行数不变。
- 设置 JSON 里的 `gateway_key` 为 `""`。
- 账号密文字节不变。
- 这一次尝试会留下一份新的带哈希 pre-v3 同目录文件（更早的唯一 pre-v3 / pre-v22 / pre-v23 文件原样保留）。
- 再次打开同一份已是 v27 的数据库**不会**再写一份 pre-v3 文件。

重写期间第二个并发打开方不是运维步骤。若备份与写入锁之间的 `PRAGMA data_version` 发生变化，实现会重试；测试表明至少一个打开方能完成，且活动主 Key 计数保持为 1。

## 失败

v27 预检与重写事务是分开的。用 sqlite3 解读活动的 `data.sqlite`（不要靠启动 OCG）：

| 你看到的 | 含义 | 怎么做 |
| --- | --- | --- |
| 打开失败；schema 仍是 26；**没有**新的 `pre-v3` 文件 | 失败发生在备份**之前**：损坏的 SQLite（`quick_check`）、存在非空账号密文却缺少 Host 密码器、Host 密码器错误，或损坏的 `key_cipher` / `password_cipher`。密文未被改写。 | 修正密码器身份，或恢复一份已知完好的整目录备份。不要改写密文。 |
| 打开失败；schema 仍是 26；`sub_gateway_keys` 完好；用量同步列仍在；已有 `pre-v3` 的 `.bak` + `.sha256` | 备份已完成；v27 **事务已回滚**（或从未提交）。源库必须仍可作为 v26 使用。 | 保留这些文件。重试 v27 二进制，或在回到具备 v26 能力的构建时恢复该已校验快照。 |
| 打开失败；schema 仍是 26；存在不止一份唯一的 `pre-v3` 文件 | 重试或 `VACUUM INTO` 写入竞态又分配了一份唯一快照。过期的竞态快照不会被覆盖。 | 校验你要恢复的那份快照的旁路文件。 |
| 打开失败；活动文件不是 SQLite / `quick_check` 失败 | 源库从未声称自己是 v27。 | 恢复整目录备份。不要使用不匹配的 `pre-v3` 文件。 |
| 打开失败；schema **新于 27** | 本构建拒绝它无法迁移的数据库。没有写入。 | 恢复匹配的数据目录**以及**加密密钥。 |
| 打开成功；schema 27 | 重写已提交。 | 见[成功](#成功)。回滚只能离线恢复一份 v26 快照。 |

已覆盖的错误文本包括 `newer than this build supports`、
`host encryption cipher` / `open_with_cipher`，以及会点名 `key_cipher` 的损坏账号密文。错误密码器会失败并保持关闭，且不会声称自己是 v27。

## WAL 与 SHM

每一次 `Database::open` 都在 `migrate()` **之前**设置 `PRAGMA journal_mode = WAL` 和
`synchronous = NORMAL`。因此活动库可能带有：

```text
data.sqlite
data.sqlite-wal
data.sqlite-shm
```

`VACUUM INTO` 写出的是**独立**快照，已经包含该活动库里已提交到 WAL 的行。`.bak` 不会附带同级的 `-wal` / `-shm` 文件。

把 `.bak` 复制到 `data.sqlite` 之上后，残留的 `data.sqlite-wal` 和 `data.sqlite-shm` 属于**之前的活动文件**，不属于这份快照。把它们删掉。留着它们不是受支持的恢复方式。

仍在运行的进程里、尚未提交的脏 WAL 内容不在快照中。升级或恢复前先停进程。

## 回滚

**没有向下迁移。** 二进制永远不会把 schema 27 转回 26，永远不会从 `access_keys` 重建
`sub_gateway_keys`，也永远不会恢复已删除的用量同步列。测试专用的反向辅助不是产品命令。

回滚是把已校验的 `data.sqlite.pre-v3.<timestamp>.bak`（一份 v26 快照）**离线原样覆盖**到
`data.sqlite`，使用**同一套 Host 密码器身份**，然后启动具备 **v26 能力**的构建——或者在这份已恢复的 v26 文件上重试 v27 升级。

仅在 schema v27 从未提交，或你有意把目录交回具备 v26 能力的二进制时使用。在 v27 **已经成功**打开之后再恢复，会丢弃该快照之后的全部写入。

1. 停止每一个打开了该目录的进程。
2. 按上文校验旁路哈希。不匹配就停止。
3. 把该 `.bak` 复制到同一目录的 `data.sqlite` 上：

   ```bash
   cp data.sqlite.pre-v3.<timestamp>.bak data.sqlite
   rm -f data.sqlite-wal data.sqlite-shm
   ```

   ```powershell
   Copy-Item -Force .\data.sqlite.pre-v3.<timestamp>.bak .\data.sqlite
   Remove-Item -ErrorAction SilentlyContinue .\data.sqlite-wal, .\data.sqlite-shm
   ```

4. 确认 sqlite3 仍报告 schema 26，且存在 `sub_gateway_keys`。
5. 用匹配的密码器启动具备 **v26 能力**的构建，或重试 v27 二进制。不要把 v26 二进制指向已经报告 schema 27 的数据库。

pre-v3 文件是历史迁移完成之后、任何 v27 写入之前拍下的 v26 快照。它不是 v25/v22 备份，不是 `.encryption-key`，也不是浏览器 Profile。若仍需要那些恢复点，请保留更早的 pre-v22 / pre-v23 文件。

## 打开失败之后

失败的 v27 事务会回滚。源文件必须仍为 schema v26，`sub_gateway_keys` 完好，五列
`accounts.usage_sync_*` 仍在。pre-v3 备份（及其哈希旁路）可能已经存在；留在原地。不要为了“重试”删掉它们。对仍是 v26 的源稍后一次成功打开会再创建一份唯一的 pre-v3 文件，而不是覆盖第一份。

重试方式：同一目录、同一 Host 密码器、具备 v27 能力的二进制。失败打开与重试之间不要更换密码器材料。

## 全新数据目录

在空目录上的首次启动会直接创建 schema v27，并且不写 `data.sqlite.pre-v3.*.bak`。除了整份数据目录的常规备份之外，没有可恢复的 pre-v3 副本。

## 限制

- 没有向下迁移，没有原地还原，没有能用具备 v26 能力的二进制打开 schema 27 的命令。
- pre-v3 同目录文件不能替代整目录备份（密码器文件 / 显式秘密、浏览器 Profile、Docker 卷所有权）。
- 哈希不匹配、`quick_check` 失败，或 `.bak` 的 schema ≠ 26，意味着不要恢复该文件。产品不会修复它。
- Host 密码器错误或缺失会失败并保持关闭。不要改写 `key_cipher` / `password_cipher`。
- Windows 桌面密码器绑定用户与机器。把该目录移到另一个 Windows 账户、另一台机器，或 CLI/Docker 静态密码器下，都无法解密已有账号密文。
- 旁路文件的父目录 `sync` 仅 Unix。Windows 只 sync `.bak` 与 `.sha256` 的文件内容。
- 重试或 `VACUUM INTO` 竞态后，额外的唯一 pre-v3 文件可能留在磁盘上。产品不会删除它们。
- Docker 恢复必须让 `/data` 保持 UID/GID `10001`。
- `ocg-manager-cli status` 会打开数据库并尝试 v27。它不是只读的 schema 检查器。
)
