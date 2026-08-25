import assert from "node:assert/strict";
import test from "node:test";
import {
  CUSTOM_BASE_URL_ISSUE_KEYS,
  customAccountNeedsVerification,
  customAccountToggleBlocked,
  customBaseUrlIssue,
  isCustomApiAccount,
} from "./custom-account.ts";
import type { Account } from "../api/dashboard.ts";

function customAccount(verification_status: Account["verification_status"]) {
  return { provider_id: "custom", offering_id: "api", verification_status };
}

test("administrator-trusted base URLs allow LAN, localhost, metadata, and plain HTTP", () => {
  const trusted = [
    "http://192.168.1.10:8080/v1",
    "http://10.0.0.2/openai",
    "http://localhost:3000",
    "http://127.0.0.1:11434/v1",
    "http://[::1]:8080/v1",
    "http://169.254.169.254/latest",
    "http://nas.lan:4000/v1/",
    "https://api.example.com/v1",
  ];
  for (const url of trusted) {
    assert.equal(customBaseUrlIssue(url), null, url);
    assert.equal(customBaseUrlIssue(`  ${url}  `), null, `trimmed ${url}`);
  }
});

test("base URL validation only rejects malformed, non-http(s), and credentialed input", () => {
  assert.equal(customBaseUrlIssue(""), "empty");
  assert.equal(customBaseUrlIssue("   "), "empty");
  assert.equal(customBaseUrlIssue("not-a-url"), "malformed");
  assert.equal(customBaseUrlIssue("api.example.com/v1"), "malformed");
  assert.equal(customBaseUrlIssue("//api.example.com/v1"), "malformed");
  assert.equal(customBaseUrlIssue("ftp://api.example.com/v1"), "not_http");
  assert.equal(customBaseUrlIssue("javascript:alert(1)"), "not_http");
  assert.equal(customBaseUrlIssue("ws://api.example.com"), "not_http");
  assert.equal(customBaseUrlIssue("https://user:pass@api.example.com"), "with_credentials");
  assert.equal(customBaseUrlIssue("http://user@192.168.1.10:8080"), "with_credentials");

  for (const issue of ["empty", "malformed", "not_http", "with_credentials"] as const) {
    assert.ok(CUSTOM_BASE_URL_ISSUE_KEYS[issue], issue);
  }
});

test("verification gating only applies to Custom API accounts", () => {
  assert.ok(isCustomApiAccount({ provider_id: "custom", offering_id: "api" }));
  assert.ok(!isCustomApiAccount({ provider_id: "opencode", offering_id: "go" }));
  assert.ok(!isCustomApiAccount({ provider_id: "custom", offering_id: "other" }));

  assert.ok(customAccountNeedsVerification(customAccount("pending")));
  assert.ok(customAccountNeedsVerification(customAccount("failed")));
  assert.ok(!customAccountNeedsVerification(customAccount("verified")));
  assert.ok(!customAccountNeedsVerification(customAccount("not_required")));
  assert.ok(!customAccountNeedsVerification({
    provider_id: "command-code",
    offering_id: "goat",
    verification_status: "pending",
  }));

  // Verification is never enablement: only a verified Custom account may use
  // the normal enable switch.
  assert.ok(customAccountToggleBlocked(customAccount("pending")));
  assert.ok(customAccountToggleBlocked(customAccount("failed")));
  assert.ok(customAccountToggleBlocked(customAccount("not_required")));
  assert.ok(!customAccountToggleBlocked(customAccount("verified")));
  assert.ok(!customAccountToggleBlocked({
    provider_id: "opencode",
    offering_id: "go",
    verification_status: "not_required",
  }));
});
