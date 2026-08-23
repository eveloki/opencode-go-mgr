# Schema v27 (V3 access keys) — operator recovery

This note is only for recovering a data directory after a failed or interrupted
upgrade to schema v27. It is not a general backup policy and it is not a license
to run an older binary against a migrated database.

## What v27 changes

- Historical databases still migrate canonically through schema v26 first.
  Older `data.sqlite.pre-v22.<UTC>.bak` and `data.sqlite.pre-v23.<UTC>.bak`
  rollback copies, when present, stay valid for those earlier rewrites.
- After the database is at v26, an existing (non-empty) library gets a fresh
  unique sibling snapshot named `data.sqlite.pre-v3.<UTC>.bak` plus a SHA-256
  sidecar `data.sqlite.pre-v3.<UTC>.bak.sha256`. The backup file is flushed to
  disk and the sidecar is written through a unique temporary file, flushed,
  and atomically renamed (the containing directory is synced where the OS
  supports it) before any v27 write. A brand-new empty database does not
  create this copy.
- Primary `AppConfig.gateway_key` and `sub_gateway_keys` are copied into one
  `access_keys` table. The live primary row uses the fixed id
  `00000000-0000-0000-0000-000000000001`, stays enabled, and cannot be
  disabled or deleted. Sanitized config JSON is no longer the database
  authority for that value.
- The five leftover `accounts.usage_sync_*` columns are dropped. Official
  usage-sync metadata already lives in `provider_usage_sync_state`.
- Account `key_cipher` / `password_cipher` bytes are validated with the Host
  encryption cipher and are never re-encrypted.

## Before you restore

1. Stop every OCG Manager process (desktop, CLI, container) that has this
   data directory open. SQLite WAL files belong with `data.sqlite`.
2. Keep the matching encryption key: Windows machine-bound material, or
   `OCG_MANAGER_ENCRYPTION_KEY` / `.encryption-key` for CLI and Docker.
   A different cipher fails closed and must not be "fixed" by rewriting
   ciphertext.
3. Verify the pre-v3 sidecar before copying:

   ```text
   SHA-256(data.sqlite.pre-v3.<UTC>.bak) == first field of the .sha256 file
   ```

   If the hash does not match, do not restore that file.

## Restore the v26 rollback copy

Use this only when schema v27 never committed (open still reports v26, or
the v27 binary refuses to open and the source is still v26) or when you are
intentionally returning the directory to a v26-capable binary.

1. Stop the process.
2. Copy `data.sqlite.pre-v3.<UTC>.bak` over `data.sqlite` in the same
   directory. Remove leftover `data.sqlite-wal` / `data.sqlite-shm` if they
   do not belong to the restored file.
3. Start a **v26-capable** build, or retry the v27 upgrade. Do not point a
   v26 binary at a database that already reports schema 27.

The pre-v3 file is a v26 snapshot taken after historical migrations and
before any v27 write. It is not a v25/v22 backup; keep any older pre-v22 /
pre-v23 files if you still need those restore points.

## What a failed v27 open leaves behind

A failed v27 transaction rolls back. The source file must remain schema v26
with `sub_gateway_keys` intact and the five `accounts.usage_sync_*` columns
still present. The pre-v3 backup (and its hash sidecar) may already exist;
leave them in place. Do not delete them to "retry". A later successful open
creates another unique pre-v3 file rather than overwriting the first.

## Fresh data directories

A first launch on an empty directory creates schema v27 directly and does
not write `data.sqlite.pre-v3.*.bak`. There is nothing to restore except a
normal backup of the whole data directory.
