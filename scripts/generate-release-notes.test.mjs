import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import test from "node:test";
import { fileURLToPath } from "node:url";

import {
  PLATFORM_RELEASE_NOTES,
  buildReleaseNotes,
  formatChangeLine,
  generateReleaseNotes,
  parseCommitSubject,
  selectPreviousTag,
} from "./generate-release-notes.mjs";

test("parseCommitSubject understands conventional commits and filters noise", () => {
  assert.deepEqual(parseCommitSubject("feat(macos): add Dock icon setting"), {
    kind: "section",
    type: "feat",
    scope: "macos",
    breaking: false,
    description: "add Dock icon setting",
    subject: "feat(macos): add Dock icon setting",
  });
  assert.equal(parseCommitSubject("style: rustfmt multi-protocol").kind, "excluded");
  assert.equal(parseCommitSubject("test: expand frontend coverage").kind, "excluded");
  assert.equal(parseCommitSubject("chore: prepare v1.5.6").kind, "excluded");
  assert.equal(parseCommitSubject("release: prepare v1.5.6").kind, "excluded");
  assert.equal(parseCommitSubject("🔧 chore(gitignore): ignore .kilo").type, "chore");
  assert.equal(parseCommitSubject("fix!: drop legacy path").breaking, true);
  assert.equal(parseCommitSubject("plain commit without type").type, "other");
});

test("buildReleaseNotes groups commits and always appends platform warnings", () => {
  const notes = buildReleaseNotes({
    tag: "v1.5.7",
    previousTag: "v1.5.6",
    subjects: [
      "feat: multi-protocol passthrough",
      "fix: harden sticky-global failover",
      "feat(settings): expose account routing controls",
      "style: rustfmt only",
      "test: unit coverage",
      "chore: prepare v1.5.7",
      "chore: add live routing smoke test script",
      "docs: mention release notes generation",
      "unscoped maintenance tweak",
    ],
  });

  assert.match(notes, /^# OCG Manager v1\.5\.7\n/);
  assert.match(notes, /## Changes since v1\.5\.6/);
  assert.match(notes, /### Features\n\n- multi-protocol passthrough\n- settings: expose account routing controls/);
  assert.match(notes, /### Fixes\n\n- harden sticky-global failover/);
  assert.match(notes, /### Documentation\n\n- mention release notes generation/);
  assert.match(notes, /### Maintenance\n\n- add live routing smoke test script/);
  assert.match(notes, /### Other\n\n- unscoped maintenance tweak/);
  assert.doesNotMatch(notes, /rustfmt only|unit coverage|prepare v1\.5\.7/);
  assert.ok(notes.trimEnd().endsWith(PLATFORM_RELEASE_NOTES));
});

test("empty ranges still produce a readable stub plus platform notes", () => {
  const notes = buildReleaseNotes({
    tag: "v1.0.0",
    previousTag: null,
    subjects: ["style: only formatting", "test: only tests"],
  });
  assert.match(notes, /## Changes since the beginning/);
  assert.match(notes, /No user-facing commits in this range/);
  assert.ok(notes.includes(PLATFORM_RELEASE_NOTES));
});

test("selectPreviousTag walks descending versions", () => {
  assert.equal(selectPreviousTag("v1.5.7", ["v1.5.7", "v1.5.6", "v1.5.5"]), "v1.5.6");
  assert.equal(selectPreviousTag("1.5.5", ["v1.5.7", "v1.5.6", "v1.5.5", "v1.4.2"]), "v1.4.2");
  assert.equal(selectPreviousTag("v1.0.0", ["v1.0.0"]), null);
  assert.throws(() => selectPreviousTag("v9.9.9", ["v1.0.0"]), /was not found/);
});

test("formatChangeLine keeps scope and breaking markers", () => {
  assert.equal(
    formatChangeLine({
      kind: "section",
      type: "fix",
      scope: "gateway",
      breaking: true,
      description: "rename route",
      subject: "fix(gateway)!: rename route",
    }),
    "- gateway: rename route **BREAKING**",
  );
});

test("generateReleaseNotes uses git helpers and previous-tag range", () => {
  const calls = [];
  const runGit = (args) => {
    calls.push(args);
    if (args[0] === "tag") return "v1.5.7\nv1.5.6\nv1.5.5\n";
    if (args[0] === "log") {
      assert.deepEqual(args.slice(0, 2), ["log", "v1.5.6..v1.5.7"]);
      return "feat: shipping notes\nstyle: ignored\n";
    }
    throw new Error(`unexpected git ${args.join(" ")}`);
  };

  const notes = generateReleaseNotes({ tag: "v1.5.7", runGit });
  assert.match(notes, /### Features\n\n- shipping notes/);
  assert.equal(calls.length, 2);
});

test("CLI writes notes for the current repository tag range", () => {
  const script = fileURLToPath(new URL("./generate-release-notes.mjs", import.meta.url));
  const repoRoot = fileURLToPath(new URL("../", import.meta.url));
  const result = spawnSync(
    process.execPath,
    [script, "--tag", "v1.5.7", "--previous", "v1.5.6", "--repo-root", repoRoot],
    { encoding: "utf8", windowsHide: true },
  );
  assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`);
  assert.match(result.stdout, /# OCG Manager v1\.5\.7/);
  assert.match(result.stdout, /## Changes since v1\.5\.6/);
  assert.match(result.stdout, /multi-protocol passthrough|account routing/);
  assert.ok(result.stdout.includes(PLATFORM_RELEASE_NOTES));
});

test("release workflow generates notes from git instead of a fixed blurb", () => {
  const workflow = readFileSync(
    new URL("../.github/workflows/release.yml", import.meta.url),
    "utf8",
  );
  assert.match(workflow, /generate-release-notes\.mjs --tag "\$GITHUB_REF_NAME"/);
  assert.match(workflow, /fetch-depth:\s*0/);
  assert.doesNotMatch(
    workflow,
    /notes="Updater payloads include Tauri minisign signatures/,
  );
});
