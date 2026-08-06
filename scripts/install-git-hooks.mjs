import { execFileSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

function git(args, opts = {}) {
  const out = execFileSync("git", args, {
    cwd: root,
    encoding: "utf8",
    stdio: opts.stdio ?? ["ignore", "pipe", "pipe"],
  });
  return String(out ?? "").trim();
}

try {
  git(["rev-parse", "--is-inside-work-tree"]);
} catch {
  // Not a git checkout (e.g. extracted source archive).
  process.exit(0);
}

const hooksPath = ".githooks";
let current = "";
try {
  current = git(["config", "--get", "core.hooksPath"]);
} catch {
  current = "";
}

if (current === hooksPath) {
  console.log(`git hooks already enabled (core.hooksPath=${hooksPath})`);
  process.exit(0);
}

execFileSync("git", ["config", "core.hooksPath", hooksPath], {
  cwd: root,
  stdio: "inherit",
});
console.log(`git hooks enabled: core.hooksPath=${hooksPath}`);
console.log("pre-commit will run cargo fmt --all when .rs files are staged");
