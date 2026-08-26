import type { Account } from "../api/dashboard.ts";
import type {
  CardCapabilitySummary,
  CapabilitySummary,
  ContractScopeKind,
  CustomEndpointContract,
  EffectiveCatalog,
  EffectiveModelContract,
  EffectiveProtocolEvidence,
  ProviderAccountChoice,
  ProviderCatalogEntry,
  ProviderContractGroup,
  ProviderContractsResponse,
  ProviderOfferingChoice,
  ProviderProtocol,
  ProtocolSwitches,
} from "../api/providers.ts";
import {
  findPlanDefinition,
  planFamilyLabel,
  planForAccount,
  PLAN_DEFINITIONS,
} from "./plans.ts";

export const PROVIDER_PROTOCOLS: readonly ProviderProtocol[] = [
  "chat_completions",
  "responses",
  "messages",
];

export const CATALOG_SOURCE_STATIC = "static";
export const CATALOG_SOURCE_OFFICIAL_ZEN = "official_zen";
export const CATALOG_SOURCE_CUSTOM_DISCOVERY = "custom_discovery";
export const CATALOG_SOURCE_DECLARED = "account_declared";
export const CATALOG_SOURCE_OPENCODE_MODELS = "opencode_get_models";
export const CATALOG_SOURCE_COMMAND_CODE_MODELS = "command_code_get_models";

export interface ProviderScopeRef {
  scope_kind: ContractScopeKind;
  scope_id: string;
}

export interface ProviderScopeView {
  key: string;
  scope_kind: ContractScopeKind;
  scope_id: string;
  provider_id: string;
  label: string;
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

export interface AccountContractSummary {
  scope_kind: ContractScopeKind;
  scope_id: string;
  label: string;
  enabledProtocols: ProviderProtocol[];
  allProtocolsDisabled: boolean;
  unroutable: boolean;
  disabledReasons: string[];
}

export type ProtocolEvidenceUiStatus =
  | "globally_closed"
  | "unavailable"
  | "unsupported"
  | "static"
  | "preset"
  | "probe_confirmed"
  | "probe_failure";

const EMPTY_SWITCHES: ProtocolSwitches = {
  chat_completions: true,
  responses: true,
  messages: true,
};

const EMPTY_CATALOG: EffectiveCatalog = {
  source: "",
  source_url: "",
  refreshed_at: null,
  models: [],
  refresh_supported: false,
};

const EMPTY_CARD: CardCapabilitySummary = {
  fetch_zen_models: false,
  discover_models: false,
  protocol_probe: false,
  catalog_refresh: false,
};

export function providerScopeKey(scopeKind: string, scopeId: string): string {
  return `${scopeKind}:${scopeId}`;
}

export function parseProviderScopeKey(key: string): ProviderScopeRef | null {
  const separator = key.indexOf(":");
  if (separator <= 0 || separator === key.length - 1) return null;
  const scope_kind = key.slice(0, separator);
  const scope_id = key.slice(separator + 1);
  if (scope_kind !== "provider" && scope_kind !== "custom_endpoint") return null;
  return { scope_kind, scope_id };
}

/** One scope key for an account: Custom endpoints stay per-account; others share the provider id. */
export function accountProviderScope(
  account: Pick<Account, "id" | "provider_id" | "offering_id">,
): ProviderScopeRef {
  const plan = planForAccount(account);
  if (plan?.kind === "custom") {
    return { scope_kind: "custom_endpoint", scope_id: account.id };
  }
  return { scope_kind: "provider", scope_id: account.provider_id };
}

export function protocolDisplayName(protocol: ProviderProtocol): string {
  if (protocol === "chat_completions") return "Chat Completions";
  if (protocol === "responses") return "Responses";
  return "Messages";
}

export function uniqueProtocols(protocols: readonly ProviderProtocol[]): ProviderProtocol[] {
  const seen = new Set<ProviderProtocol>();
  const unique: ProviderProtocol[] = [];
  for (const protocol of protocols) {
    if (!PROVIDER_PROTOCOLS.includes(protocol) || seen.has(protocol)) continue;
    seen.add(protocol);
    unique.push(protocol);
  }
  return unique;
}

export function isSafeSourceUrl(value: string): boolean {
  const trimmed = value.trim();
  if (!trimmed) return false;
  try {
    const url = new URL(trimmed);
    if (url.protocol !== "https:" && url.protocol !== "http:") return false;
    if (url.username || url.password) return false;
    return Boolean(url.hostname);
  } catch {
    return false;
  }
}

function asNumber(value: unknown, fallback = 0): number {
  return typeof value === "number" && Number.isFinite(value) ? value : fallback;
}

function asString(value: unknown, fallback = ""): string {
  return typeof value === "string" ? value : fallback;
}

function asBoolean(value: unknown, fallback = false): boolean {
  return typeof value === "boolean" ? value : fallback;
}

function asStringArray(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === "string") : [];
}

function normalizeSwitches(value: ProtocolSwitches | null | undefined): ProtocolSwitches {
  return {
    chat_completions: asBoolean(value?.chat_completions, true),
    responses: asBoolean(value?.responses, true),
    messages: asBoolean(value?.messages, true),
  };
}

function normalizeCatalog(value: EffectiveCatalog | null | undefined): EffectiveCatalog {
  return {
    source: asString(value?.source),
    source_url: asString(value?.source_url),
    refreshed_at: typeof value?.refreshed_at === "string" ? value.refreshed_at : null,
    models: asStringArray(value?.models),
    refresh_supported: asBoolean(value?.refresh_supported, false),
  };
}

function normalizeEvidence(
  protocol: ProviderProtocol,
  value: EffectiveProtocolEvidence | undefined,
): EffectiveProtocolEvidence {
  return {
    protocol: value?.protocol ?? protocol,
    available: asBoolean(value?.available, false),
    enabled: asBoolean(value?.enabled, false),
    source: value?.source ?? "static",
    verified_at: typeof value?.verified_at === "string" ? value.verified_at : null,
    observed_at: typeof value?.observed_at === "string" ? value.observed_at : null,
    last_probe_result: value?.last_probe_result ?? null,
    last_probe_at: typeof value?.last_probe_at === "string" ? value.last_probe_at : null,
    last_probe_error: typeof value?.last_probe_error === "string" ? value.last_probe_error : null,
  };
}

function normalizeModel(model: EffectiveModelContract): EffectiveModelContract {
  const protocols: Record<string, EffectiveProtocolEvidence> = {};
  for (const protocol of PROVIDER_PROTOCOLS) {
    const evidence = model.protocols?.[protocol];
    if (evidence) protocols[protocol] = normalizeEvidence(protocol, evidence);
  }
  if (model.protocols) {
    for (const [key, evidence] of Object.entries(model.protocols)) {
      if (!protocols[key] && evidence) {
        protocols[key] = normalizeEvidence(evidence.protocol ?? (key as ProviderProtocol), evidence);
      }
    }
  }
  return {
    model_id: asString(model.model_id),
    preferred_protocol: model.preferred_protocol ?? "chat_completions",
    protocols,
    routable: asBoolean(model.routable, false),
    disabled_reasons: asStringArray(model.disabled_reasons),
  };
}

function normalizeCard(value: CardCapabilitySummary | null | undefined): CardCapabilitySummary {
  return {
    fetch_zen_models: asBoolean(value?.fetch_zen_models, false),
    discover_models: asBoolean(value?.discover_models, false),
    protocol_probe: asBoolean(value?.protocol_probe, false),
    catalog_refresh: asBoolean(value?.catalog_refresh, false),
  };
}

function normalizeOffering(offering: ProviderOfferingChoice): ProviderOfferingChoice {
  return {
    offering_id: asString(offering.offering_id),
    display_name: asString(offering.display_name),
    routable: asBoolean(offering.routable, false),
    accounts: Array.isArray(offering.accounts)
      ? offering.accounts.map((account) => ({
        id: asString(account.id),
        name: asString(account.name),
        enabled: asBoolean(account.enabled, false),
        verification_status: account.verification_status ?? "not_required",
      }))
      : [],
  };
}

function normalizeProviderGroup(group: ProviderContractGroup): ProviderContractGroup {
  return {
    ...group,
    scope_kind: group.scope_kind === "custom_endpoint" ? "custom_endpoint" : "provider",
    scope_id: asString(group.scope_id),
    provider_id: asString(group.provider_id),
    offerings: Array.isArray(group.offerings) ? group.offerings.map(normalizeOffering) : [],
    catalog: normalizeCatalog(group.catalog),
    models: Array.isArray(group.models) ? group.models.map(normalizeModel) : [],
    protocols: normalizeSwitches(group.protocols),
    pricing: { availability: asString(group.pricing?.availability) },
    usage: { availability: asString(group.usage?.availability) },
    card: normalizeCard(group.card),
    catalog_routable: asBoolean(group.catalog_routable, false),
    production_inference: asBoolean(group.production_inference, false),
    disabled_reasons: asStringArray(group.disabled_reasons),
    revision: asNumber(group.revision),
  };
}

function normalizeCustomEndpoint(endpoint: CustomEndpointContract): CustomEndpointContract {
  return {
    ...endpoint,
    scope_kind: "custom_endpoint",
    scope_id: asString(endpoint.scope_id),
    provider_id: asString(endpoint.provider_id, "custom"),
    account: {
      id: asString(endpoint.account?.id, asString(endpoint.scope_id)),
      name: asString(endpoint.account?.name, asString(endpoint.scope_id)),
      enabled: asBoolean(endpoint.account?.enabled, false),
      verification_status: endpoint.account?.verification_status ?? "pending",
    },
    catalog: normalizeCatalog(endpoint.catalog),
    models: Array.isArray(endpoint.models) ? endpoint.models.map(normalizeModel) : [],
    protocols: normalizeSwitches(endpoint.protocols),
    pricing: { availability: asString(endpoint.pricing?.availability, "unpriced") },
    usage: { availability: asString(endpoint.usage?.availability) },
    card: normalizeCard(endpoint.card),
    catalog_routable: asBoolean(endpoint.catalog_routable, false),
    production_inference: asBoolean(endpoint.production_inference, false),
    disabled_reasons: asStringArray(endpoint.disabled_reasons),
    revision: asNumber(endpoint.revision),
  };
}

export function normalizeProviderContractsResponse(
  raw: ProviderContractsResponse | null | undefined,
): ProviderContractsResponse {
  return {
    revision: asNumber(raw?.revision),
    providers: Array.isArray(raw?.providers) ? raw.providers.map(normalizeProviderGroup) : [],
    custom_endpoints: Array.isArray(raw?.custom_endpoints)
      ? raw.custom_endpoints.map(normalizeCustomEndpoint)
      : [],
  };
}

function providerLabel(
  providerId: string,
  catalog: readonly ProviderCatalogEntry[] | null | undefined,
): string {
  const plan = PLAN_DEFINITIONS.find((item) => item.provider_id === providerId);
  if (plan) return planFamilyLabel(plan, catalog);
  return providerId;
}

function customEndpointLabel(
  endpoint: CustomEndpointContract,
  catalog: readonly ProviderCatalogEntry[] | null | undefined,
): string {
  const name = endpoint.account.name.trim();
  if (name) return name;
  const plan = PLAN_DEFINITIONS.find((item) => item.kind === "custom");
  return plan ? planFamilyLabel(plan, catalog) : endpoint.scope_id;
}

function customOfferings(
  endpoint: CustomEndpointContract,
  catalog: readonly ProviderCatalogEntry[] | null | undefined,
): ProviderOfferingChoice[] {
  const plan = PLAN_DEFINITIONS.find((item) => item.kind === "custom");
  const offeringId = plan?.offering_ids[0] ?? "api";
  const entry = catalog?.find((item) => (
    item.provider_id === endpoint.provider_id && item.offering_id === offeringId
  ));
  return [{
    offering_id: offeringId,
    display_name: entry?.display_name || plan?.label || "Custom API",
    routable: endpoint.catalog_routable,
    accounts: [endpoint.account],
  }];
}

export function flattenProviderScopes(
  response: ProviderContractsResponse,
  catalog: readonly ProviderCatalogEntry[] | null | undefined = null,
): ProviderScopeView[] {
  const providers = response.providers.map((group) => ({
    key: providerScopeKey(group.scope_kind, group.scope_id),
    scope_kind: group.scope_kind,
    scope_id: group.scope_id,
    provider_id: group.provider_id,
    label: providerLabel(group.provider_id, catalog),
    offerings: group.offerings,
    catalog: group.catalog,
    models: group.models,
    protocols: group.protocols,
    pricing: group.pricing,
    usage: group.usage,
    card: group.card,
    catalog_routable: group.catalog_routable,
    production_inference: group.production_inference,
    disabled_reasons: group.disabled_reasons,
    revision: group.revision,
  }));
  const custom = response.custom_endpoints.map((endpoint) => ({
    key: providerScopeKey(endpoint.scope_kind, endpoint.scope_id),
    scope_kind: endpoint.scope_kind,
    scope_id: endpoint.scope_id,
    provider_id: endpoint.provider_id,
    label: customEndpointLabel(endpoint, catalog),
    offerings: customOfferings(endpoint, catalog),
    catalog: endpoint.catalog,
    models: endpoint.models,
    protocols: endpoint.protocols,
    pricing: endpoint.pricing,
    usage: endpoint.usage,
    card: endpoint.card,
    catalog_routable: endpoint.catalog_routable,
    production_inference: endpoint.production_inference,
    disabled_reasons: endpoint.disabled_reasons,
    revision: endpoint.revision,
  }));
  return [...providers, ...custom];
}

export function selectProviderScope(
  scopes: readonly ProviderScopeView[],
  scopeKind: string | null | undefined,
  scopeId: string | null | undefined,
): { scope: ProviderScopeView | null; fellBack: boolean } {
  if (scopes.length === 0) return { scope: null, fellBack: false };
  const match = scopes.find((scope) => (
    scope.scope_kind === scopeKind && scope.scope_id === scopeId
  ));
  if (match) return { scope: match, fellBack: false };
  return { scope: scopes[0] ?? null, fellBack: Boolean(scopeKind || scopeId) };
}

export function findScopeView(
  scopes: readonly ProviderScopeView[],
  ref: ProviderScopeRef,
): ProviderScopeView | undefined {
  return scopes.find((scope) => (
    scope.scope_kind === ref.scope_kind && scope.scope_id === ref.scope_id
  ));
}

export function scopeAccounts(scope: ProviderScopeView): ProviderAccountChoice[] {
  const seen = new Set<string>();
  const accounts: ProviderAccountChoice[] = [];
  for (const offering of scope.offerings) {
    for (const account of offering.accounts) {
      if (seen.has(account.id)) continue;
      seen.add(account.id);
      accounts.push(account);
    }
  }
  return accounts;
}

export function catalogRefreshSupported(scope: Pick<ProviderScopeView, "card" | "catalog">): boolean {
  return scope.card.catalog_refresh || scope.catalog.refresh_supported;
}

export function protocolProbeSupported(scope: Pick<ProviderScopeView, "card">): boolean {
  return scope.card.protocol_probe;
}

export function enabledProtocols(scope: Pick<ProviderScopeView, "protocols" | "models">): ProviderProtocol[] {
  return PROVIDER_PROTOCOLS.filter((protocol) => (
    scope.protocols[protocol]
    && scope.models.some((model) => model.protocols[protocol]?.enabled)
  ));
}

/**
 * Protocols that structurally belong to a scope: any protocol with evidence
 * on at least one model, plus any protocol whose switch is currently off so
 * it can always be switched back on. Switches render only this set.
 */
export function structuralProtocols(
  scope: Pick<ProviderScopeView, "protocols" | "models">,
): ProviderProtocol[] {
  return PROVIDER_PROTOCOLS.filter((protocol) => (
    !scope.protocols[protocol]
    || scope.models.some((model) => model.protocols[protocol] !== undefined)
  ));
}

/** Models effectively usable through a protocol right now (evidence enabled after switches). */
export function availableModelCount(
  scope: Pick<ProviderScopeView, "models">,
  protocol: ProviderProtocol,
): number {
  return scope.models.filter((model) => model.protocols[protocol]?.enabled).length;
}

export function allSupplierProtocolsDisabled(switches: ProtocolSwitches): boolean {
  return PROVIDER_PROTOCOLS.every((protocol) => !switches[protocol]);
}

export function protocolEvidenceStatus(
  protocol: ProviderProtocol,
  evidence: EffectiveProtocolEvidence | undefined,
  switches: ProtocolSwitches,
): ProtocolEvidenceUiStatus {
  if (!switches[protocol]) return "globally_closed";
  if (!evidence) return "unsupported";
  if (!evidence.available) return "unavailable";
  if (evidence.last_probe_result === "failure") return "probe_failure";
  if (evidence.source === "probe_confirmed" || evidence.last_probe_result === "success") {
    return "probe_confirmed";
  }
  if (evidence.source === "preset") return "preset";
  if (evidence.source === "static") return "static";
  return "unavailable";
}

export function mergeModelContract(
  models: readonly EffectiveModelContract[],
  next: EffectiveModelContract,
): EffectiveModelContract[] {
  const normalized = normalizeModel(next);
  const index = models.findIndex((model) => model.model_id === normalized.model_id);
  if (index < 0) return [...models, normalized];
  return models.map((model, itemIndex) => (itemIndex === index ? normalized : model));
}

export function replaceContractsResponse(
  next: ProviderContractsResponse,
): ProviderContractsResponse {
  return normalizeProviderContractsResponse(next);
}

export function applyModelContractToResponse(
  response: ProviderContractsResponse,
  scope: ProviderScopeRef,
  contract: EffectiveModelContract,
): ProviderContractsResponse {
  const normalized = normalizeProviderContractsResponse(response);
  if (scope.scope_kind === "custom_endpoint") {
    return {
      ...normalized,
      custom_endpoints: normalized.custom_endpoints.map((endpoint) => (
        endpoint.scope_id === scope.scope_id
          ? { ...endpoint, models: mergeModelContract(endpoint.models, contract) }
          : endpoint
      )),
    };
  }
  return {
    ...normalized,
    providers: normalized.providers.map((group) => (
      group.scope_id === scope.scope_id
        ? { ...group, models: mergeModelContract(group.models, contract) }
        : group
    )),
  };
}

export function accountContractSummary(
  account: Pick<Account, "id" | "name" | "provider_id" | "offering_id">,
  response: ProviderContractsResponse | null | undefined,
  catalog: readonly ProviderCatalogEntry[] | null | undefined = null,
): AccountContractSummary | null {
  if (response == null) return null;
  const ref = accountProviderScope(account);
  const scopes = flattenProviderScopes(normalizeProviderContractsResponse(response), catalog);
  const scope = findScopeView(scopes, ref);
  const plan = findPlanDefinition(account.provider_id, account.offering_id);
  const fallbackLabel = scope?.label
    ?? (plan ? planFamilyLabel(plan, catalog) : account.name);
  if (!scope) {
    return {
      ...ref,
      label: fallbackLabel,
      enabledProtocols: [],
      allProtocolsDisabled: false,
      unroutable: false,
      disabledReasons: [],
    };
  }
  const protocols = enabledProtocols(scope);
  const allDisabled = allSupplierProtocolsDisabled(scope.protocols);
  const unroutable = !scope.catalog_routable || !scope.production_inference;
  return {
    ...ref,
    label: scope.scope_kind === "custom_endpoint" ? scope.label : fallbackLabel,
    enabledProtocols: protocols,
    allProtocolsDisabled: allDisabled,
    unroutable: unroutable && !allDisabled,
    disabledReasons: scope.disabled_reasons,
  };
}

export function emptyProviderScopeView(
  ref: ProviderScopeRef,
  providerId = ref.scope_id,
): ProviderScopeView {
  return {
    key: providerScopeKey(ref.scope_kind, ref.scope_id),
    scope_kind: ref.scope_kind,
    scope_id: ref.scope_id,
    provider_id: providerId,
    label: providerId,
    offerings: [],
    catalog: EMPTY_CATALOG,
    models: [],
    protocols: EMPTY_SWITCHES,
    pricing: { availability: "" },
    usage: { availability: "" },
    card: EMPTY_CARD,
    catalog_routable: false,
    production_inference: false,
    disabled_reasons: [],
    revision: 0,
  };
}
