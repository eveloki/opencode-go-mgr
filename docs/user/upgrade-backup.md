[简体中文](upgrade-backup.zh-CN.md)

# Upgrade, Backup, Restore, And Uninstall

Download upgrades from the
[latest GitHub Release](https://github.com/klarkxy/opencode-go-mgr/releases/latest)
and verify them against `SHA256SUMS` from the same release:
`Get-FileHash <file> -Algorithm SHA256` on PowerShell, `shasum -a 256 <file>`
on macOS, or `sha256sum <file>` on Linux.

## Database Migration And Access Keys (Schema v27)

The current database schema is **v27**; historical databases migrate in
place on startup. Upgrading from a single-key version keeps your existing
credential as the **primary key** (fixed id
`00000000-0000-0000-0000-000000000001`), so clients keep authenticating with
the same value. Primary and additional (sub) Keys live together in one
`access_keys` table: at most 64 non-deleted sub keys, and deleting a sub key
is a soft delete that keeps the name for log attribution and clears the
plaintext.

An existing (non-empty) library migrates through schema v26 first, then
receives a unique sibling snapshot `data.sqlite.pre-v3.<timestamp>.bak` plus
a SHA-256 sidecar before any v27 write. A brand-new empty data directory
creates schema v27 directly and does not write that copy. The snapshot is a
v26 rollback point, not a substitute for a complete backup; verify the
sidecar before restoring it, and restore it only onto a v26-capable binary
or to retry a v27 open that never committed. Never open a migrated database
with an older build: extra Keys do not authenticate on a single-key-era
build, and a revoked value cannot come back to life by downgrading.

On every startup, enabled leftovers for Command Code GOAT and all three
SCNet Token Plan tiers are disabled without changing `updated_at`; Custom
API enabled state is preserved, and an existing unverified GOAT row is reset
to `pending`. OpenCode Go, Zen Free, and unknown provider/offering pairs are
untouched.

## Backup

1. Stop every process using the data: choose **Quit** from the desktop tray,
   stop the CLI with Ctrl+C or its service manager, or run
   `docker compose stop`.
2. Copy the **entire** GUI or CLI data directory. Desktop
   `browser-profiles/` is already inside the GUI data directory. For Docker,
   back up both sensitive volumes: `ocg-data` and `ocg-browser-profiles`.
   With the containers stopped, run
   `docker compose cp ocg-manager:/data/. ../ocg-data-backup` and
   `docker compose cp ocg-manager:/browser-profiles/. ../ocg-browser-profiles-backup`.
3. Keep the backup outside the repository, and check that it contains
   `data.sqlite` and, where present, `.encryption-key`. Browser profiles hold
   long-lived cookies and login state and are not encrypted by OCG Manager;
   protect them like account keys and the database.

## Restore

1. Stop the process, move the current data aside, and copy the whole backup
   back to its original directory or an empty Docker volume.
2. Start the same or a newer version.

Caveats:

- Docker files in `/data` must remain writable by UID/GID `10001`.
- Docker files in `/browser-profiles` must also remain writable by UID/GID
  `10001`.
- Windows GUI obfuscation is bound to the Windows user and machine, so its
  data cannot restore account keys or passwords on another machine — create
  fresh data there and re-enter the credentials.
- macOS/Linux GUI, CLI, and Docker restores must preserve `.encryption-key`
  or the explicitly supplied `--encryption-key` /
  `OCG_MANAGER_ENCRYPTION_KEY` value.
- There is no automatic downgrade compatibility guarantee; do not open a
  newer database with an older build.

## Docker Restore Into A Fresh Volume

First verify the backup and confirm that `.env` pins the intended same or
newer image. The `docker compose down -v` command below permanently deletes
all current named volumes; run it only after preserving both kinds of
persistent data separately:

```bash
docker compose down -v
docker compose run --rm --no-deps --user root \
  --cap-add CHOWN --cap-add DAC_OVERRIDE --cap-add FOWNER \
  --entrypoint sh \
  --volume ../ocg-data-backup:/backup/data:ro \
  --volume ../ocg-browser-profiles-backup:/backup/browser-profiles:ro \
  ocg-manager \
  -c 'cp -a /backup/data/. /data/ && \
      cp -a /backup/browser-profiles/. /browser-profiles/ && \
      chown -R 10001:10001 /data /browser-profiles && \
      find /data /browser-profiles -type d -exec chmod 700 {} + && \
      find /data /browser-profiles -type f -exec chmod 600 {} +'
docker compose --profile browser up -d --no-build
docker compose ps
```

If the original deployment used `OCG_MANAGER_ENCRYPTION_KEY`, put the same
secret back into `.env` before the restore. Keep the backup until the
dashboard, accounts, and a real gateway request have all been verified.

## Upgrade And Uninstall By Surface

The direct GUI steps are also the fallback when in-app update is unavailable.

- **Windows GUI:** quit the tray app, run the new installer, and choose
  **Install without uninstalling**. Uninstall from Windows **Installed
  apps**; the uninstaller asks whether to delete `%USERPROFILE%\.ocg-mgr`.
- **macOS GUI:** replace the app in **Applications** with the new DMG copy.
  Delete the app to uninstall; remove `~/.ocg-mgr` separately only when you
  also intend to delete the data.
- **Linux GUI:** install the new `.deb` over the old package, or replace the
  AppImage. Remove the package or AppImage to uninstall; data remains in
  `~/.ocg-mgr` until you delete it.
- **CLI:** replace the extracted package as a unit so the executable,
  `dist/`, and `LICENSE` stay together. Delete that package to uninstall;
  data remains in `~/.ocg-mgr-cli` or the custom `--data-dir`.
- **Docker:** after backing up, run `docker compose pull` followed by
  `docker compose up -d --no-build`. If the browser profile is enabled, use
  `docker compose --profile browser pull` followed by
  `docker compose --profile browser up -d --no-build` so both images are
  upgraded together. Pin `OCG_IMAGE` and `OCG_BROWSER_IMAGE` to full release tags
  for repeatable production deployments. `docker compose down` removes
  containers but keeps `ocg-data` and `ocg-browser-profiles`;
  `docker compose down -v` permanently deletes them and is only for an
  intentional reset after a verified two-volume backup. Selecting an older
  image does not roll back the database; restore
  the complete backup made by that older version when a database rollback is
  required.

---

[User guide index](../USER.md) · [简体中文](upgrade-backup.zh-CN.md) · [Docs index](../README.md)
