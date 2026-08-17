import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";

test("the keys page owns lifecycle CRUD and does not render plaintext values", async () => {
  const source = await readFile(new URL("./Keys.vue", import.meta.url), "utf8");
  const template = source.slice(source.indexOf("<template>"), source.indexOf("<script setup"));

  assert.match(template, /id="gateway-keys-title"/);
  assert.match(template, /class="gateway-key-row gateway-key-row--primary"/);
  assert.match(template, /v-for="entry in connection\.sub_keys"/);
  assert.match(template, /t\(['"]新建 Key['"]\)/);
  assert.match(template, />\{\{ t\("新建"\) \}\}<\/n-button>/);
  assert.match(template, /class="gateway-key-actions"/);
  assert.match(template, /class="gateway-key-split"/);
  assert.ok(
    template.indexOf("t('复制 Key')") < template.indexOf("t('刷新 Key')")
      && template.lastIndexOf("t('刷新 Key')") < template.lastIndexOf("t('启用或停用 Key')")
      && template.lastIndexOf("t('启用或停用 Key')") < template.lastIndexOf("t('删除 Key')"),
  );
  assert.doesNotMatch(template, /t\("保存主 Key 值"\)/);
  assert.doesNotMatch(template, /v-model:value="primaryKeyDraft"/);
  assert.doesNotMatch(template, /t\("自定义主 Key 值"\)/);
  assert.match(template, /\{\{ maskConnectionKey\(connection\.primary_key\) \}\}/);
  assert.match(template, /\{\{ maskConnectionKey\(entry\.value\) \}\}/);
  assert.doesNotMatch(template, /<code>\{\{ connection\.primary_key \}\}<\/code>/);
  assert.doesNotMatch(template, /<code>\{\{ entry\.value \}\}<\/code>/);
});

test("the keys page uses ConnectionInfo and resets the primary key instead of editing it", async () => {
  const source = await readFile(new URL("./Keys.vue", import.meta.url), "utf8");

  assert.match(source, /ref<ConnectionInfo>/);
  assert.doesNotMatch(source, /ref<AppConfig>/);
  assert.match(source, /tauriApi\.getConnection\(\)/);
  assert.match(source, /tauriApi\.createGatewayKey\(name, connection\.value\.revision\)/);
  assert.match(source, /tauriApi\.updateGatewayKey\(entry\.id, \{ enabled \}, connection\.value\.revision\)/);
  assert.match(source, /tauriApi\.deleteGatewayKey\(entry\.id, connection\.value\.revision\)/);
  assert.match(source, /tauriApi\.regenerateGatewayKey\(\)/);
  assert.match(source, /tauriApi\.regenerateGatewayKeyEntry\(entry\.id, connection\.value\.revision\)/);
  assert.doesNotMatch(source, /tauriApi\.getSettings\(\)|tauriApi\.updateSettings\(/);
  assert.match(source, /onActivated\(\(\) => \{\s*if \(!loading\.value\) void loadConnection\(\);/);
  assert.doesNotMatch(source, /validatePrimaryKey\(\)|savePrimaryKey|primaryKeyDraft/);
});

test("the dashboard consume surface does not host key lifecycle controls", async () => {
  const dashboard = await readFile(new URL("./Dashboard.vue", import.meta.url), "utf8");
  const template = dashboard.slice(dashboard.indexOf("<template>"), dashboard.indexOf("<script setup"));

  assert.match(template, /t\("管理接入 Key"\)/);
  assert.doesNotMatch(template, /t\("新建 Key"\)/);
  assert.doesNotMatch(template, /t\("删除 Key"\)/);
  assert.doesNotMatch(template, /t\("启用或停用 Key"\)/);
  assert.doesNotMatch(template, /t\("自定义主 Key 值"\)/);
});

test("app registers the keys view between dashboard and accounts", async () => {
  const app = await readFile(new URL("../App.vue", import.meta.url), "utf8");

  assert.match(app, /type ViewKey = "dashboard" \| "keys" \| "accounts"/);
  assert.match(app, /keys: "接入 Key"/);
  assert.match(app, /<Keys v-else-if="activeKey === 'keys'" \/>/);
  assert.match(app, /<Dashboard v-if="activeKey === 'dashboard'" @navigate="selectView" \/>/);
  assert.match(app, /import\("\.\/views\/Keys\.vue"\)/);
  assert.match(app, /\{ label: t\("接入 Key"\), key: "keys"/);
  assert.match(app, /\{ label: t\("账号"\), key: "accounts", icon: renderIcon\(TeamOutlined\) \}/);
});
