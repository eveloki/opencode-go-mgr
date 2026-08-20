import assert from "node:assert/strict";
import { appendFileSync, chmodSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";
import { fileURLToPath } from "node:url";

const repositoryRoot = fileURLToPath(new URL("../", import.meta.url));
const helperPath = fileURLToPath(new URL("./container-publish.sh", import.meta.url));
const bashPath = process.platform === "win32"
  ? join(process.env.ProgramFiles ?? "C:\\Program Files", "Git", "bin", "bash.exe")
  : "bash";

const digests = {
  browser: `sha256:${"b".repeat(64)}`,
  browserAmd64: `sha256:${"c".repeat(64)}`,
  browserArm64: `sha256:${"d".repeat(64)}`,
  main: `sha256:${"a".repeat(64)}`,
  mainAmd64: `sha256:${"e".repeat(64)}`,
  mainArm64: `sha256:${"f".repeat(64)}`,
};

function toBashPath(path) {
  if (process.platform !== "win32") return path;
  const normalized = path.replaceAll("\\", "/");
  return normalized.replace(/^([A-Za-z]):/, (_, drive) => `/${drive.toLowerCase()}`);
}

function writeExecutable(path, source) {
  writeFileSync(path, source, "utf8");
  chmodSync(path, 0o755);
}

const fakeDocker = String.raw`#!/usr/bin/env bash
set -euo pipefail

printf '%s|%s\n' "\${3:-docker}" "$*" >>"$CALL_LOG"

raw_for_image() {
  local image=$1
  local amd64_digest
  local arm64_digest
  local test_image
  if [[ "$image" == "$BROWSER_IMAGE" ]]; then
    amd64_digest=$BROWSER_DIGEST_AMD64
    arm64_digest=$BROWSER_DIGEST_ARM64
    test_image=browser
  else
    amd64_digest=$MAIN_DIGEST_AMD64
    arm64_digest=$MAIN_DIGEST_ARM64
    test_image=main
  fi
  printf '{"schemaVersion":2,"mediaType":"application/vnd.oci.image.index.v1+json","manifests":[{"digest":"%s","platform":{"os":"linux","architecture":"amd64"}},{"digest":"%s","platform":{"os":"linux","architecture":"arm64"}}],"annotations":{"org.opencontainers.image.version":"%s","org.opencontainers.image.revision":"%s"},"testImage":"%s"}' \
    "$amd64_digest" "$arm64_digest" "$CANDIDATE_VERSION" "$FULL_SHA" "$test_image"
}

state_file_for() {
  local ref=$1
  local key
  key=$(printf '%s' "$ref" | tr '/:@' '____')
  printf '%s/%s' "$FAKE_STATE_DIR" "$key"
}

if [[ "\${1:-}" != buildx || "\${2:-}" != imagetools ]]; then
  echo "unexpected docker command: $*" >&2
  exit 90
fi

case "\${3:-}" in
  inspect)
    ref=$4
    state_file=$(state_file_for "$ref")
    if [[ ! -f "$state_file" ]]; then
      echo "manifest unknown: $ref" >&2
      exit 1
    fi
    digest=$(<"$state_file")
    if [[ "$*" == *"--raw"* ]]; then
      if [[ "$ref" == "$BROWSER_IMAGE"* ]]; then
        raw_for_image "$BROWSER_IMAGE"
      else
        raw_for_image "$MAIN_IMAGE"
      fi
    elif [[ "$*" == *".Image.Config.Labels"* ]]; then
      printf '{"org.opencontainers.image.version":"%s"}' "$CANDIDATE_VERSION"
    else
      printf '{"digest":"%s"}' "$digest"
    fi
    ;;
  create)
    if [[ "\${FAIL_DOCKER_DRY_RUN:-0}" == 1 && "$*" == *"--dry-run"* ]]; then
      echo "forced docker dry-run failure" >&2
      exit 71
    fi
    image=$MAIN_IMAGE
    if [[ "$*" == *"$BROWSER_IMAGE@"* || "$*" == *"--tag $BROWSER_IMAGE:"* ]]; then
      image=$BROWSER_IMAGE
      digest=$BROWSER_CANDIDATE_DIGEST
    else
      digest=$MAIN_CANDIDATE_DIGEST
    fi
    if [[ "$*" == *"--dry-run"* ]]; then
      raw_for_image "$image"
      exit 0
    fi
    tag=
    previous=
    for argument in "$@"; do
      if [[ "$previous" == --tag ]]; then
        tag=$argument
        break
      fi
      previous=$argument
    done
    if [[ -z "$tag" ]]; then
      echo "create call did not include --tag: $*" >&2
      exit 72
    fi
    printf '%s' "$digest" >"$(state_file_for "$tag")"
    ;;
  *)
    echo "unexpected imagetools command: $*" >&2
    exit 91
    ;;
esac
`.replaceAll("\\${", "${");

const fakeJq = String.raw`#!/usr/bin/env bash
set -euo pipefail
input=$(cat)
if [[ "\${FAIL_JQ:-0}" == 1 ]]; then
  echo "forced jq failure" >&2
  exit 73
fi
arguments="$*"
if [[ "$arguments" == *"manifests[]?"* && "$arguments" == *"sort | join"* ]]; then
  if [[ "$input" == *'"testImage":"browser"'* ]]; then
    printf '%s\n%s\n' "$BROWSER_DIGEST_AMD64" "$BROWSER_DIGEST_ARM64" | LC_ALL=C sort | paste -sd, -
  else
    printf '%s\n%s\n' "$MAIN_DIGEST_AMD64" "$MAIN_DIGEST_ARM64" | LC_ALL=C sort | paste -sd, -
  fi
elif [[ "$arguments" == *"org.opencontainers.image.revision"* ]]; then
  printf '%s\n' "$FULL_SHA"
elif [[ "$arguments" == *"org.opencontainers.image.version"* ]]; then
  printf '%s\n' "$CANDIDATE_VERSION"
elif [[ "$arguments" == *"mediaType // empty"* ]]; then
  printf '%s\n' 'application/vnd.oci.image.index.v1+json'
elif [[ "$arguments" == *".mainAdvance"* || "$arguments" == *".browserAdvance"* ]]; then
  printf '%s\n' true
elif [[ "$arguments" == *".version"* ]]; then
  printf '%s\n' "$CANDIDATE_VERSION"
elif [[ "$arguments" == *".digest"* ]]; then
  printf '%s\n' "$input" | sed -n 's/.*"digest":"\([^"]*\)".*/\1/p'
elif [[ "$arguments" == *"-e ."* ]]; then
  [[ -n "$input" ]]
else
  echo "unexpected jq invocation: $arguments" >&2
  exit 74
fi
`.replaceAll("\\${", "${");

const fakeNode = String.raw`#!/usr/bin/env bash
set -euo pipefail
printf 'NODE|%s\n' "$*" >>"$CALL_LOG"
if [[ "\${1:-}" != scripts/release-policy.mjs ]]; then
  echo "unexpected node invocation: $*" >&2
  exit 75
fi
case "\${2:-}" in
  immutable-tag)
    count_file="$FAKE_STATE_DIR/immutable-count"
    count=0
    if [[ -f "$count_file" ]]; then
      count=$(<"$count_file")
    fi
    count=$((count + 1))
    printf '%s' "$count" >"$count_file"
    if [[ "\${FAIL_IMMUTABLE_CALL:-0}" == "$count" ]]; then
      echo "forced immutable preflight failure $count" >&2
      exit 76
    fi
    printf '%s' create
    ;;
  paired-channel)
    printf '{"mainAdvance":true,"browserAdvance":true,"version":"%s"}' "$CANDIDATE_VERSION"
    ;;
  *)
    echo "unexpected release policy command: $*" >&2
    exit 77
    ;;
esac
`.replaceAll("\\${", "${");

const fakeSha256sum = String.raw`#!/usr/bin/env bash
set -euo pipefail
input=$(cat)
if [[ "\${FAIL_SHA256SUM:-0}" == 1 ]]; then
  echo "forced sha256sum failure" >&2
  exit 78
fi
if [[ "$input" == *'"testImage":"browser"'* ]]; then
  printf '%s  -\n' "\${BROWSER_CANDIDATE_DIGEST#sha256:}"
else
  printf '%s  -\n' "\${MAIN_CANDIDATE_DIGEST#sha256:}"
fi
`.replaceAll("\\${", "${");

function createSandbox(t) {
  const root = mkdtempSync(join(tmpdir(), "ocg-container-publish-"));
  const fakeBin = join(root, "bin");
  const state = join(root, "state");
  const runnerTemp = join(root, "runner");
  const output = join(root, "github-output");
  const log = join(root, "calls.log");
  mkdirSync(fakeBin);
  mkdirSync(state);
  mkdirSync(runnerTemp);
  writeExecutable(join(fakeBin, "docker"), fakeDocker);
  writeExecutable(join(fakeBin, "jq"), fakeJq);
  writeExecutable(join(fakeBin, "node"), fakeNode);
  writeExecutable(join(fakeBin, "sha256sum"), fakeSha256sum);
  t.after(() => rmSync(root, { force: true, recursive: true }));

  const env = {
    ...process.env,
    BROWSER_CANDIDATE_DIGEST: digests.browser,
    BROWSER_DIGEST_AMD64: digests.browserAmd64,
    BROWSER_DIGEST_ARM64: digests.browserArm64,
    BROWSER_IMAGE: "ghcr.io/example/ocg-manager-browser",
    CALL_LOG: toBashPath(log),
    CANDIDATE_VERSION: "1.8.0",
    FAKE_BIN: toBashPath(fakeBin),
    FAKE_STATE_DIR: toBashPath(state),
    FULL_SHA: "1234567890abcdef1234567890abcdef12345678",
    GITHUB_OUTPUT: toBashPath(output),
    HELPER: toBashPath(helperPath),
    MAIN_CANDIDATE_DIGEST: digests.main,
    MAIN_DIGEST_AMD64: digests.mainAmd64,
    MAIN_DIGEST_ARM64: digests.mainArm64,
    MAIN_IMAGE: "ghcr.io/example/ocg-manager",
    MINOR_CHANNEL: "1.8",
    PHASE: "publish-immutable",
    PUBLISH_LATEST: "true",
    RUNNER_TEMP: toBashPath(runnerTemp),
    SHORT_SHA: "1234567890ab",
    STABLE: "true",
  };

  return {
    appendLog(value) {
      appendFileSync(log, value, "utf8");
    },
    log() {
      return readFileSync(log, "utf8");
    },
    output() {
      return readFileSync(output, "utf8");
    },
    run(phase, overrides = {}) {
      return spawnSync(
        bashPath,
        ["-c", 'export PATH="$FAKE_BIN:$PATH"; bash "$HELPER" "$PHASE"'],
        {
          cwd: repositoryRoot,
          encoding: "utf8",
          env: { ...env, ...overrides, PHASE: phase },
          timeout: 60_000,
        },
      );
    },
  };
}

function diagnostic(result) {
  return `status=${result.status}\nstdout=${result.stdout}\nstderr=${result.stderr}`;
}

function createLines(log) {
  return log.split(/\r?\n/).filter((line) => line.startsWith("create|") && line.includes("--tag"));
}

function comparableCreateArguments(line) {
  const tokens = line.slice(line.indexOf("|") + 1).split(" ");
  const annotations = [];
  const sources = [];
  for (let index = 0; index < tokens.length; index += 1) {
    if (tokens[index] === "--annotation") annotations.push(tokens[index + 1]);
    if (tokens[index].includes("@sha256:")) sources.push(tokens[index]);
  }
  return { annotations, sources };
}

test("any immutable preflight failure exits before every tag create", async (t) => {
  for (let failedCall = 1; failedCall <= 4; failedCall += 1) {
    await t.test(`immutable preflight ${failedCall}`, (t) => {
      const sandbox = createSandbox(t);
      const result = sandbox.run("publish-immutable", { FAIL_IMMUTABLE_CALL: String(failedCall) });
      assert.notEqual(result.status, 0, diagnostic(result));
      assert.equal(createLines(sandbox.log()).length, 0, sandbox.log());
    });
  }
});

test("immutable publication reuses dry-run sources and annotations and publishes browser first", (t) => {
  const sandbox = createSandbox(t);
  const result = sandbox.run("publish-immutable");
  assert.equal(result.status, 0, `${diagnostic(result)}\n${sandbox.log()}`);

  const lines = sandbox.log().split(/\r?\n/);
  const dryRuns = lines.filter((line) => line.startsWith("create|") && line.includes("--dry-run"));
  const creates = createLines(sandbox.log());
  assert.equal(dryRuns.length, 2, sandbox.log());
  assert.deepEqual(
    creates.map((line) => /--tag ([^ ]+)/.exec(line)?.[1]),
    [
      "ghcr.io/example/ocg-manager-browser:1.8.0",
      "ghcr.io/example/ocg-manager-browser:sha-1234567890ab",
      "ghcr.io/example/ocg-manager:1.8.0",
      "ghcr.io/example/ocg-manager:sha-1234567890ab",
    ],
  );
  for (const create of creates) {
    const image = create.includes("ocg-manager-browser") ? "ocg-manager-browser" : "ocg-manager@";
    const dryRun = dryRuns.find((line) => line.includes(image));
    assert.ok(dryRun, `missing dry-run for ${create}`);
    assert.deepEqual(comparableCreateArguments(create), comparableCreateArguments(dryRun));
  }
  assert.match(sandbox.output(), new RegExp(`main_digest=${digests.main}`));
  assert.match(sandbox.output(), new RegExp(`browser_digest=${digests.browser}`));
});

test("docker, jq, node, and command-substitution failures remain non-zero", async (t) => {
  const scenarios = [
    ["docker", { FAIL_DOCKER_DRY_RUN: "1" }],
    ["jq", { FAIL_JQ: "1" }],
    ["node", { FAIL_IMMUTABLE_CALL: "1" }],
    ["command substitution", { FAIL_SHA256SUM: "1" }],
  ];
  for (const [name, overrides] of scenarios) {
    await t.test(name, (t) => {
      const sandbox = createSandbox(t);
      const result = sandbox.run("publish-immutable", overrides);
      assert.notEqual(result.status, 0, diagnostic(result));
      assert.equal(createLines(sandbox.log()).length, 0, sandbox.log());
    });
  }
});

test("moving phase freshly preflights remote channels then publishes browser before main", (t) => {
  const sandbox = createSandbox(t);
  const immutable = sandbox.run("publish-immutable");
  assert.equal(immutable.status, 0, diagnostic(immutable));
  sandbox.appendLog("===ADVANCE===\n");

  const moving = sandbox.run("advance-moving", {
    BROWSER_DIGEST: digests.browser,
    MAIN_DIGEST: digests.main,
  });
  assert.equal(moving.status, 0, diagnostic(moving));
  const movingLog = sandbox.log().split("===ADVANCE===\n")[1];
  const firstCreate = movingLog.indexOf("create|");
  for (const ref of [
    "ghcr.io/example/ocg-manager:1.8",
    "ghcr.io/example/ocg-manager-browser:1.8",
    "ghcr.io/example/ocg-manager:latest",
    "ghcr.io/example/ocg-manager-browser:latest",
  ]) {
    const inspect = movingLog.indexOf(`inspect|buildx imagetools inspect ${ref}`);
    assert.ok(inspect >= 0 && inspect < firstCreate, `${ref} was not freshly preflighted\n${movingLog}`);
  }
  assert.deepEqual(
    createLines(movingLog).map((line) => /--tag ([^ ]+)/.exec(line)?.[1]),
    [
      "ghcr.io/example/ocg-manager-browser:1.8",
      "ghcr.io/example/ocg-manager:1.8",
      "ghcr.io/example/ocg-manager-browser:latest",
      "ghcr.io/example/ocg-manager:latest",
    ],
  );
});
