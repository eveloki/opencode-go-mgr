import assert from "node:assert/strict";
import test from "node:test";
import {
  GOAT_PRICING_REFERENCE,
  PRICING_REFERENCE_CHECKED_AT,
  SCNET_PRICING_REFERENCE,
} from "./pricing-references.ts";

test("GOAT reference preserves official monthly price, credits, and rolling limits", () => {
  assert.equal(PRICING_REFERENCE_CHECKED_AT, "2026-08-22");
  assert.equal(GOAT_PRICING_REFERENCE.monthlyPriceUsd, 10);
  assert.equal(GOAT_PRICING_REFERENCE.monthlyCreditsUsd, 70);
  assert.equal(GOAT_PRICING_REFERENCE.approximateRequests, "75K");
  assert.deepEqual(GOAT_PRICING_REFERENCE.rollingLimits, [
    { window: "5 小时", creditsUsd: 14 },
    { window: "7 天", creditsUsd: 35 },
  ]);
  assert.match(GOAT_PRICING_REFERENCE.sourceUrl, /^https:\/\/commandcode\.ai\//);
});

test("SCNet reference preserves official tier prices and monthly Credits", () => {
  assert.deepEqual(
    SCNET_PRICING_REFERENCE.tiers.map((tier) => ({
      id: tier.id,
      list: tier.listPriceCny,
      promotion: tier.promotionalPriceCny,
      credits: tier.monthlyCredits,
    })),
    [
      { id: "basic", list: 50, promotion: 30, credits: 60_000 },
      { id: "standard", list: 185, promotion: 110, credits: 240_000 },
      { id: "premium", list: 440, promotion: 265, credits: 600_000 },
    ],
  );
  assert.match(SCNET_PRICING_REFERENCE.sourceUrl, /^https:\/\/www\.scnet\.cn\//);
  assert.match(SCNET_PRICING_REFERENCE.restrictionsUrl, /^https:\/\/www\.scnet\.cn\//);
});
