import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import {
  GOAT_PRICING_REFERENCE,
  PRICING_REFERENCE_CHECKED_AT,
} from "./pricing-references.ts";

test("GOAT reference mirrors the official plan summary and 40 included models", () => {
  assert.equal(PRICING_REFERENCE_CHECKED_AT, "2026-08-24");
  assert.equal(GOAT_PRICING_REFERENCE.includedModelCount, 40);
  assert.equal(GOAT_PRICING_REFERENCE.models.length, 40);
  assert.deepEqual(
    GOAT_PRICING_REFERENCE.models.find(({ model }) => model === "GPT-5.6 Sol"),
    {
      model: "GPT-5.6 Sol",
      input: 5,
      output: 30,
      cacheRead: 0.5,
      cacheWrite: 6.25,
      quotaMultiplier: 1,
    },
  );
  assert.equal(
    GOAT_PRICING_REFERENCE.models.find(({ model }) => model === "Gemini 3.7 Flash")
      ?.quotaMultiplier,
    1.75,
  );
  assert.equal(
    GOAT_PRICING_REFERENCE.models.filter(({ quotaMultiplier }) => quotaMultiplier === null).length,
    2,
  );
  assert.equal(GOAT_PRICING_REFERENCE.models.some(({ model }) => model.startsWith("Claude")), false);
  assert.match(GOAT_PRICING_REFERENCE.sourceUrl, /commandcode\.ai\/docs\/plans\/goat$/);
  assert.match(GOAT_PRICING_REFERENCE.pricingUrl, /commandcode\.ai\/docs\/plans\/goat#models-included$/);
});

test("GOAT pricing keeps a static fallback and exposes editable saved multipliers", () => {
  const files = [
    new URL("./pricing-references.ts", import.meta.url),
    new URL("../components/GoatQuotaReference.vue", import.meta.url),
    new URL("../components/ProviderPricingReference.vue", import.meta.url),
    new URL("../components/PricingCatalog.vue", import.meta.url),
  ];
  for (const file of files) {
    const source = readFileSync(file, "utf8");
    assert.doesNotMatch(source, /75K|approximateRequests/);
    assert.doesNotMatch(source, /promotionalPriceCny|listPriceCny/);
  }
  const quota = readFileSync(new URL("../components/GoatQuotaReference.vue", import.meta.url), "utf8");
  assert.match(quota, /GOAT_PRICING_REFERENCE\.models/);
  assert.match(quota, /未知价格不会参与费用估算/);
  assert.match(quota, /class="pricing-ledger"/);
  assert.match(quota, /<n-data-table/);
  assert.match(quota, /formatPricingRate/);
  assert.match(quota, /t\("官方倍率"\)/);
  assert.match(quota, /NInputNumber/);
  assert.match(quota, /emit\("save-multiplier"/);
  assert.doesNotMatch(quota, /5 小时额度|周额度|月额度|月费|monthlyPriceUsd|monthlyCreditsUsd|model_allowance|rollingLimitsUsd/);
  assert.doesNotMatch(quota, /<table|goat-pricing-summary|goat-pricing-table-wrap/);
  assert.doesNotMatch(quota, /provider-usage|used|remaining|percentage/);
});
