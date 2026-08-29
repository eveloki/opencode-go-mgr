import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import { DEFAULT_DEV_GATEWAY_PORT, devEnvironment } from "./dev.mjs";

test("development uses 19042 unless an explicit Gateway port is provided", () => {
  assert.equal(DEFAULT_DEV_GATEWAY_PORT, "19042");
  assert.equal(devEnvironment({}).OCG_GATEWAY_PORT, "19042");
  assert.equal(devEnvironment({ OCG_GATEWAY_PORT: "" }).OCG_GATEWAY_PORT, "19042");
  assert.equal(devEnvironment({ OCG_GATEWAY_PORT: " 19043 " }).OCG_GATEWAY_PORT, "19043");
});

test("pnpm dev enters the shared development launcher", async () => {
  const packageJson = JSON.parse(await readFile(new URL("../package.json", import.meta.url), "utf8"));
  assert.equal(packageJson.scripts.dev, "node scripts/dev.mjs");
});
