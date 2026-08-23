import type { PricingSnapshot } from "../api/dashboard";
import type { MessageKey } from "../i18n/index.ts";
import type {
  ProviderCatalogEntry,
  ProviderPricingResponse,
  StoredProviderPricingSnapshot,
} from "../api/providers.ts";
import {
  PLAN_DEFINITIONS,
  type PlanDefinition,
  type PlanId,
  findCatalogEntry,
  planFamilyLabel,
} from "./plans.ts";

export type PricingAvailability = "available" | "unavailable" | "not_applicable" | "unpriced";

/**
 * The pricing content for a single plan family. The shape is deliberately
 * kind-tagged so the component can render plan-specific notes without guessing
 * about the backend snapshot format.
 */
export type PlanPricingContent =
  | { kind: "opencode-go"; snapshot: PricingSnapshot | null }
  | { kind: "goat-reference"; snapshot: null }
  | { kind: "scnet-reference"; snapshot: null }
  | { kind: "free"; snapshot: null }
  | { kind: "api-key"; snapshot: StoredProviderPricingSnapshot | null }
  | { kind: "subscription"; snapshot: StoredProviderPricingSnapshot | null }
  | { kind: "custom"; snapshot: StoredProviderPricingSnapshot | null };

export interface PlanPricingGroup {
  plan: PlanDefinition;
  label: string;
  pricingAvailability: PricingAvailability;
  content: PlanPricingContent;
}

export type PlanPricingState =
  | "error"
  | "reference"
  | "unavailable"
  | "unpriced"
  | "not_applicable"
  | "available-empty"
  | "available-table";

export interface PlanPricingDisplay {
  state: PlanPricingState;
  messageKey: MessageKey;
  error: string | null;
}

/**
 * Per-plan provider-pricing responses fetched from `/providers/{provider}/{offering}/pricing`.
 * Plans that have not been fetched (or whose fetch failed) are simply absent.
 */
export type ProviderSnapshots = Partial<Record<PlanId, ProviderPricingResponse>>;

export const PRICING_PLAN_IDS = [
  "opencode-go",
  "command-code-goat",
  "scnet",
] as const satisfies readonly PlanId[];

const pricingPlanIdSet = new Set<PlanId>(PRICING_PLAN_IDS);

export const PRICING_PLAN_DEFINITIONS = PLAN_DEFINITIONS.filter(
  (plan) => pricingPlanIdSet.has(plan.id),
);

function defaultPricingAvailability(plan: PlanDefinition): PricingAvailability {
  if (plan.id === "opencode-go") return "available";
  if (plan.id === "zen-free") return "not_applicable";
  if (plan.id === "custom-endpoint") return "unpriced";
  return "unavailable";
}

function buildContent(
  plan: PlanDefinition,
  pricingAvailability: PricingAvailability,
  opencodeSnapshot: PricingSnapshot | null,
  providerSnapshots: ProviderSnapshots,
): PlanPricingContent {
  if (plan.id === "opencode-go") {
    return { kind: "opencode-go", snapshot: opencodeSnapshot };
  }

  if (plan.id === "command-code-goat") {
    return { kind: "goat-reference", snapshot: null };
  }

  if (plan.id === "scnet") {
    return { kind: "scnet-reference", snapshot: null };
  }

  if (plan.id === "zen-free") {
    return { kind: "free", snapshot: null };
  }

  const response = providerSnapshots[plan.id];
  const snapshot = pricingAvailability === "available" ? (response?.snapshot ?? null) : null;

  switch (plan.kind) {
    case "api-key":
      return { kind: "api-key", snapshot };
    case "subscription":
      return { kind: "subscription", snapshot };
    case "custom":
      return { kind: "custom", snapshot };
    default:
      return { kind: "custom", snapshot };
  }
}

function hasPricingTable(group: PlanPricingGroup): boolean {
  if (group.content.kind === "opencode-go") {
    return Boolean(group.content.snapshot?.models.length);
  }
  return group.content.snapshot !== null;
}

/**
 * One exhaustive pricing presentation state. Keeping this pure prevents a
 * template branch from accidentally making an unavailable or empty plan look
 * like a populated zero-price table.
 */
export function resolvePlanPricingDisplay(
  group: PlanPricingGroup,
  error: string | null = null,
): PlanPricingDisplay {
  if (error) {
    return { state: "error", messageKey: "加载额度价格表失败: {error}", error };
  }
  if (group.content.kind === "goat-reference" || group.content.kind === "scnet-reference") {
    return {
      state: "reference",
      messageKey: "以下为官方套餐参考，不是 OCG Manager 实时计价或用量。",
      error: null,
    };
  }
  if (group.pricingAvailability === "unavailable") {
    const messageKey = group.content.kind === "api-key"
      ? "实验性接入，尚未配置价格目录，不展示价格表。"
      : group.content.kind === "subscription"
        ? "订阅制方案：额度、计费与续费由服务商订阅条款管理。"
        : group.content.kind === "custom"
          ? "自定义端点由你自行维护，Gateway 无法验证其价格、额度与协议兼容性。"
          : "暂无该方案的价格数据";
    return { state: "unavailable", messageKey, error: null };
  }
  if (group.pricingAvailability === "unpriced") {
    return {
      state: "unpriced",
      messageKey: group.content.kind === "custom"
        ? "自定义端点由你自行维护，Gateway 无法验证其价格、额度与协议兼容性。"
        : "该方案未定价",
      error: null,
    };
  }
  if (group.pricingAvailability === "not_applicable") {
    return {
      state: "not_applicable",
      messageKey: group.content.kind === "free"
        ? "零价格；额度按出口 IP 共享，429 后整条 free 通道冷却。"
        : "该方案无需价格表",
      error: null,
    };
  }
  if (!hasPricingTable(group)) {
    return { state: "available-empty", messageKey: "暂无该方案的价格数据", error: null };
  }
  return {
    state: "available-table",
    messageKey: group.content.kind === "opencode-go"
      ? "只在你主动刷新时访问官方文档；刷新失败会继续使用当前快照。"
      : "未知价格不会参与费用估算",
    error: null,
  };
}

/**
 * Groups the pricing page by the three paid plans users compare here. Zen Free
 * has no price and Custom API pricing belongs to its administrator, so neither
 * appears in Pricing even though both remain in the shared Plan registry.
 *
 * The OpenCode Go group is always rendered when a Go pricing snapshot has been
 * fetched, even if the provider catalog is still loading, failed, or empty.
 * Other families rely on the catalog for their `pricing_availability` and,
 * when applicable, on the per-family provider-pricing response.
 */
function groupForPlan(
  plan: PlanDefinition,
  catalog: readonly ProviderCatalogEntry[] | null | undefined,
  opencodeSnapshot: PricingSnapshot | null,
  providerSnapshots: ProviderSnapshots,
): PlanPricingGroup {
  // For families with multiple offerings (SCNet), the catalog should list all
  // of them with the same pricing_availability; read the first present entry.
  const entry = plan.offering_ids
    .map((offeringId) => findCatalogEntry(catalog, plan.provider_id, offeringId))
    .find(Boolean);
  const response = providerSnapshots[plan.id];
  const pricingAvailability = response?.availability
    ?? entry?.pricing_availability
    ?? defaultPricingAvailability(plan);
  const label = planFamilyLabel(plan, catalog);

  return {
    plan,
    label,
    pricingAvailability,
    content: buildContent(plan, pricingAvailability, opencodeSnapshot, providerSnapshots),
  };
}

export function buildPlanPricingGroups(
  catalog: readonly ProviderCatalogEntry[] | null | undefined,
  opencodeSnapshot: PricingSnapshot | null,
  providerSnapshots: ProviderSnapshots,
): PlanPricingGroup[] {
  return PRICING_PLAN_DEFINITIONS.map((plan) => (
    groupForPlan(plan, catalog, opencodeSnapshot, providerSnapshots)
  ));
}

/** Pricing groups for a single provider family, including Zen Free and Custom. */
export function buildScopedPlanPricingGroups(
  providerId: string,
  catalog: readonly ProviderCatalogEntry[] | null | undefined,
  opencodeSnapshot: PricingSnapshot | null,
  providerSnapshots: ProviderSnapshots,
): PlanPricingGroup[] {
  return PLAN_DEFINITIONS
    .filter((plan) => plan.provider_id === providerId)
    .map((plan) => groupForPlan(plan, catalog, opencodeSnapshot, providerSnapshots));
}
