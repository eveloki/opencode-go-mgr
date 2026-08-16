import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import test from "node:test";
import { fileURLToPath } from "node:url";

import {
  compareStableVersions,
  immutableTagDecision,
  isPrereleaseVersion,
  normalizeReleaseVersion,
  pairedChannelDecision,
  shouldAdvanceChannel,
  validateComposeVersion,
} from "./release-policy.mjs";

const digest = (character) => `sha256:${character.repeat(64)}`;
const repositoryRoot = fileURLToPath(new URL("../", import.meta.url));
const packageVersion = JSON.parse(
  readFileSync(new URL("../package.json", import.meta.url), "utf8"),
).version;

test("stable release channels advance monotonically", () => {
  assert.equal(shouldAdvanceChannel("v1.5.0", "v1.4.9"), true);
  assert.equal(shouldAdvanceChannel("1.5.0", "1.5.0"), false);
  assert.equal(shouldAdvanceChannel("v1.4.9", "v1.5.0"), false);
  assert.equal(shouldAdvanceChannel("v1.5.0", ""), true);
  assert.equal(compareStableVersions("v10.0.0", "v2.99.99"), 1);
  assert.throws(() => shouldAdvanceChannel("v1.5.0-beta.1", "v1.4.2"), /stable semantic version/);
});

test("prerelease classification preserves the stable-only latest policy", () => {
  assert.equal(isPrereleaseVersion("v1.5.8-beta.1"), true);
  assert.equal(isPrereleaseVersion("1.5.8-rc.2"), true);
  assert.equal(isPrereleaseVersion("v1.5.8"), false);
  assert.equal(normalizeReleaseVersion("v1.5.8-beta.1"), "1.5.8-beta.1");
  assert.throws(() => isPrereleaseVersion("v1.5.8-beta.01"), /semantic version/);
  assert.throws(() => isPrereleaseVersion("v1.5.8+build.1"), /semantic version/);
});

test("immutable image tags are created once or retained at the same digest", () => {
  assert.equal(immutableTagDecision({
    tag: "1.5.0",
    candidateDigest: digest("a"),
    existingDigest: "",
  }), "create");
  assert.equal(immutableTagDecision({
    tag: "sha-0123456789ab",
    candidateDigest: digest("a"),
    existingDigest: digest("a"),
  }), "keep");
  assert.throws(() => immutableTagDecision({
    tag: "1.5.0",
    candidateDigest: digest("a"),
    existingDigest: digest("b"),
  }), /Refusing to move immutable container tag/);
});

test("Compose header and default image must match the release version", () => {
  const valid = `# Pull-only Docker Compose example for OCG Manager v1.5.0.\n`
    + `image: \${OCG_IMAGE:-ghcr.io/klarkxy/opencode-go-mgr:1.5.0}\n`
    + `browser: \${OCG_BROWSER_IMAGE:-ghcr.io/klarkxy/opencode-go-mgr-browser:1.5.0}\n`;
  assert.equal(validateComposeVersion(valid, "1.5.0"), "1.5.0");
  assert.throws(
    () => validateComposeVersion(valid.replace(/:1\.5\.0}/, ":1.4.2}"), "1.5.0"),
    /Compose version mismatch/,
  );
  assert.throws(
    () => validateComposeVersion(valid.replace(/browser:1\.5\.0}/, "browser:1.4.2}"), "1.5.0"),
    /Compose version mismatch/,
  );
  assert.throws(
    () => validateComposeVersion(valid.replace(/^browser:.*\n/m, ""), "1.5.0"),
    /exactly one/,
  );
  assert.throws(
    () => validateComposeVersion(`${valid}${valid}`, "1.5.0"),
    /exactly one/,
  );
  const prerelease = valid.replaceAll("1.5.0", "1.5.0-rc.1");
  assert.equal(validateComposeVersion(prerelease, "v1.5.0-rc.1"), "1.5.0-rc.1");
  assert.equal(normalizeReleaseVersion("v1.5.0-rc.1"), "1.5.0-rc.1");
});

test("manual release runs cannot inherit the production tag path", () => {
  const workflow = readFileSync(
    new URL("../.github/workflows/release.yml", import.meta.url),
    "utf8",
  );
  const buildJob = workflow.match(/\n  build:[\s\S]*?\n  draft-release:/)?.[0] ?? "";
  const draftJob = workflow.match(/\n  draft-release:[\s\S]*?\n  verify-release:/)?.[0] ?? "";
  const verifyJob = workflow.match(/\n  verify-release:[\s\S]*?\n  publish-release:/)?.[0] ?? "";
  const publishJob = workflow.match(/\n  publish-release:[\s\S]*$/)?.[0] ?? "";
  assert.match(workflow, /production: \$\{\{ steps\.matrix\.outputs\.production \}\}/);
  assert.match(
    workflow,
    /if \[\[ "\$GITHUB_EVENT_NAME" == push && "\$GITHUB_REF" == refs\/tags\/v\* \]\]; then/,
  );
  assert.doesNotMatch(workflow, /if:\s*startsWith\(github\.ref, 'refs\/tags\/v'\)/);
  assert.doesNotMatch(workflow, /release-signing|release-candidate/);
  assert.doesNotMatch(workflow, /OCG_TAURI_SIGNING|OCG_RELEASE_APPROVAL_ENABLED/);
  assert.match(
    buildJob,
    /TAURI_SIGNING_PRIVATE_KEY: \$\{\{ needs\.plan\.outputs\.production == 'true' && secrets\.TAURI_SIGNING_PRIVATE_KEY \|\| '' \}\}/,
  );
  assert.match(
    buildJob,
    /TAURI_SIGNING_PRIVATE_KEY_PASSWORD: \$\{\{ needs\.plan\.outputs\.production == 'true' && secrets\.TAURI_SIGNING_PRIVATE_KEY_PASSWORD \|\| '' \}\}/,
  );
  assert.doesNotMatch(buildJob, /environment:/);
  assert.match(draftJob, /if: needs\.plan\.outputs\.production == 'true'/);
  assert.match(verifyJob, /if: needs\.plan\.outputs\.production == 'true'/);
  assert.match(publishJob, /if: needs\.plan\.outputs\.production == 'true'/);
  assert.match(publishJob, /needs:\s+- plan\s+- verify-release/);
  assert.doesNotMatch(publishJob, /environment:|always\(\)/);
});

test("the exact draft Release identity flows through verification and publication", () => {
  const workflow = readFileSync(
    new URL("../.github/workflows/release.yml", import.meta.url),
    "utf8",
  );
  const draftJob = workflow.match(/\n  draft-release:[\s\S]*?\n  verify-release:/)?.[0] ?? "";
  const verifyJob = workflow.match(/\n  verify-release:[\s\S]*?\n  publish-release:/)?.[0] ?? "";
  const publishJob = workflow.match(/\n  publish-release:[\s\S]*$/)?.[0] ?? "";

  assert.match(draftJob, /release_id: \$\{\{ steps\.release\.outputs\.release_id \}\}/);
  assert.match(draftJob, /- name: Create or update draft release\s+id: release/);
  assert.match(draftJob, /gh release view "\$GITHUB_REF_NAME" --json databaseId,isDraft,isPrerelease,tagName/);
  assert.match(draftJob, /release_type_flags=\(\)/);
  assert.match(draftJob, /release_type_flags\+=\(--prerelease\)/);
  assert.match(draftJob, /\.isPrerelease == \$prerelease/);
  assert.match(verifyJob, /release_id: \$\{\{ steps\.asset_metadata\.outputs\.release_id \}\}/);
  assert.match(verifyJob, /permissions:\s+contents: write/);
  assert.match(verifyJob, /RELEASE_ID: \$\{\{ needs\.draft-release\.outputs\.release_id \}\}/);
  assert.match(verifyJob, /if gh api "repos\/\$GITHUB_REPOSITORY\/releases\/\$RELEASE_ID"/);
  assert.match(verifyJob, /\.id == \$release_id and \.tag_name == \$tag and \.draft == true/);
  assert.match(verifyJob, /\.prerelease == \$prerelease/);
  assert.match(
    verifyJob,
    /expected_assets=\$\(cd release && find \. -maxdepth 1 -type f -printf '%f\\n' \| jq -R \. \| jq -s \.\)/,
  );
  assert.match(verifyJob, /--argjson expected_assets "\$expected_assets"/);
  assert.match(
    verifyJob,
    /\(\[\.assets\[\]\.name\] \| sort\) == \(\$expected_assets \| sort\)/,
  );
  assert.doesNotMatch(`${verifyJob}${publishJob}`, /length == 15/);
  assert.doesNotMatch(verifyJob, /releases\/tags\//);
  assert.match(publishJob, /RELEASE_ID: \$\{\{ needs\.verify-release\.outputs\.release_id \}\}/);
  assert.match(publishJob, /gh api "repos\/\$GITHUB_REPOSITORY\/releases\/\$RELEASE_ID" > release-metadata\.json/);
  assert.match(publishJob, /--method PATCH "repos\/\$GITHUB_REPOSITORY\/releases\/\$RELEASE_ID"/);
  assert.match(
    publishJob,
    /if \[ "\$prerelease" = true \]; then[\s\S]*?make_latest=false[\s\S]*?else[\s\S]*?release-policy\.mjs should-advance/,
  );
  assert.match(publishJob, /else[\s\S]*?release-policy\.mjs should-advance/);
  assert.match(publishJob, /-F prerelease="\$prerelease"/);
  assert.match(publishJob, /-f make_latest="\$make_latest"/);
  assert.match(publishJob, /\.prerelease == \$prerelease/);
  assert.doesNotMatch(publishJob, /releases\/tags\//);
});

test("container publication checks out the release tag or an explicit source ref", () => {
  const workflow = readFileSync(
    new URL("../.github/workflows/container.yml", import.meta.url),
    "utf8",
  );
  assert.match(
    workflow,
    /ref: \$\{\{ inputs\.source_ref != '' && inputs\.source_ref \|\| format\('refs\/tags\/\{0\}', steps\.release\.outputs\.tag\) \}\}/,
  );
  assert.match(workflow, /git show-ref --verify --quiet "\$expected_ref"/);
  assert.match(workflow, /tag_commit=\$\(git rev-parse "\$expected_ref\^\{commit\}"\)/);
  assert.match(workflow, /node scripts\/release\.mjs --check/);
  assert.match(workflow, /file: \.\/Dockerfile\.browser/);
  assert.match(workflow, /Smoke-test browser container and real Chromium/);
  assert.match(workflow, /provenance: mode=max[\s\S]*?sbom: true/);
  assert.match(workflow, /subject-name: \$\{\{ needs\.build\.outputs\.browser_image \}\}/);
});

test("paired moving channels either converge at the candidate or remain aligned", () => {
  assert.deepEqual(pairedChannelDecision({
    candidate: "1.5.1",
    mainCurrent: "1.5.0",
    browserCurrent: "1.5.1",
  }), { mainAdvance: true, browserAdvance: false, version: "1.5.1" });
  assert.deepEqual(pairedChannelDecision({
    candidate: "1.5.0",
    mainCurrent: "1.5.1",
    browserCurrent: "1.5.1",
  }), { mainAdvance: false, browserAdvance: false, version: "1.5.1" });
  assert.throws(() => pairedChannelDecision({
    candidate: "1.5.0",
    mainCurrent: "1.5.1",
    browserCurrent: "1.5.2",
  }), /Refusing to leave paired container channel split/);
  assert.throws(() => pairedChannelDecision({
    candidate: "1.5.0",
    mainCurrent: "",
    browserCurrent: "1.5.1",
  }), /Refusing to leave paired container channel split/);
});

test("container publication preflights paired moving channels before publishing the browser then main pair", () => {
  const workflow = readFileSync(
    new URL("../.github/workflows/container.yml", import.meta.url),
    "utf8",
  );
  const publishStep = workflow.match(
    /- name: Publish tags without moving immutable references or rolling channels back[\s\S]*?(?=\n      - name: Generate signed GitHub provenance)/,
  )?.[0] ?? "";
  const mainPreflight = publishStep.indexOf('preflight_immutable_image_tags "$MAIN_IMAGE" "$MAIN_DIGEST"');
  const browserPreflight = publishStep.indexOf('preflight_immutable_image_tags "$BROWSER_IMAGE" "$BROWSER_DIGEST"');
  const movingPreflight = publishStep.indexOf('preflight_moving_pair "$MINOR_CHANNEL"');
  const browserPublish = publishStep.indexOf('publish_immutable_image_tags "$BROWSER_IMAGE" "$BROWSER_DIGEST"');
  const mainPublish = publishStep.indexOf('publish_immutable_image_tags "$MAIN_IMAGE" "$MAIN_DIGEST"');
  const pairVerification = publishStep.indexOf('verify_paired_tag "$CANDIDATE_VERSION"');

  assert.ok(mainPreflight >= 0 && browserPreflight >= 0, "both images must be preflighted");
  assert.ok(movingPreflight >= 0, "moving channels must be preflighted as a pair");
  assert.ok(mainPreflight < browserPublish && browserPreflight < browserPublish,
    "all immutable and moving-tag decisions must finish before the first tag write");
  assert.ok(browserPublish < mainPublish,
    "publish the browser sidecar before exposing the matching main image");
  assert.ok(mainPublish < pairVerification,
    "verify both images together only after their publication sequence completes");
  assert.match(publishStep, /paired-channel/);
  assert.match(publishStep, /MOVING_EXPECTED_VERSIONS\["\$tag"\]=\$version/);
  assert.match(publishStep, /verify_paired_moving_tag/);
  assert.match(publishStep, /verify_published_digest "\$BROWSER_IMAGE:\$tag"/);
});

test("container publication requires anonymous exact-version pulls", () => {
  const workflow = readFileSync(
    new URL("../.github/workflows/container.yml", import.meta.url),
    "utf8",
  );
  const anonymousPull = workflow.match(
    /- name: Verify public anonymous GHCR pulls[\s\S]*?(?=\n      - name: Generate signed GitHub provenance)/,
  )?.[0] ?? "";
  assert.match(anonymousPull, /anonymous_docker_config=\$\(mktemp -d\)/);
  assert.match(anonymousPull, /export DOCKER_CONFIG="\$anonymous_docker_config"/);
  assert.match(anonymousPull, /docker pull --quiet "\$MAIN_IMAGE:\$CANDIDATE_VERSION"/);
  assert.match(anonymousPull, /docker pull --quiet "\$BROWSER_IMAGE:\$CANDIDATE_VERSION"/);
});

test("release preflight rejects a tag that does not match repository versions", () => {
  const result = spawnSync(
    process.execPath,
    [fileURLToPath(new URL("./release.mjs", import.meta.url)), "--check"],
    {
      cwd: repositoryRoot,
      encoding: "utf8",
      env: {
        ...process.env,
        OCG_RELEASE_TAG: "v9.9.9",
        OCG_REQUIRE_UPDATER_ARTIFACTS: "0",
      },
    },
  );
  assert.notEqual(result.status, 0);
  const output = `${result.stdout}\n${result.stderr}`;
  assert.ok(
    output.includes(`Release tag v9.9.9 does not match version ${packageVersion}`),
    output,
  );
});
