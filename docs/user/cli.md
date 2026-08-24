[简体中文](cli.zh-CN.md)

# CLI

Download the archive for your platform and extract it as a directory. It
contains the executable, `dist/`, and `LICENSE`. Keep `dist/` beside the
executable so `serve` can serve the dashboard. On Windows the executable is
`ocg-manager-cli.exe`; on Linux you may need `chmod +x ocg-manager-cli` after
extraction.

The CLI data directory defaults to `~/.ocg-mgr-cli` on every platform;
override it with `--data-dir <path>`. The obfuscation secret defaults to
`<data-dir>/.encryption-key`; override it with the named
`--encryption-key <key>` option or the `OCG_MANAGER_ENCRYPTION_KEY`
environment variable.

The CLI surface is `serve`, `key`, and `status` only. `key` manages OpenCode
Go account credentials, not dashboard Access Keys and not Custom or Zen Free
cards. Access Keys, Custom destinations, protocol switches, and catalogs stay
on the dashboard. CLI account writes bump that process's settings revision
in-process; they do not take `expectedRevision` on the command line.

```text
ocg-manager-cli
├── serve         Start the gateway server
│   --host        Address to listen on (default 127.0.0.1)
│   -p, --port    Gateway port (sets and saves config)
│   --dashboard-dir  Directory containing the built web dashboard
├── key list      List OpenCode Go API-key accounts (excludes Zen Free)
├── key add <name> <key>
│   --username    OpenCode-Go login account
│   --password    OpenCode-Go login password
├── key remove <id>      Remove an account
├── key enable <id>      Enable an account
├── key disable <id>     Disable an account
├── key ping [id]
│   --model       Model to send (default mimo-v2.5)
│   --message     User message (default "ping")
│   --max-tokens  max_tokens for the ping (default 3)
└── status        Show data dir, gateway port/key, upstream, account totals
```

The fastest way to bootstrap a headless gateway:

```bash
./ocg-manager-cli key add main sk-...
./ocg-manager-cli key list
./ocg-manager-cli serve --port 9042
```

`serve --port <port>` writes the new port to SQLite. Later `serve` runs
without `--port` reuse that saved value.

`key ping` reads the obfuscated key, sends a tiny chat completion, and prints
the real upstream status code and a short body excerpt — use it to surface
real `401`/`403`/`429`/`200` from each key without going through the
dashboard.

---

[User guide index](../USER.md) · [简体中文](cli.zh-CN.md) · [Docs index](../README.md)
