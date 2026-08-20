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

test("resolve is the only phase that interprets a mutable container source ref", () => {
  const workflow = readFileSync(
    new URL("../.github/workflows/container.yml", import.meta.url),
    "utf8",
  );
  const resolveJob = workflow.match(/\n  resolve:[\s\S]*?\n  build:/)?.[0] ?? "";
  const buildJob = workflow.match(/\n  build:[\s\S]*?\n  publish:/)?.[0] ?? "";
  const publishJob = workflow.match(/\n  publish:[\s\S]*$/)?.[0] ?? "";

  assert.match(
    resolveJob,
    /ref: \$\{\{ inputs\.source_ref != '' && inputs\.source_ref \|\| format\('refs\/tags\/\{0\}', steps\.release\.outputs\.tag\) \}\}/,
  );
  assert.match(resolveJob, /full_sha=\$\(git rev-parse HEAD\)/);
  assert.match(resolveJob, /node scripts\/release\.mjs --check/);

  assert.match(buildJob, /ref: \$\{\{ needs\.resolve\.outputs\.full_sha \}\}/);
  assert.match(buildJob, /EXPECTED_SHA: \$\{\{ needs\.resolve\.outputs\.full_sha \}\}/);
  assert.match(buildJob, /actual_sha=\$\(git rev-parse HEAD\)/);
  assert.match(buildJob, /\[\[ "\$actual_sha" != "\$EXPECTED_SHA" \]\]/);
  assert.doesNotMatch(buildJob, /inputs\.source_ref|refs\/tags|github\.workflow_sha/,
    "both matrix legs must build the resolved commit without symbolic-ref reuse");

  assert.match(publishJob, /ref: \$\{\{ github\.workflow_sha \}\}/);
  assert.match(publishJob, /EXPECTED_WORKFLOW_SHA: \$\{\{ github\.workflow_sha \}\}/);
  assert.match(publishJob, /\[\[ "\$actual_sha" != "\$EXPECTED_WORKFLOW_SHA" \]\]/);
  assert.doesNotMatch(publishJob, /inputs\.source_ref|refs\/tags/,
    "the privileged publish helper must come from the trusted workflow commit");

  assert.match(workflow, /docker-bake\.hcl/);
  assert.match(workflow, /file: \.\/Dockerfile\.browser/);
  assert.match(workflow, /Smoke-test browser container and real Chromium/);
  assert.match(workflow, /provenance: mode=max[\s\S]*?sbom: true/);
  assert.match(workflow, /subject-name: \$\{\{ needs\.resolve\.outputs\.browser_image \}\}/);
  assert.match(workflow, /subject-name: \$\{\{ needs\.resolve\.outputs\.image \}\}/);
  assert.match(workflow, /subject-digest: \$\{\{ steps\.immutable\.outputs\.main_digest \}\}/);
  assert.match(workflow, /subject-digest: \$\{\{ steps\.immutable\.outputs\.browser_digest \}\}/);
});

test("container publication builds each architecture natively and merges both digests", () => {
  const workflow = readFileSync(
    new URL("../.github/workflows/container.yml", import.meta.url),
    "utf8",
  );
  assert.match(workflow, /arch: \[amd64, arm64\]/);
  assert.match(workflow, /- arch: amd64\n\s+runs-on: ubuntu-24\.04\n/);
  assert.match(workflow, /- arch: arm64\n\s+runs-on: ubuntu-24\.04-arm\n/);
  assert.match(workflow, /runs-on: \$\{\{ matrix\.runs-on \}\}/);
  assert.match(workflow, /timeout-minutes: 120/);
  // Without parameterized platforms the arm64 leg would bake amd64 images through QEMU.
  assert.match(workflow, /platforms: linux\/\$\{\{ matrix\.arch \}\}/);
  assert.doesNotMatch(workflow, /platforms: linux\/amd64\b/);

  assert.match(workflow, /browser_digest_amd64: \$\{\{ steps\.record_amd64\.outputs\.browser_digest \}\}/);
  assert.match(workflow, /browser_digest_arm64: \$\{\{ steps\.record_arm64\.outputs\.browser_digest \}\}/);
  assert.match(workflow, /digest_amd64: \$\{\{ steps\.record_amd64\.outputs\.digest \}\}/);
  assert.match(workflow, /digest_arm64: \$\{\{ steps\.record_arm64\.outputs\.digest \}\}/);
  assert.match(workflow, /MAIN_DIGEST_AMD64: \$\{\{ needs\.build\.outputs\.digest_amd64 \}\}/);
  assert.match(workflow, /MAIN_DIGEST_ARM64: \$\{\{ needs\.build\.outputs\.digest_arm64 \}\}/);
  assert.match(workflow, /BROWSER_DIGEST_AMD64: \$\{\{ needs\.build\.outputs\.browser_digest_amd64 \}\}/);
  assert.match(workflow, /BROWSER_DIGEST_ARM64: \$\{\{ needs\.build\.outputs\.browser_digest_arm64 \}\}/);
  assert.match(workflow, /if: matrix\.arch == 'amd64'/);
  assert.match(workflow, /if: matrix\.arch == 'arm64'/);
  assert.match(workflow, /cache-from: type=gha,scope=ocg-manager-linux-\$\{\{ matrix\.arch \}\}/);
  assert.match(workflow, /cache-to: type=gha,mode=max,scope=ocg-manager-linux-\$\{\{ matrix\.arch \}\}/);
  assert.match(workflow, /cache-from: type=gha,scope=ocg-browser-linux-\$\{\{ matrix\.arch \}\}/);
  assert.match(workflow, /cache-to: type=gha,mode=max,scope=ocg-browser-linux-\$\{\{ matrix\.arch \}\}/);
  assert.match(workflow, /CACHE_SCOPE=ocg-manager-linux-\$\{\{ matrix\.arch \}\}/);
  assert.match(workflow, /CACHE_BROWSER_SCOPE=ocg-browser-linux-\$\{\{ matrix\.arch \}\}/);
  assert.match(workflow, /SMOKE_IMAGE: ocg-manager:smoke-\$\{\{ github\.run_id \}\}-\$\{\{ matrix\.arch \}\}/);

  const bake = readFileSync(
    new URL("../docker-bake.hcl", import.meta.url),
    "utf8",
  );
  assert.match(bake, /variable "CACHE_SCOPE" \{\s*\n\s*default = "ocg-manager-linux-amd64"/);
  assert.match(bake, /variable "CACHE_BROWSER_SCOPE" \{\s*\n\s*default = "ocg-browser-linux-amd64"/);
  assert.match(bake, /cache-from = \["type=gha,scope=\$\{CACHE_SCOPE\}"\]/);
  assert.match(bake, /cache-from = \["type=gha,scope=\$\{CACHE_BROWSER_SCOPE\}"\]/);
  // A platforms pin in the bake smoke targets would bake the wrong architecture
  // on the arm64 leg; the native runner default is required.
  assert.doesNotMatch(bake, /platforms\s*=/);
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

test("immutable publication, public proof, attestations, and moving channels are strictly ordered", () => {
  const workflow = readFileSync(
    new URL("../.github/workflows/container.yml", import.meta.url),
    "utf8",
  );
  const immutable = workflow.indexOf("      - name: Publish and verify immutable version and commit tags");
  const anonymous = workflow.indexOf("      - name: Verify public anonymous GHCR pulls");
  const mainAttestation = workflow.indexOf("      - name: Generate signed GitHub provenance\n");
  const browserAttestation = workflow.indexOf("      - name: Generate signed GitHub provenance for browser image");
  const moving = workflow.indexOf("      - name: Re-read and advance paired moving channels");

  assert.ok(
    immutable >= 0
      && immutable < anonymous
      && anonymous < mainAttestation
      && mainAttestation < browserAttestation
      && browserAttestation < moving,
    "moving channels must wait for immutable tags, anonymous pulls, and both attestations",
  );
  assert.match(workflow, /concurrency:\n\s+group: ghcr-moving-channels\n\s+queue: max/);
  assert.match(workflow, /run: bash scripts\/container-publish\.sh publish-immutable/);
  assert.match(workflow, /run: bash scripts\/container-publish\.sh advance-moving/);
  assert.match(workflow, /subject-digest: \$\{\{ steps\.immutable\.outputs\.main_digest \}\}/);
  assert.match(workflow, /subject-digest: \$\{\{ steps\.immutable\.outputs\.browser_digest \}\}/);
});

test("candidate index digests are computed without a registry tag before every immutable preflight", () => {
  const helper = readFileSync(
    new URL("./container-publish.sh", import.meta.url),
    "utf8",
  );
  const candidateStart = helper.indexOf("candidate_manifest_for() {");
  const candidateEnd = helper.indexOf("verify_published_digest() {");
  const candidateFunction = helper.slice(candidateStart, candidateEnd);
  const immutableStart = helper.indexOf("publish_immutable_phase() {");
  const movingStart = helper.indexOf("advance_moving_phase() {");
  const immutablePhase = helper.slice(immutableStart, movingStart);
  const localMainDigest = immutablePhase.indexOf('main_digest=$(manifest_digest "$main_manifest")');
  const immutablePreflight = immutablePhase.indexOf('immutable_decisions["$key"]=$(check_immutable');
  const firstTagWrite = immutablePhase.indexOf('publish_immutable_image_tags "$BROWSER_IMAGE"');

  assert.match(candidateFunction, /docker buildx imagetools create[\s\S]*?--dry-run/);
  assert.doesNotMatch(candidateFunction, /--tag|staging/);
  assert.match(helper, /if ! checksum=\$\(printf '%s' "\$manifest" \| sha256sum\); then/);
  assert.match(immutablePhase, /browser_manifest=\$\(candidate_manifest_for "\$BROWSER_IMAGE"\)/);
  assert.match(immutablePhase, /main_manifest=\$\(candidate_manifest_for "\$MAIN_IMAGE"\)/);
  assert.match(immutablePhase, /verify_index_json "\$BROWSER_IMAGE" "browser candidate dry-run" "\$browser_manifest"/);
  assert.match(immutablePhase, /verify_index_json "\$MAIN_IMAGE" "main candidate dry-run" "\$main_manifest"/);
  assert.ok(localMainDigest >= 0 && localMainDigest < immutablePreflight,
    "both local candidate digests must exist before immutable preflight");
  assert.ok(immutablePreflight >= 0 && immutablePreflight < firstTagWrite,
    "all immutable decisions must finish before the first version or sha tag write");
  assert.match(
    immutablePhase,
    /for image_and_digest in[\s\S]*?"\$MAIN_IMAGE \$main_digest"[\s\S]*?"\$BROWSER_IMAGE \$browser_digest"[\s\S]*?for tag in "\$CANDIDATE_VERSION" "sha-\$SHORT_SHA"/,
  );
  assert.match(helper, /application\/vnd\.oci\.image\.index\.v1\+json/);
  assert.match(helper, /org\.opencontainers\.image\.version/);
  assert.match(helper, /org\.opencontainers\.image\.revision/);
  assert.match(helper, /arch_manifests[\s\S]*?expected_manifests/);
  assert.doesNotMatch(helper, /derive_candidate_index|staging[-_]/);
});

test("moving channels re-read remote state and publish browser before main", () => {
  const helper = readFileSync(
    new URL("./container-publish.sh", import.meta.url),
    "utf8",
  );
  const movingStart = helper.indexOf("advance_moving_phase() {");
  const movingPhase = helper.slice(movingStart);
  const freshRead = movingPhase.indexOf(
    'verify_remote_candidate_index "$BROWSER_IMAGE" "$BROWSER_IMAGE:$CANDIDATE_VERSION" "$BROWSER_DIGEST"',
  );
  const minorPreflight = movingPhase.indexOf('preflight_moving_pair "$MINOR_CHANNEL"');
  const minorPublish = movingPhase.lastIndexOf('publish_moving_pair "$MINOR_CHANNEL"');
  const browserPublish = movingPhase.indexOf('publish_moving "$BROWSER_IMAGE" "$tag" "$BROWSER_DIGEST"');
  const mainPublish = movingPhase.indexOf('publish_moving "$MAIN_IMAGE" "$tag" "$MAIN_DIGEST"');

  assert.ok(freshRead >= 0 && freshRead < minorPreflight,
    "moving-channel phase must freshly verify the immutable candidate first");
  assert.ok(minorPreflight >= 0 && minorPreflight < minorPublish,
    "all moving-channel decisions must finish before the first moving write");
  assert.ok(browserPublish >= 0 && browserPublish < mainPublish,
    "the browser dependency must move before the matching main channel");
  assert.match(movingPhase, /if \[\[ "\$PUBLISH_LATEST" == true \]\]; then\r?\n\s+preflight_moving_pair latest/);
  assert.match(movingPhase, /decision=\$\(node scripts\/release-policy\.mjs paired-channel/);
  assert.match(movingPhase, /verify_paired_moving_tag "\$tag"/);
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
  assert.match(anonymousPull, /verify_public_index "\$MAIN_IMAGE:\$CANDIDATE_VERSION"/);
  assert.match(anonymousPull, /verify_public_index "\$BROWSER_IMAGE:\$CANDIDATE_VERSION"/);
  assert.match(anonymousPull, /"amd64,arm64"/,
    "anonymous pulls must expose both architectures in the public manifest");
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
