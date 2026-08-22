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

export interface ProviderCatalogFormField {
  id: string;
  kind: "text" | "secret" | "date" | "acknowledgement" | "url" | "select" | "models";
  required: boolean;
  immutable_after_create: boolean;
}

export interface ProviderCatalogRiskNotice {
  acknowledgement_id: string;
  version: string;
  source_url: string;
  body: string;
  content_hash: string;
}

export interface ProviderCatalogEntry {
  provider_id: string;
  offering_id: string;
  display_name: string;
  display_family: string;
  credential_kind: AccountCredentialKind;
  quota_scope: AccountQuotaScope;
  singleton: boolean;
  creation_availability: "available" | "unavailable";
  creation_unavailable_reason?: string | null;
  verification_policy: "not_required" | "required";
  verification_runtime_availability: "optional" | "unavailable" | "not_applicable" | "available";
  routable: boolean;
  managed_registration: boolean;
  pricing_availability: "available" | "unavailable" | "not_applicable" | "unpriced";
  usage_availability: "available" | "unavailable" | "local_state";
  manual_usage_calibration: boolean;
  quota_unit: string;
  model_source: string;
  key_prefix?: string | null;
  auth_schemes: ("bearer" | "x-api-key")[];
  upstream_protocols: ("chat_completions" | "responses" | "messages")[];
  form_fields: ProviderCatalogFormField[];
  risk_notice?: ProviderCatalogRiskNotice | null;
  model_aliases: string[];
}

export type ProviderProtocol = "chat_completions" | "responses" | "messages";

export interface ProviderModelCapability {
  model_id: string;
  provider_id: string;
  offering_id: string;
  preferred_protocol: ProviderProtocol;
  supported_protocols: ProviderProtocol[];
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
  availability: "available" | "unavailable" | "not_applicable" | "unpriced";
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
  /** Settings revision guard; omit only when no revision has been loaded. */
  expected_revision?: number;
}

export interface ProviderSettingsResponse {
  account: Account;
  revision: number;
}

export interface ZenFreeModelEntry {
  model_id: string;
  alias: string;
}

export interface ZenFreeModelsResponse {
  account_id: string;
  models: ZenFreeModelEntry[];
  refreshed_at: string | null;
  source_url: string;
}

export type ContractScopeKind = "provider" | "custom_endpoint";
export type ContractEvidenceSource = "static" | "preset" | "probe_confirmed" | "probe_observed";
export type ProbeResultKind = "success" | "failure";
export type ConnectionVerificationStatus = "not_required" | "pending" | "verified" | "failed";

export interface ProtocolSwitches {
  chat_completions: boolean;
  responses: boolean;
  messages: boolean;
}

export interface EffectiveCatalog {
  source: string;
  source_url: string;
  refreshed_at: string | null;
  models: string[];
  refresh_supported: boolean;
}

export interface EffectiveProtocolEvidence {
  protocol: ProviderProtocol;
  available: boolean;
  enabled: boolean;
  source: ContractEvidenceSource;
  verified_at: string | null;
  observed_at: string | null;
  last_probe_result: ProbeResultKind | null;
  last_probe_at: string | null;
  last_probe_error: string | null;
}

export interface EffectiveModelContract {
  model_id: string;
  preferred_protocol: ProviderProtocol;
  protocols: Record<string, EffectiveProtocolEvidence>;
  routable: boolean;
  disabled_reasons: string[];
}

export interface ProviderAccountChoice {
  id: string;
  name: string;
  enabled: boolean;
  verification_status: ConnectionVerificationStatus;
}

export interface ProviderOfferingChoice {
  offering_id: string;
  display_name: string;
  routable: boolean;
  accounts: ProviderAccountChoice[];
}

export interface CapabilitySummary {
  availability: string;
}

export interface CardCapabilitySummary {
  fetch_zen_models: boolean;
  discover_models: boolean;
  protocol_probe: boolean;
  catalog_refresh: boolean;
}

export interface ProviderContractGroup {
  scope_kind: ContractScopeKind;
  scope_id: string;
  provider_id: string;
  offerings: ProviderOfferingChoice[];
  catalog: EffectiveCatalog;
  models: EffectiveModelContract[];
  protocols: ProtocolSwitches;
  pricing: CapabilitySummary;
  usage: CapabilitySummary;
  card: CardCapabilitySummary;
  catalog_routable: boolean;
  production_inference: boolean;
  disabled_reasons: string[];
  revision: number;
}

export interface CustomEndpointContract {
  scope_kind: ContractScopeKind;
  scope_id: string;
  provider_id: string;
  account: ProviderAccountChoice;
  catalog: EffectiveCatalog;
  models: EffectiveModelContract[];
  protocols: ProtocolSwitches;
  pricing: CapabilitySummary;
  usage: CapabilitySummary;
  card: CardCapabilitySummary;
  catalog_routable: boolean;
  production_inference: boolean;
  disabled_reasons: string[];
  revision: number;
}

export interface ProviderContractsResponse {
  /** Shared settings revision for PUT `expected_revision`. Distinct from each scope `revision`. */
  revision: number;
  providers: ProviderContractGroup[];
  custom_endpoints: CustomEndpointContract[];
}

export interface ProtocolSwitchUpdate {
  enabled: boolean;
  expected_revision?: number;
}

export interface ProtocolProbeRequest {
  model_id: string;
  protocols: ProviderProtocol[];
}

export interface ProtocolProbeResult {
  protocol: ProviderProtocol;
  success: boolean;
  skipped: boolean;
  error: string | null;
}

export interface ProtocolProbeResponse {
  account_id: string;
  model_id: string;
  results: ProtocolProbeResult[];
  contract: EffectiveModelContract | null;
}

export interface CustomCatalogRefreshResponse {
  scope_kind: ContractScopeKind;
  scope_id: string;
  models: string[];
  truncated: boolean;
  refreshed_at: string;
  source: string;
  declared_capabilities_unchanged: boolean;
}

export type ProviderModelsRefreshResponse = ZenFreeModelsResponse | CustomCatalogRefreshResponse;

export function isCustomCatalogRefreshResponse(
  value: ProviderModelsRefreshResponse,
): value is CustomCatalogRefreshResponse {
  return "scope_kind" in value && "truncated" in value;
}

export const providerApi = {
  getProviderCatalog: () => request<ProviderCatalogEntry[]>("/providers/catalog"),
  getProviderModelCapabilities: () =>
    request<ProviderModelCapability[]>("/providers/model-capabilities"),
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
  getProviderModels: (accountId: string) =>
    request<ZenFreeModelsResponse>(
      `/accounts/${encodeURIComponent(accountId)}/provider-models`,
    ),
  refreshProviderModels: (accountId: string) =>
    request<ProviderModelsRefreshResponse>(
      `/accounts/${encodeURIComponent(accountId)}/provider-models/refresh`,
      { method: "POST" },
    ),
  getProviderContracts: () => request<ProviderContractsResponse>("/provider-contracts"),
  updateProviderContractProtocol: (
    scopeKind: ContractScopeKind,
    scopeId: string,
    protocol: ProviderProtocol,
    update: ProtocolSwitchUpdate,
  ) => request<ProviderContractsResponse>(
    `/provider-contracts/${encodeURIComponent(scopeKind)}/${encodeURIComponent(scopeId)}/protocols/${encodeURIComponent(protocol)}`,
    { method: "PUT", body: jsonBody(update) },
  ),
  runProtocolProbes: (accountId: string, input: ProtocolProbeRequest) =>
    request<ProtocolProbeResponse>(
      `/accounts/${encodeURIComponent(accountId)}/protocol-probes`,
      { method: "POST", body: jsonBody(input) },
    ),
};
