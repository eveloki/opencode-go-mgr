import assert from "node:assert/strict";
import test from "node:test";
import { normalizeProxyUrl } from "./settings-proxy.ts";

test("manual proxy URL is normalized to an HTTP origin", () => {
  assert.equal(normalizeProxyUrl("manual", " http://127.0.0.1:7890/ "), "http://127.0.0.1:7890");
  assert.equal(normalizeProxyUrl("auto", ""), "");
  assert.equal(
    normalizeProxyUrl("auto", " http://127.0.0.1:7890/ "),
    "http://127.0.0.1:7890",
  );
});

test("manual proxy rejects missing, credentialed, and non-origin URLs", () => {
  for (const value of [
    "",
    "socks5://127.0.0.1:1080",
    "http://user:secret@127.0.0.1:7890",
    "http://127.0.0.1:7890/proxy",
  ]) {
    assert.throws(() => normalizeProxyUrl("manual", value));
  }
});

test("non-manual modes keep leftover invalid URLs instead of blocking save", () => {
  assert.equal(normalizeProxyUrl("auto", "socks5://127.0.0.1:1080"), "socks5://127.0.0.1:1080");
  assert.equal(normalizeProxyUrl("direct", "not a proxy"), "not a proxy");
});
