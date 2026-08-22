export const PRICING_REFERENCE_CHECKED_AT = "2026-08-22";

export const GOAT_PRICING_REFERENCE = {
  sourceUrl: "https://commandcode.ai/docs/resources/pricing-limits",
  pricingUrl: "https://commandcode.ai/pricing",
  monthlyPriceUsd: 10,
  monthlyCreditsUsd: 70,
  approximateRequests: "75K",
  rollingLimits: [
    { window: "5 小时", creditsUsd: 14 },
    { window: "7 天", creditsUsd: 35 },
  ],
} as const;

export const SCNET_PRICING_REFERENCE = {
  sourceUrl: "https://www.scnet.cn/ac/openapi/doc/2.0/moduleapi/plans/token-plan.html",
  restrictionsUrl: "https://www.scnet.cn/ac/openapi/doc/2.0/moduleapi/plans/faq.html",
  tiers: [
    { id: "basic", label: "基础版", listPriceCny: 50, promotionalPriceCny: 30, monthlyCredits: 60_000 },
    { id: "standard", label: "标准版", listPriceCny: 185, promotionalPriceCny: 110, monthlyCredits: 240_000 },
    { id: "premium", label: "高级版", listPriceCny: 440, promotionalPriceCny: 265, monthlyCredits: 600_000 },
  ],
} as const;
