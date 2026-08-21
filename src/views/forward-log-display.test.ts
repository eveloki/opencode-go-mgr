import assert from "node:assert/strict";
import test from "node:test";
import {
  forwardLogAlias,
  forwardLogRequestedModel,
  forwardLogResolvedAlias,
  forwardLogUpstreamModel,
} from "./forward-log-display.ts";

test("forward log Alias columns keep requested, resolved, and upstream meanings distinct", () => {
  const row = {
    model: "legacy-model",
    requested_model: "sonnet",
    resolved_alias: "claude-sonnet",
    upstream_model: "anthropic/claude-sonnet-4",
  };
  assert.equal(forwardLogAlias(row), "claude-sonnet");
  assert.equal(forwardLogRequestedModel(row), "sonnet");
  assert.equal(forwardLogResolvedAlias(row), "claude-sonnet");
  assert.equal(forwardLogUpstreamModel(row), "anthropic/claude-sonnet-4");
});

test("legacy forward logs still expose their stored model without inventing an Alias", () => {
  assert.equal(forwardLogAlias({ model: "legacy", requested_model: null, resolved_alias: null }), "legacy");
  assert.equal(forwardLogRequestedModel({ requested_model: null }), null);
  assert.equal(forwardLogResolvedAlias({ resolved_alias: "  " }), null);
});
