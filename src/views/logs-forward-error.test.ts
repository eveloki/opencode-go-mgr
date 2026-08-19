import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const logs = readFileSync(new URL("./Logs.vue", import.meta.url), "utf8");

test("forward log failures surface a persistent retry alert distinct from the empty state", () => {
  assert.match(logs, /const forwardError = ref\(""\)/);

  // The alert lives inside the forward tab pane, above the remote table, and
  // its retry button reloads the forward list.
  const forwardPane = logs.slice(logs.indexOf('name="forward"'));
  const alert = forwardPane.slice(0, forwardPane.indexOf("n-data-table"));
  assert.match(alert, /<n-alert v-if="forwardError" type="error"/);
  assert.match(alert, /@click="loadForwardLogs"/);
  assert.match(alert, /t\("重试"\)/);

  // The legitimate empty state stays separate from the error state.
  assert.match(forwardPane, /<template #empty>/);
  assert.match(forwardPane, /仅记录经本机 API 转发的请求/);
});

test("forward log loader clears the alert on retry and reports dashboardErrorDetail", () => {
  const loader = logs.slice(
    logs.indexOf("async function loadForwardLogs"),
    logs.indexOf("async function loadAccounts"),
  );
  // Every attempt clears the previous error up front, so retry and success
  // both dismiss the alert.
  assert.match(loader, /forwardLoading\.value = true;\s*forwardError\.value = ""/);
  assert.match(loader, /forwardError\.value = dashboardErrorDetail\(e\)/);
  assert.match(loader, /message\.error\(t\("加载请求日志失败: \{error\}", \{ error: forwardError\.value \}\)\)/);
  assert.doesNotMatch(loader, /String\(e\)/);
  // Stale responses must not resurrect an error after a newer request starts.
  assert.match(loader, /if \(request === forwardRequest\) \{/);
});
