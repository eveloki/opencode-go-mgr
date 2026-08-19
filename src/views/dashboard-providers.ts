import type { Account } from "../api/tauri.ts";
import type { ProviderCatalogEntry } from "../api/providers.ts";
import { findProviderOffering } from "./account-providers.ts";

export type ProviderCostState = "known" | "free" | "unknown";

export interface ProviderOverview {
  key: string;
  provider_id: string;
  offering_id: string;
  label: string;
  total: number;
  enabled: number;
  healthy: number;
  cost: number | null;
  cost_state: ProviderCostState;
}

export function providerPairKey(providerId: string, offeringId: string): string {
  return `${providerId}/${offeringId}`;
}

function accountCooling(account: Account, now: number): boolean {
  if (!account.cooldown_until) return false;
  const until = Date.parse(account.cooldown_until);
  return Number.isFinite(until) && until > now;
}

export function providerAccountHealthy(account: Account, now: number): boolean {
  if (!account.enabled || account.setup_step !== "ready" || account.auth_error || accountCooling(account, now)) {
    return false;
  }
  if (account.provider_id === "opencode" && account.offering_id === "go") {
    return account.credential_kind === "api_key";
  }
  if (account.provider_id === "opencode-zen-free" && account.offering_id === "anonymous-free") {
    return account.credential_kind === "none";
  }
  // GOAT and unknown offerings stay fail-closed until a verified runtime
  // contract is configured.
  return false;
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
      label: findProviderOffering(entry.provider_id, entry.offering_id)?.label ?? key,
      total: matching.length,
      enabled: matching.filter((account) => account.enabled).length,
      healthy: matching.filter((account) => providerAccountHealthy(account, now)).length,
      cost: isFree ? 0 : isPricedGo ? (costs[key] ?? null) : null,
      cost_state: isFree ? "free" : isPricedGo && typeof costs[key] === "number" ? "known" : "unknown",
    };
  });
}
