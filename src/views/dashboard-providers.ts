import type { Account } from "../api/tauri.ts";
import type { ProviderCatalogEntry } from "../api/providers.ts";
import { planLabel } from "./plans.ts";

export type ProviderCostState = "known" | "free" | "unknown";

export interface ProviderOverview {
  key: string;
  provider_id: string;
  offering_id: string;
  label: string;
  routable: boolean;
  total: number;
  enabled: number;
  healthy: number;
  cost: number | null;
  cost_state: ProviderCostState;
}

export function providerPairKey(providerId: string, offeringId: string): string {
  return `${providerId}/${offeringId}`;
}

function cooldownActive(until: string | null, now: number): boolean {
  if (!until) return false;
  const parsed = Date.parse(until);
  return Number.isFinite(parsed) && parsed > now;
}

// Each provider family only honors the cooldown fields its runtime writes:
// Go uses the generic/legacy plus per-window fields (never the Zen free lane),
// Zen free uses generic/legacy plus its shared egress-IP lane cooldown.
const GO_COOLDOWN_FIELDS = [
  "cooldown_until",
  "cooldown_generic_until",
  "cooldown_5h_until",
  "cooldown_week_until",
  "cooldown_month_until",
] as const;
const ZEN_FREE_COOLDOWN_FIELDS = [
  "cooldown_until",
  "cooldown_generic_until",
  "cooldown_free_until",
] as const;

export function providerAccountHealthy(account: Account, now: number): boolean {
  if (
    !account.enabled
    || account.setup_step !== "ready"
    || account.auth_error
    || !account.plan_routable
    || account.verification_status === "pending"
    || account.verification_status === "failed"
  ) {
    return false;
  }
  if (account.provider_id === "opencode" && account.offering_id === "go") {
    return account.credential_kind === "api_key"
      && !GO_COOLDOWN_FIELDS.some((field) => cooldownActive(account[field], now));
  }
  if (account.provider_id === "opencode-zen-free" && account.offering_id === "anonymous-free") {
    return account.credential_kind === "none"
      && !ZEN_FREE_COOLDOWN_FIELDS.some((field) => cooldownActive(account[field], now));
  }
  return !["cooldown_until", "cooldown_generic_until"].some((field) => (
    cooldownActive(account[field as "cooldown_until" | "cooldown_generic_until"], now)
  ));
}

export function buildProviderOverviews(
  accounts: readonly Account[],
  catalog: readonly ProviderCatalogEntry[],
  costs: Readonly<Record<string, number | null | undefined>>,
  now: number,
): ProviderOverview[] {
  return catalog.map((entry) => {
    const key = providerPairKey(entry.provider_id, entry.offering_id);
    const matching = accounts.filter((account) => (
      account.provider_id === entry.provider_id && account.offering_id === entry.offering_id
    ));
    const isFree = entry.provider_id === "opencode-zen-free"
      && entry.offering_id === "anonymous-free";
    const isPricedGo = entry.provider_id === "opencode" && entry.offering_id === "go";
    return {
      key,
      provider_id: entry.provider_id,
      offering_id: entry.offering_id,
      label: entry.display_name.trim()
        || planLabel({ provider_id: entry.provider_id, offering_id: entry.offering_id }, catalog),
      routable: entry.routable,
      total: matching.length,
      enabled: matching.filter((account) => account.enabled).length,
      healthy: matching.filter((account) => providerAccountHealthy(account, now)).length,
      cost: isFree ? 0 : isPricedGo ? (costs[key] ?? null) : null,
      cost_state: isFree ? "free" : isPricedGo && typeof costs[key] === "number" ? "known" : "unknown",
    };
  });
}
