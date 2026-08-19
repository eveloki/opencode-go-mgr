import { jsonBody, request } from "./http.ts";
import type {
  Account,
  AccountCredentialKind,
  AccountQuotaScope,
} from "./tauri.ts";

/**
 * Typed wrappers for the provider-scoped dashboard endpoints. These live
 * outside `tauri.ts` so provider catalog/pricing/usage/settings calls share
 * the `http.ts` transport without growing the legacy account surface; Zen
 * provider settings must go through `updateProviderSettings`, never the
 * generic account PATCH.
 */

export interface ProviderCatalogEntry {
  provider_id: string;
  offering_id: string;
  credential_kind: AccountCredentialKind;
  quota_scope: AccountQuotaScope;
  singleton: boolean;
  pricing_availability: string;
  usage_availability: string;
}

export interface StoredProviderPricingSnapshot {
  provider_id: string;
  offering_id: string;
  revision: string;
  activated_at: string;
  document_updated_at: string | null;
  source_url: string;
  content_hash: string;
  snapshot_json: string;
}

export interface ProviderPricingResponse {
  provider_id: string;
  offering_id: string;
  availability: string;
  snapshot?: StoredProviderPricingSnapshot;
}

export interface ProviderQuotaWindow {
  account_id: string;
  window_kind: string;
  used: number;
  limit_value: number | null;
  started_at: string | null;
  resets_at: string | null;
  calibration_offset: number;
  unit: string;
  source: string;
  observed_at: string | null;
  updated_at: string;
}

export interface ProviderCreditBalance {
  account_id: string;
  balance_kind: string;
  amount: number;
  unit: string;
  source: string;
  observed_at: string | null;
  updated_at: string;
}

export interface ProviderUsageSyncState {
  last_success_at: string | null;
  last_attempt_at: string | null;
  next_eligible_at: string | null;
  failure_streak: number;
  last_expedited_at: string | null;
}

export interface ProviderUsageResponse {
  account_id: string;
  provider_id: string;
  offering_id: string;
  availability: string;
  quota_windows: ProviderQuotaWindow[];
  credit_balances: ProviderCreditBalance[];
  sync_state: ProviderUsageSyncState | null;
}

export interface ProviderSettingsUpdate {
  enabled: boolean;
  free_alias_enabled: boolean;
  /** Settings revision guard; omit only when no revision has been loaded. */
  expected_revision?: number;
}

export interface ProviderSettingsResponse {
  account: Account;
  revision: number;
}

export const providerApi = {
  getProviderCatalog: () => request<ProviderCatalogEntry[]>("/providers/catalog"),
  getProviderPricing: (providerId: string, offeringId: string) =>
    request<ProviderPricingResponse>(
      `/providers/${encodeURIComponent(providerId)}/${encodeURIComponent(offeringId)}/pricing`,
    ),
  getProviderUsage: (accountId: string) =>
    request<ProviderUsageResponse>(`/accounts/${encodeURIComponent(accountId)}/provider-usage`),
  updateProviderSettings: (accountId: string, update: ProviderSettingsUpdate) =>
    request<ProviderSettingsResponse>(`/accounts/${encodeURIComponent(accountId)}/provider-settings`, {
      method: "PATCH",
      body: jsonBody(update),
    }),
};
