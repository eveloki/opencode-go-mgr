import { apiBase, jsonBody, request } from "./http.ts";

// Compatibility re-exports: the transport layer lives in `./http.ts` and the
// version helper in `../utils/version.ts`, but existing consumers (including
// tests) keep importing them from here.
export {
  DASHBOARD_AUTH_REQUIRED_EVENT,
  DashboardAuthError,
  DashboardRequestError,
} from "./http.ts";
export { isVersionAtLeast } from "../utils/version.ts";

export type AccountCredentialKind = "api_key" | "none";
export type AccountQuotaScope = "key" | "egress-ip";

export interface Account {
  id: string;
  name: string;
  username: string;
  password: string;
  key: string;
  enabled: boolean;
  account_type: AccountType;
  setup_step: AccountSetupStep;
  provider_id: string;
  offering_id: string;
  credential_kind: AccountCredentialKind;
  quota_scope: AccountQuotaScope;
  free_alias_enabled: boolean;
  /** Shared control-plane revision for optimistic account/settings writes. */
  revision?: number;
  purchase_date: string;
  expires_on: string;
  cooldown_until: string | null;
  cooldown_generic_until: string | null;
  cooldown_5h_until: string | null;
  cooldown_week_until: string | null;
  cooldown_month_until: string | null;
  cooldown_free_until: string | null;
  last_error: string | null;
  auth_error: string | null;
  notes: string;
  /** Last successful official Go usage calibration (RFC3339), if any. */
  usage_sync_last_success_at: string | null;
  /** When a manual refresh may be attempted again; null when allowed now. */
  usage_sync_next_allowed_at: string | null;
  created_at: string;
  updated_at: string;
}

export type AccountType = "key" | "managed";

export type AccountSetupStep =
  | "google_account"
  | "opencode_registration"
  | "payment"
  | "key_verification"
  | "ready";

export interface AccountInput {
  name: string;
  username?: string;
  password?: string;
  key: string;
  /** Defaults to the built-in OpenCode Go pair when omitted. */
  provider_id?: string;
  offering_id?: string;
  purchase_date?: string;
  notes?: string;
  expected_revision?: number;
}

export interface AccountUpdate {
  name?: string;
  username?: string;
  password?: string;
  key?: string;
  enabled?: boolean;
  purchase_date?: string;
  notes?: string;
  expected_revision?: number;
}

export interface ManagedAccountInput {
  name: string;
  username?: string;
  notes?: string;
  expected_revision?: number;
}

export type RoutingMode = "strict-priority" | "sticky-global" | "round-robin";
export type FreeModelRouting = "deny" | "explicit" | "prefer";
export type ProxyMode = "auto" | "manual" | "direct";

/** Fixed attribution id of the primary key; mirrors the backend constant. */
export const PRIMARY_KEY_ID = "00000000-0000-0000-0000-000000000001";

/** One database-owned sub key as returned by the key lifecycle API. */
export interface SubGatewayKey {
  id: string;
  name: string;
  key: string;
  enabled: boolean;
  deleted_at: string | null;
  created_at: string;
}

export interface AppConfig {
  revision: number;
  gateway_port: number;
  gateway_key: string;
  upstream_base_url: string;
  proxy_mode: ProxyMode;
  proxy_url: string;
  opencode_invite_url: string;
  client_root_url: string;
  client_root_url_from_env: boolean;
  auto_start: boolean;
  auto_start_supported: boolean;
  show_dock_icon: boolean;
  dock_visibility_supported: boolean;
  connect_timeout_secs: number;
  non_stream_timeout_secs: number;
  stream_idle_timeout_secs: number;
  routing_mode: RoutingMode;
  conversation_sticky: boolean;
  free_model_routing: FreeModelRouting;
}

/** Sub key entry in the lightweight connection payload. */
export interface ConnectionSubKey {
  id: string;
  name: string;
  enabled: boolean;
  value: string;
}

/**
 * Aggregated connection view for the connection center: the primary key
 * value, non-deleted sub keys with values, the settings revision, and URL
 * fields. Plaintext sits behind the dashboard session layer.
 */
export interface ConnectionInfo {
  gateway_port: number;
  client_root_url: string;
  upstream_base_url: string;
  primary_key: string;
  sub_keys: ConnectionSubKey[];
  revision: number;
}

export type BrowserMode = "native" | "remote" | "unsupported";
export type BrowserTarget =
  | "google_signup"
  | "google_login"
  | "github_signup"
  | "github_login"
  | "invite"
  | "console";

export interface BrowserCapabilities {
  mode: BrowserMode;
  reason?: string | null;
}

export interface BrowserLaunchResult {
  mode: Exclude<BrowserMode, "unsupported">;
  session_token?: string | null;
}

export interface UpdateCheckResult {
  current_version: string;
  latest_version: string;
  update_available: boolean;
  install_supported: boolean;
  release_url: string;
}

export interface ProxyTestResult {
  proxy_mode: ProxyMode;
  status: number;
  latency_ms: number;
}

export type UpdatePhase = "idle" | "checking" | "downloading" | "installing" | "failed";

export interface UpdateStatus {
  phase: UpdatePhase;
  downloaded: number;
  total: number | null;
  error: string | null;
  current_version: string;
  install_supported: boolean;
}

export interface ClaudeDesktopModels {
  sonnet: string;
  opus: string;
  haiku: string;
}

export interface GatewayLog {
  id: number;
  level: string;
  category: string;
  message: string;
  created_at: string;
  request_id?: string | null;
  attempt?: number | null;
  error_source?: string | null;
  error_stage?: string | null;
  duration_ms?: number | null;
  diagnostic?: ErrorDiagnostic | null;
}

export interface ErrorDiagnostic {
  version: number;
  request_id: string;
  attempt: number;
  error_source: string;
  error_stage: string;
  client_format: string;
  upstream_format?: string | null;
  model?: string | null;
  stream?: boolean | null;
  client_body_bytes?: number | null;
  upstream_body_bytes?: number | null;
  duration_ms: number;
  upstream_wait_ms?: number | null;
  downstream_status?: number | null;
  upstream_status?: number | null;
  retry_action?: string | null;
  upstream_headers?: Record<string, string> | null;
  request_summary?: unknown;
  request_fingerprint?: string | null;
  upstream_error?: unknown;
  truncated: boolean;
}

export interface ForwardLog {
  id: number;
  timestamp: string;
  model: string;
  account_id: string;
  account_name: string;
  client_key_id?: string | null;
  client_key_name?: string | null;
  /** Routing/provider attribution; all null for rows predating provider-aware logging. */
  route_account_id?: string | null;
  provider_id?: string | null;
  offering_id?: string | null;
  credential_account_id?: string | null;
  raw_cost_usd?: number | null;
  quota_debit?: number | null;
  effective_paid_cost_usd?: number | null;
  status: string;
  http_status: number | null;
  prompt_tokens: number;
  completion_tokens: number;
  cached_tokens: number;
  cache_creation_tokens: number;
  cost: number | null;
  cost_state?: string | null;
  pricing_revision_id?: string | null;
  quota_multiplier?: number | null;
  local_adjustment_multiplier?: number | null;
  service_tier?: string | null;
  error_message: string | null;
  request_id?: string | null;
  attempt?: number | null;
  error_source?: string | null;
  error_stage?: string | null;
  duration_ms?: number | null;
  diagnostic?: ErrorDiagnostic | null;
}

export interface ForwardLogSummary {
  total_requests: number;
  prompt_tokens: number;
  completion_tokens: number;
  cached_tokens: number;
  cost: number;
}

export interface ForwardLogPage {
  items: ForwardLog[];
  summary: ForwardLogSummary;
}

export interface ForwardLogQuery {
  limit?: number;
  offset?: number;
  status?: string | null;
  account_id?: string | null;
  model?: string | null;
  request_id?: string | null;
  key_id?: string | null;
  provider_id?: string | null;
  offering_id?: string | null;
  route_account_id?: string | null;
  credential_account_id?: string | null;
  start_time?: string | null;
  end_time?: string | null;
  sort_by?: string | null;
  sort_order?: string | null;
}

/** Sentinel selecting forward logs without client key attribution. */
export const UNATTRIBUTED_KEY_FILTER = "__unattributed__";

export interface ForwardLogClientKey {
  id: string;
  name: string;
}

export interface SubGatewayKeyResponse extends SubGatewayKey {
  revision: number;
}

/** Summary entry in list-shaped lifecycle responses; plaintext is omitted. */
export interface SubGatewayKeySummary {
  id: string;
  name: string;
  enabled: boolean;
}

export interface SubGatewayKeyRevisionResponse {
  revision: number;
  keys: SubGatewayKeySummary[];
}

export interface UsageWindow {
  account_id: string;
  window_5h: number;
  window_week: number;
  window_month: number;
  /** 5h 固定窗口的清零时刻（RFC3339）；null 表示窗口尚未开始（无成功请求）。 */
  resets_in_5h: string | null;
  /** 周固定窗口的清零时刻；null 表示窗口尚未开始。 */
  resets_in_week: string | null;
  /** 月窗口的到期时刻（purchase_date + 1 自然月）；null 表示账号无购买日期。 */
  resets_in_month: string | null;
}

export interface OfficialUsageRefreshResult {
  usage: UsageWindow;
  source: string;
  last_success_at: string;
  next_allowed_at: string;
}

export interface PricingLimits {
  window_5h: number;
  window_week: number;
  window_month: number;
}

export interface PricingAdjustment {
  label: string;
  multiplier: number;
  applies_to: string;
}

export interface PricingModel {
  model_id: string;
  display_name: string;
  input: number;
  output: number;
  cache_read: number | null;
  cache_write: number | null;
  usage: number;
  quota_multiplier: number;
  min_input_tokens?: number | null;
  max_input_tokens?: number | null;
  time_window?: "always" | "off_peak" | "peak" | null;
  adjustments: PricingAdjustment[];
}

export interface PricingSnapshot {
  revision: string;
  activated_at: string;
  document_updated_at: string | null;
  source_url: string;
  content_hash: string;
  adjustment_policy_version: string;
  limits: PricingLimits;
  models: PricingModel[];
}

export interface PricingRefreshResult extends PricingSnapshot {
  refresh_status: "success" | "unchanged" | "needs_confirmation" | "failed_no_change";
  multiplier_changes?: PricingMultiplierChange[];
  official_content_hash?: string;
  error?: string | null;
}

export interface PricingMultiplierChange {
  model_id: string;
  current_multiplier: number;
  official_multiplier: number;
}

export interface PricingRefreshRequest {
  policy?: "keep_current" | "use_official";
  expected_revision?: string;
  expected_official_content_hash?: string;
}

export interface PricingMultiplierUpdate {
  model_id: string;
  multiplier: number;
}

export interface DashboardSummary {
  total_accounts: number;
  available_accounts: number;
  today_cost: number;
  week_cost: number;
  month_cost: number;
  gateway_running: boolean;
}

export interface DailyModelCost {
  date: string;
  model: string;
  cost: number;
}

export interface DashboardAuthStatus {
  local: boolean;
  initialized: boolean;
  authenticated: boolean;
}

export const tauriApi = {
  getAuthStatus: () => request<DashboardAuthStatus>("/auth/status", {}, false),
  registerAdmin: (username: string, password: string) =>
    request<{ ok: boolean }>(
      "/auth/register",
      { method: "POST", body: jsonBody({ username, password }) },
      false,
    ),
  loginAdmin: (username: string, password: string) =>
    request<{ ok: boolean }>(
      "/auth/login",
      { method: "POST", body: jsonBody({ username, password }) },
      false,
    ),
  logoutAdmin: () =>
    request<void>("/auth/logout", { method: "POST" }, false),

  getAccounts: () => request<Account[]>("/accounts"),
  createAccount: (input: AccountInput) =>
    request<Account>("/accounts", { method: "POST", body: jsonBody(input) }),
  createManagedAccount: (input: ManagedAccountInput) =>
    request<Account>("/accounts/managed", { method: "POST", body: jsonBody(input) }),
  updateAccount: (id: string, update: AccountUpdate) =>
    request<Account>(`/accounts/${id}`, { method: "PATCH", body: jsonBody(update) }),
  reorderAccounts: (accountIds: string[], expectedRevision?: number | null) =>
    request<Account[]>("/accounts/order", {
      method: "PUT",
      body: jsonBody({
        account_ids: accountIds,
        ...(expectedRevision === null || expectedRevision === undefined
          ? {}
          : { expected_revision: expectedRevision }),
      }),
    }),
  deleteAccount: (id: string, expectedRevision?: number | null) => request<void>(`/accounts/${id}`, {
    method: "DELETE",
    ...(expectedRevision === null || expectedRevision === undefined
      ? {}
      : { body: jsonBody({ expected_revision: expectedRevision }) }),
  }),
  toggleAccount: (id: string, expectedRevision?: number | null) => request<Account>(`/accounts/${id}/toggle`, {
    method: "POST",
    ...(expectedRevision === null || expectedRevision === undefined
      ? {}
      : { body: jsonBody({ expected_revision: expectedRevision }) }),
  }),
  testAccount: async (id: string) => {
    const result = await request<{ message: string }>(`/accounts/${id}/test`, { method: "POST" });
    return result.message;
  },
  getAccountUsage: (id: string) => request<UsageWindow>(`/accounts/${id}/usage`),
  updateAccountUsage: (
    id: string,
    window: "window_5h" | "window_week" | "window_month",
    percent: number,
    resets_in_minutes?: number | null,
  ) => request<UsageWindow>(`/accounts/${id}/usage`, {
    method: "PATCH",
    body: jsonBody({ window, percent, resets_in_minutes: resets_in_minutes ?? null }),
  }),
  refreshAccountUsage: (id: string) =>
    request<OfficialUsageRefreshResult>(`/accounts/${id}/usage/refresh`, {
      method: "POST",
    }),
  resetAccountCooldown: (id: string, expectedRevision?: number | null) =>
    request<Account>(`/accounts/${id}/reset-cooldown`, {
      method: "POST",
      ...(expectedRevision === null || expectedRevision === undefined
        ? {}
        : { body: jsonBody({ expected_revision: expectedRevision }) }),
    }),
  advanceAccountSetup: (id: string, setupStep: AccountSetupStep, expectedRevision?: number | null) =>
    request<Account>(`/accounts/${id}/setup`, {
      method: "PATCH",
      body: jsonBody({
        setup_step: setupStep,
        ...(expectedRevision === null || expectedRevision === undefined
          ? {}
          : { expected_revision: expectedRevision }),
      }),
    }),
  verifyManagedAccountKey: (id: string, key: string, expectedRevision?: number | null) =>
    request<Account>(`/accounts/${id}/setup/verify-key`, {
      method: "POST",
      body: jsonBody({
        key,
        ...(expectedRevision === null || expectedRevision === undefined
          ? {}
          : { expected_revision: expectedRevision }),
      }),
    }),
  getBrowserCapabilities: () => request<BrowserCapabilities>("/browser/capabilities"),
  openAccountBrowser: (id: string, target: BrowserTarget) =>
    request<BrowserLaunchResult>(`/accounts/${id}/browser`, {
      method: "POST",
      body: jsonBody({ target }),
    }),
  resetAccountBrowserProfile: (id: string, expectedRevision?: number | null) =>
    request<Account>(`/accounts/${id}/browser-profile`, {
      method: "DELETE",
      ...(expectedRevision === null || expectedRevision === undefined
        ? {}
        : { body: jsonBody({ expected_revision: expectedRevision }) }),
    }),

  getSettings: () => request<AppConfig>("/settings"),
  testProxy: (input: Pick<AppConfig, "proxy_mode" | "proxy_url" | "upstream_base_url">) =>
    request<ProxyTestResult>("/settings/test-proxy", {
      method: "POST",
      body: jsonBody(input),
    }),
  getPricing: () => request<PricingSnapshot>("/pricing"),
  refreshPricing: (refresh: PricingRefreshRequest = {}) => request<PricingRefreshResult>("/pricing/refresh", {
    method: "POST",
    body: jsonBody(refresh),
  }),
  updatePricingMultipliers: (expectedRevision: string, multipliers: PricingMultiplierUpdate[]) =>
    request<PricingSnapshot>("/pricing/multipliers", {
      method: "PUT",
      body: jsonBody({ expected_revision: expectedRevision, multipliers }),
    }),
  getApplicationModels: () => request<string[]>("/application-models"),
  getClaudeDesktopModels: () => request<ClaudeDesktopModels>("/claude-desktop/models"),
  updateClaudeDesktopModels: (models: ClaudeDesktopModels) =>
    request<ClaudeDesktopModels>("/claude-desktop/models", {
      method: "PUT",
      body: jsonBody(models),
    }),
  updateSettings: (config: AppConfig) => {
    const { revision, ...settings } = config;
    return request<{ revision: number }>("/settings", {
      method: "POST",
      body: jsonBody({ ...settings, expected_revision: revision }),
    });
  },
  regenerateGatewayKey: async () => {
    return request<{ key: string; revision: number }>("/settings/regenerate-gateway-key", {
      method: "POST",
    });
  },
  getConnection: () => request<ConnectionInfo>("/connection"),
  createGatewayKey: (name: string, expectedRevision?: number) =>
    request<SubGatewayKeyResponse>("/settings/keys", {
      method: "POST",
      body: jsonBody({ name, expected_revision: expectedRevision }),
    }),
  updateGatewayKey: (
    id: string,
    update: { name?: string; enabled?: boolean },
    expectedRevision?: number,
  ) =>
    request<SubGatewayKeyRevisionResponse>(`/settings/keys/${encodeURIComponent(id)}`, {
      method: "PATCH",
      body: jsonBody({ ...update, expected_revision: expectedRevision }),
    }),
  deleteGatewayKey: (id: string, expectedRevision?: number) =>
    request<SubGatewayKeyRevisionResponse>(`/settings/keys/${encodeURIComponent(id)}`, {
      method: "DELETE",
      body: jsonBody({ expected_revision: expectedRevision }),
    }),
  regenerateGatewayKeyEntry: (id: string, expectedRevision?: number) =>
    request<SubGatewayKeyResponse>(
      `/settings/keys/${encodeURIComponent(id)}/regenerate`,
      { method: "POST", body: jsonBody({ expected_revision: expectedRevision }) },
    ),
  checkForUpdate: () => request<UpdateCheckResult>("/settings/check-update"),
  getUpdateStatus: () => request<UpdateStatus>("/settings/update-status"),
  installUpdate: (expectedVersion: string) => request<UpdateStatus>("/settings/install-update", {
    method: "POST",
    body: jsonBody({ expected_version: expectedVersion }),
  }),
  getGatewayLogs: (limit?: number, requestId?: string | null) => {
    const params = new URLSearchParams({ limit: String(limit ?? 100) });
    if (requestId) params.set("request_id", requestId);
    return request<GatewayLog[]>(`/logs/gateway?${params}`);
  },
  getForwardLogs: (query: ForwardLogQuery = {}) => {
    // Filters are set before pagination params so the exact-match filters lead
    // the query string; the backend applies them before paging anyway.
    const params = new URLSearchParams();
    if (query.status) params.set("status", query.status);
    if (query.account_id) params.set("account_id", query.account_id);
    if (query.model) params.set("model", query.model);
    if (query.request_id) params.set("request_id", query.request_id);
    if (query.key_id) params.set("key_id", query.key_id);
    if (query.provider_id) params.set("provider_id", query.provider_id);
    if (query.offering_id) params.set("offering_id", query.offering_id);
    if (query.route_account_id) params.set("route_account_id", query.route_account_id);
    if (query.credential_account_id) params.set("credential_account_id", query.credential_account_id);
    if (query.start_time) params.set("start_time", query.start_time);
    if (query.end_time) params.set("end_time", query.end_time);
    if (query.sort_by) params.set("sort_by", query.sort_by);
    if (query.sort_order) params.set("sort_order", query.sort_order);
    params.set("limit", String(query.limit ?? 20));
    params.set("offset", String(query.offset ?? 0));
    return request<ForwardLogPage>(`/logs/forward?${params}`);
  },
  getForwardLogModels: () => request<string[]>("/logs/forward/models"),
  getForwardLogKeys: () => request<ForwardLogClientKey[]>("/logs/forward/keys"),

  getDashboardSummary: () => request<DashboardSummary>("/dashboard/summary"),
  getDailyCostByModel: (days?: number) =>
    request<DailyModelCost[]>(`/dashboard/daily-cost-by-model?days=${days ?? 30}`),
};

export function browserSessionWebSocketUrl(token: string): string {
  const url = new URL(`${apiBase()}/browser/sessions/${encodeURIComponent(token)}/ws`, window.location.href);
  url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
  return url.toString();
}
