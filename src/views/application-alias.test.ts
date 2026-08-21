import assert from "node:assert/strict";
import test from "node:test";
import { selectedApplicationAlias } from "./application-alias.ts";

test("Applications copies the user-selected Alias independently of catalog order or failure", () => {
  assert.equal(selectedApplicationAlias(undefined, {}, "  kimi-k3  "), "kimi-k3");
  assert.equal(selectedApplicationAlias(undefined, {}, null), "");
  assert.equal(
    selectedApplicationAlias(["model", "review_model"], {
      model: "gpt-5.6-luna",
      review_model: "grok-4.6",
    }, "catalog-first-must-not-win"),
    "gpt-5.6-luna",
  );
});
