import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const applications = readFileSync(new URL("./Applications.vue", import.meta.url), "utf8");

test("Applications blocks connector writes while conflict or partial needs attention", () => {
  assert.match(
    applications,
    /\["ready", "connected"\]\.includes\(activeConnector\.value\?\.status \?\? ""\)/,
  );
  assert.match(
    applications,
    /activeConnector\.value\?\.status === "connected"/,
  );
  assert.match(
    applications,
    /\['conflict', 'partial'\]\.includes\(activeConnector\.status\)/,
  );
  assert.match(applications, /需要人工处理/);
});

test("Applications exposes the fixed managed-config and native-plugin connector surfaces", () => {
  const connectorSet = applications.match(
    /const CONNECTOR_IDS = new Set<ApplicationId>\(\[([\s\S]*?)\]\);/,
  );
  assert.ok(connectorSet, "connector id set must remain explicit");
  assert.deepEqual(
    [...connectorSet[1].matchAll(/"([a-z-]+)"/g)].map((match) => match[1]),
    [
      "claude-code",
      "codex",
      "dsh",
      "gemini-cli",
      "opencode",
      "openclaw",
      "pi",
      "hermes",
    ],
  );
  const nativePluginSet = applications.match(
    /const NATIVE_PLUGIN_CONNECTOR_IDS = new Set<ApplicationId>\(\[([^\]]+)\]\);/,
  );
  assert.ok(nativePluginSet, "native plugin connector id set must remain explicit");
  assert.deepEqual(
    [...nativePluginSet[1].matchAll(/"([a-z-]+)"/g)].map((match) => match[1]),
    ["dsh", "pi"],
  );
  const nativeCredentialSet = applications.match(
    /const CLIENT_NATIVE_CREDENTIAL_CONNECTOR_IDS = new Set<ApplicationId>\(\[([^\]]+)\]\);/,
  );
  assert.ok(nativeCredentialSet, "client-native credential connector set must remain explicit");
  assert.deepEqual(
    [...nativeCredentialSet[1].matchAll(/"([a-z-]+)"/g)].map((match) => match[1]),
    ["pi"],
  );
  assert.match(applications, /插件已安装；重新打开客户端并在其凭据入口保存 Key/);
  assert.match(applications, /插件和本机凭据已安装；重新打开 DSH 后生效/);
  assert.match(
    applications,
    /action === "connect" && !clientNativeCredentialConnector\.value[\s\S]*selectedKeyId\.value/,
  );
});

test("native plugin uninstall confirmation never asks the user to save a Key", () => {
  assert.match(
    applications,
    /connectorPreview\.value\?\.action === "restore"[\s\S]*卸载完成后请重新打开客户端。[\s\S]*安装完成后请重新打开客户端并使用其原生凭据入口保存 Key。/,
  );
  assert.match(
    applications,
    /activeGuide\.value\.id === "dsh"[\s\S]*只安装 OCG 插件并管理 DSH \.env 中的 OCG_MANAGER_API_KEY/,
  );
});
