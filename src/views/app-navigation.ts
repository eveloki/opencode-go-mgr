export const APP_VIEW_KEYS = [
  "dashboard",
  "keys",
  "accounts",
  "providers",
  "apps",
  "logs",
  "settings",
  "browser",
] as const;

export type AppViewKey = (typeof APP_VIEW_KEYS)[number];

export const LEGACY_PRICING_VIEW = "pricing";
export const PROVIDERS_VIEW: AppViewKey = "providers";

const viewKeySet = new Set<string>(APP_VIEW_KEYS);

export interface ProviderScopeQuery {
  scope_kind?: string;
  scope_id?: string;
}

export function isLegacyPricingView(raw: string | null | undefined): boolean {
  return raw === LEGACY_PRICING_VIEW;
}

export function resolveAppViewKey(raw: string | null | undefined): AppViewKey {
  if (!raw) return "dashboard";
  if (isLegacyPricingView(raw) || raw === PROVIDERS_VIEW) return "providers";
  return viewKeySet.has(raw) ? raw as AppViewKey : "dashboard";
}

export function readProviderScopeQuery(search: string): {
  scope_kind: string | null;
  scope_id: string | null;
} {
  const params = new URLSearchParams(search.startsWith("?") ? search.slice(1) : search);
  return {
    scope_kind: params.get("scope_kind"),
    scope_id: params.get("scope_id"),
  };
}

export function applyAppViewSearchParams(
  url: URL,
  view: AppViewKey,
  scope?: ProviderScopeQuery | null,
): URL {
  url.searchParams.set("view", view);
  if (view !== "accounts") url.searchParams.delete("account_id");
  if (view !== "providers") {
    url.searchParams.delete("scope_kind");
    url.searchParams.delete("scope_id");
    return url;
  }
  if (scope === undefined) return url;
  if (scope === null) {
    url.searchParams.delete("scope_kind");
    url.searchParams.delete("scope_id");
    return url;
  }
  if (scope.scope_kind) url.searchParams.set("scope_kind", scope.scope_kind);
  else url.searchParams.delete("scope_kind");
  if (scope.scope_id) url.searchParams.set("scope_id", scope.scope_id);
  else url.searchParams.delete("scope_id");
  return url;
}
