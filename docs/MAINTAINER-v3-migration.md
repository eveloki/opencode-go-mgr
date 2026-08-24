[简体中文](MAINTAINER-v3-migration.zh-CN.md)

# Schema v27 (V3 access keys) — operator runbook

This runbook is the operator contract for the schema v27 rewrite (V3 access
keys). It matches `CURRENT_SCHEMA_VERSION = 27` in
`crates/ocg-core/src/db.rs`. Use it with the general backup advice in
[MAINTAINER.md](MAINTAINER.md#upgrades-and-database-migrations).

It is not a general backup policy. It is not a license to open a migrated
database with an older binary. **This binary has no down-migration.**

## Table of contents

- [What v27 changes](#what-v27-changes)
- [Data directories and cipher identity](#data-directories-and-cipher-identity)
- [Upgrade](#upgrade)
- [Backup naming and hash verification](#backup-naming-and-hash-verification)
- [Success](#success)
- [Failure](#failure)
- [WAL and SHM](#wal-and-shm)
- [Rollback](#rollback)
- [Failed open](#failed-open)
- [Fresh data directories](#fresh-data-directories)
- [Limitations](#limitations)

## What v27 changes

- Historical databases still migrate **canonically through schema v26** first
  (`migrate()`), then the v27 rewrite runs. Older
  `data.sqlite.pre-v22.<timestamp>.bak` and
  `data.sqlite.pre-v23.<timestamp>.bak` rollback copies, when present, stay
  valid for those earlier rewrites. Additive v24–v26 still do not create their
  own pre-v24 / pre-v25 / pre-v26 files.
- After the live file is at v26, an existing (non-empty) library gets a fresh
  **unique sibling snapshot** named `data.sqlite.pre-v3.<timestamp>.bak` plus
  a SHA-256 sidecar `data.sqlite.pre-v3.<timestamp>.bak.sha256`. That snapshot
  is taken with SQLite `VACUUM INTO` **after** preflight and **before** any
  v27 write. A brand-new empty database does not create this copy.
- Primary `AppConfig.gateway_key` and every `sub_gateway_keys` row (including
  soft-deleted tombstones) are copied into one `access_keys` table, then
  `sub_gateway_keys` is dropped. The live primary row uses the fixed id
  `00000000-0000-0000-0000-000000000001`, name `Primary`, stays enabled, and
  cannot be disabled or deleted. Sanitized config JSON stores `gateway_key` as
  `""` and is no longer the database authority for that value.
- The five leftover `accounts.usage_sync_*` columns are dropped:
  `usage_sync_last_success_at`, `usage_sync_last_attempt_at`,
  `usage_sync_next_eligible_at`, `usage_sync_failure_streak`,
  `usage_sync_last_expedited_at`. Official usage-sync metadata already lives
  in `provider_usage_sync_state`.
- Account `key_cipher` / `password_cipher` bytes are validated with the Host
  encryption cipher and are **never re-encrypted**. Plaintext access-key
  values are not cipher-probed.

## Data directories and cipher identity

v27 open always uses the Host-resolved cipher (`Database::open_with_cipher`
on CLI, desktop, and Docker). A different cipher fails closed. Ciphertext
bytes must not be rewritten to “fix” a mismatch.

| Surface | Default data directory | Cipher identity |
| --- | --- | --- |
| Windows desktop (Tauri) | `%USERPROFILE%\.ocg-mgr` | `MachineBoundCipher` from `USERNAME`, `COMPUTERNAME`, and `APPDATA`. The data directory is not used as the cipher seed. There is no `.encryption-key` on this path. |
| macOS / Linux desktop (Tauri) | `~/.ocg-mgr` | `StaticKeyCipher` from `<data-dir>/.encryption-key` (created on first launch). |
| CLI | `~/.ocg-mgr-cli`, or `--data-dir <path>` | Priority (tested): `--encryption-key` > `OCG_MANAGER_ENCRYPTION_KEY` > `<data-dir>/.encryption-key`. |
| Docker | container `--data-dir /data` (Compose volume `ocg-data`) | Same CLI resolution. Optional `OCG_MANAGER_ENCRYPTION_KEY` is an explicit restore override; a normal volume keeps `.encryption-key`. Files in `/data` must stay writable by UID/GID `10001`. |

Do not mix these identities:

- Windows desktop data cannot decrypt account ciphertext on another Windows
  user or machine, and cannot decrypt under the CLI/Docker static cipher.
- Copying a GUI directory onto the CLI default path (or the reverse) uses a
  different directory **and**, on Windows, a different cipher.
- If the process was started with `--encryption-key` or
  `OCG_MANAGER_ENCRYPTION_KEY`, restoring only `.encryption-key` is not
  enough; supply the same explicit secret again.

## Upgrade

1. Stop every OCG Manager process (desktop tray **Quit**, CLI Ctrl+C / service
   stop, `docker compose stop`) that has this data directory open. SQLite WAL
   files belong with `data.sqlite`.
2. Copy the **entire** data directory (desktop: include `browser-profiles/`
   when present; CLI: the `--data-dir` tree; Docker: `ocg-data` and
   `ocg-browser-profiles`) to a location outside the live directory. Keep the
   matching cipher material listed above. This whole-directory copy is the
   operator backup; the later pre-v3 sibling is only a v26 SQLite snapshot.
3. Install or unpack the v27-capable build. Start it against the **same**
   data directory and cipher identity. Migration runs in place on `open`.
4. Do not start a v26-capable binary against a directory that already reports
   schema 27. Do not start two writers against the same `data.sqlite` during
   the upgrade.

Inspect schema **without** starting an OCG binary (a CLI `status` / `serve` /
desktop launch will attempt v27). Stop the process first so WAL is idle:

```bash
sqlite3 data.sqlite "SELECT version FROM schema_version ORDER BY version DESC LIMIT 1;"
sqlite3 data.sqlite "SELECT name FROM sqlite_master WHERE type='table' AND name IN ('access_keys','sub_gateway_keys') ORDER BY name;"
```

## Backup naming and hash verification

Existing-library snapshot (not created for a fresh empty directory):

```text
data.sqlite.pre-v3.<timestamp>.bak
data.sqlite.pre-v3.<timestamp>.bak.sha256
```

`<timestamp>` is UTC `YYYYMMDDThhmmss` plus 9 fractional-second digits and a
trailing `Z` (chrono `%Y%m%dT%H%M%S%9fZ`, 25 characters). Example:
`data.sqlite.pre-v3.20260824T153045123456789Z.bak`. Names are unique; the
writer never overwrites an existing `.bak`. A later retry or a writer race
during `VACUUM INTO` allocates another unique name (up to 8 filename attempts
per backup, and up to 8 retries of the whole v27 preflight/backup loop).

The snapshot is a standalone SQLite file. At creation the implementation:

1. Runs `PRAGMA quick_check` on the live v26 source.
2. Probes every non-empty `accounts.key_cipher` and `accounts.password_cipher`
   with the Host cipher (no rewrite). Probe failure happens **before** this
   snapshot.
3. `VACUUM INTO` the unique `.bak` (includes WAL-committed pages from the
   live library).
4. Re-opens the `.bak` read-only, requires schema **26**, and runs
   `quick_check` again.
5. `sync`s the `.bak`, streams SHA-256, writes
   `{digest}  {backup-file-name}\n` through a unique
   `*.sha256.<uuid>.tmp`, flushes/syncs, then atomically renames to
   `*.sha256`. The parent directory is `sync`ed on Unix only; Windows still
   syncs the backup and sidecar file contents.

The sidecar first field is a lowercase hex SHA-256 of the `.bak` bytes. The
second field is the basename only (GNU `sha256sum` layout, two spaces).

Verify **before** any restore, from the data directory so the basename
resolves:

```text
SHA-256(data.sqlite.pre-v3.<timestamp>.bak)
  == first whitespace-separated field of
     data.sqlite.pre-v3.<timestamp>.bak.sha256
```

Compare the digest case-insensitively. If it does not match, do not restore
that file.

Linux (data directory):

```bash
sha256sum -c data.sqlite.pre-v3.<timestamp>.bak.sha256
```

macOS:

```bash
shasum -a 256 -c data.sqlite.pre-v3.<timestamp>.bak.sha256
```

Windows PowerShell:

```powershell
$bak = ".\data.sqlite.pre-v3.<timestamp>.bak"
$actual = (Get-FileHash -Algorithm SHA256 $bak).Hash.ToLowerInvariant()
$expected = ((Get-Content -Raw "$bak.sha256") -split '\s+')[0].ToLowerInvariant()
if ($actual -ne $expected) { throw "hash mismatch; do not restore $bak" }
```

If several `data.sqlite.pre-v3.*.bak` files exist, verify the sidecar of the
file you intend to restore. That file must also open as schema 26 and still
contain `sub_gateway_keys`. There is no picker in the product.

## Success

After a successful open of an **existing** library:

- `schema_version` is `27`.
- `access_keys` exists; `sub_gateway_keys` does not.
- The five `accounts.usage_sync_*` columns listed above are gone.
- Exactly one live primary row: id
  `00000000-0000-0000-0000-000000000001`, enabled, not deleted, non-empty.
- Copied sub-key count plus that primary equals `access_keys` row count;
  account row count is unchanged.
- Settings JSON `gateway_key` is `""`.
- Account ciphertext bytes are unchanged.
- One new hashed pre-v3 sibling exists from this attempt (older unique
  pre-v3 / pre-v22 / pre-v23 files are left in place).
- Re-opening the same v27 database does **not** write another pre-v3 file.

A concurrent second opener during the rewrite is not an operator procedure.
The implementation retries when `PRAGMA data_version` changes between backup
and the writer lock; tests show at least one opener finishes and the live
primary count stays 1.

## Failure

v27 preflight and the rewrite transaction are separate. Interpret the live
`data.sqlite` with sqlite3 (not by launching OCG):

| What you see | Meaning | What to do |
| --- | --- | --- |
| Open fails; schema still 26; **no** new `pre-v3` file | Failed **before** backup: corrupt SQLite (`quick_check`), missing Host cipher with non-empty account ciphertext, wrong Host cipher, or corrupt `key_cipher` / `password_cipher`. Ciphertext was not rewritten. | Fix the cipher identity or restore a known-good whole-directory backup. Do not rewrite ciphertext. |
| Open fails; schema still 26; `sub_gateway_keys` intact; usage-sync columns still present; a `pre-v3` `.bak` + `.sha256` exist | Backup completed; the v27 **transaction rolled back** (or never committed). Source must remain usable v26. | Leave those files in place. Retry the v27 binary, or restore that verified snapshot if you are returning to a v26-capable build. |
| Open fails; schema still 26; more than one unique `pre-v3` file | A retry or a `VACUUM INTO` writer race allocated another unique snapshot. Stale raced snapshots are not overwritten. | Verify the sidecar of the snapshot you restore. |
| Open fails; live file is not SQLite / `quick_check` fails | Source never claimed v27. | Restore a whole-directory backup. Do not use a mismatched `pre-v3` file. |
| Open fails; schema **newer than 27** | This build refuses databases it cannot migrate. No writes. | Restore a matching data directory **and** encryption key. |
| Open succeeds; schema 27 | Rewrite committed. | See [Success](#success). Rollback is only an offline restore of a v26 snapshot. |

Error text that is tested includes `newer than this build supports`,
`host encryption cipher` / `open_with_cipher`, and a corrupt account cipher
that names `key_cipher`. A wrong cipher fails closed without claiming v27.

## WAL and SHM

Every `Database::open` sets `PRAGMA journal_mode = WAL` and
`synchronous = NORMAL` **before** `migrate()`. A live library may therefore
have:

```text
data.sqlite
data.sqlite-wal
data.sqlite-shm
```

`VACUUM INTO` writes a **standalone** snapshot that already includes
WAL-committed rows from that live library. The `.bak` does not ship sibling
`-wal` / `-shm` files.

After you copy a `.bak` over `data.sqlite`, leftover `data.sqlite-wal` and
`data.sqlite-shm` belong to the **previous live file**, not to the snapshot.
Remove them. Leaving them in place is not a supported restore.

Uncommitted work in a dirty WAL of a still-running process is not in the
snapshot. Stop the process before upgrade or restore.

## Rollback

**There is no down-migration.** The binary never converts schema 27 back to
26, never recreates `sub_gateway_keys` from `access_keys`, and never restores
the dropped usage-sync columns. Test-only reverse helpers are not a product
command.

Rollback is **offline exact-file restoration** of a verified
`data.sqlite.pre-v3.<timestamp>.bak` (a v26 snapshot) onto `data.sqlite`,
with the **same Host cipher identity**, then starting a **v26-capable**
build — or retrying the v27 upgrade on that restored v26 file.

Use this when schema v27 never committed, or when you are intentionally
returning the directory to a v26-capable binary. Restoring after a
**successful** v27 open discards every write that happened after that
snapshot.

1. Stop every process that has the directory open.
2. Verify the sidecar hash as above. If it does not match, stop.
3. Copy that `.bak` over `data.sqlite` in the same directory:

   ```bash
   cp data.sqlite.pre-v3.<timestamp>.bak data.sqlite
   rm -f data.sqlite-wal data.sqlite-shm
   ```

   ```powershell
   Copy-Item -Force .\data.sqlite.pre-v3.<timestamp>.bak .\data.sqlite
   Remove-Item -ErrorAction SilentlyContinue .\data.sqlite-wal, .\data.sqlite-shm
   ```

4. Confirm sqlite3 still reports schema 26 and `sub_gateway_keys` exists.
5. Start a **v26-capable** build with the matching cipher, or retry the v27
   binary. Do not point a v26 binary at a database that already reports
   schema 27.

The pre-v3 file is a v26 snapshot taken after historical migrations and
before any v27 write. It is not a v25/v22 backup, not `.encryption-key`, and
not browser profiles. Keep any older pre-v22 / pre-v23 files if you still
need those restore points.

## Failed open

A failed v27 transaction rolls back. The source file must remain schema v26
with `sub_gateway_keys` intact and the five `accounts.usage_sync_*` columns
still present. The pre-v3 backup (and its hash sidecar) may already exist;
leave them in place. Do not delete them to “retry”. A later successful open
of a still-v26 source creates another unique pre-v3 file rather than
overwriting the first.

Retry is: same directory, same Host cipher, v27-capable binary. Do not change
cipher material between the failed open and the retry.

## Fresh data directories

A first launch on an empty directory creates schema v27 directly and does
not write `data.sqlite.pre-v3.*.bak`. There is nothing to restore except a
normal backup of the whole data directory.

## Limitations

- No down-migration, no in-place revert, no command that opens schema 27 with
  a v26-capable binary.
- The pre-v3 sibling is not a substitute for a whole-directory backup
  (cipher file / explicit secret, browser profiles, Docker volume ownership).
- Hash mismatch, `quick_check` failure, or schema ≠ 26 on a `.bak` means do
  not restore that file. The product does not repair it.
- Wrong or missing Host cipher fails closed. Do not rewrite
  `key_cipher` / `password_cipher`.
- Windows desktop cipher is machine/user bound. Moving that directory to
  another Windows account, another machine, or the CLI/Docker static cipher
  cannot decrypt existing account ciphertext.
- Parent-directory `sync` of the sidecar is Unix-only. Windows syncs file
  contents of the `.bak` and `.sha256` only.
- Extra unique pre-v3 files can remain after retries or a `VACUUM INTO` race.
  The product does not delete them.
- Docker restores must keep UID/GID `10001` on `/data`.
- `ocg-manager-cli status` opens the database and will attempt v27. It is not
  a read-only schema inspector.
)
