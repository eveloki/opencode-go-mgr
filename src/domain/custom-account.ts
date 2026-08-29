import type {
  Account,
  AccountCustomConfigUpdateInput,
  AccountModelCapability,
  AccountModelCapabilityInput,
  AccountProtocol,
  AccountUpdate,
} from "../api/dashboard.ts";

/**
 * Custom API accounts are administrator-trusted endpoints: the UI accepts any
 * backend-valid http:// or https:// API URL, including LAN,
 * localhost, and metadata addresses. Client-side validation only rejects
 * malformed input, non-http(s) schemes, and URL-embedded credentials.
 */
export const CUSTOM_PROVIDER_ID = "custom";
export const CUSTOM_OFFERING_ID = "api";

export function isCustomApiAccount(
  account: Pick<Account, "provider_id" | "offering_id">,
): boolean {
  return account.provider_id === CUSTOM_PROVIDER_ID
    && account.offering_id === CUSTOM_OFFERING_ID;
}

export type CustomEndpointUrlIssue = "empty" | "malformed" | "not_http" | "with_credentials";

export const CUSTOM_ENDPOINT_URL_ISSUE_KEYS = {
  empty: "请填写 API 地址",
  malformed: "Endpoint 格式无效",
  not_http: "Endpoint 必须是 http:// 或 https:// URL",
  with_credentials: "Endpoint 不能包含用户名或密码",
} as const satisfies Record<CustomEndpointUrlIssue, string>;

export const MAX_CUSTOM_MODEL_ID_CHARS = 200;

export type CustomCapabilityIssue =
  | "missing"
  | "duplicate_model_id"
  | "model_id_too_long"
  | "model_id_has_control_character"
  | "protocol_mismatch";

export const CUSTOM_CAPABILITY_ISSUE_KEYS = {
  missing: "请至少添加一个模型能力",
  duplicate_model_id: "模型 ID 不能重复",
  model_id_too_long: "模型 ID 最多 200 个字符",
  model_id_has_control_character: "模型 ID 不能包含控制字符",
  protocol_mismatch: "模型能力必须使用所选上游协议",
} as const satisfies Record<CustomCapabilityIssue, string>;

export class CustomCapabilityError extends Error {
  readonly issue: CustomCapabilityIssue;

  constructor(issue: CustomCapabilityIssue) {
    super(issue);
    this.issue = issue;
  }
}

export function customEndpointUrlIssue(value: string): CustomEndpointUrlIssue | null {
  const trimmed = value.trim();
  if (!trimmed) return "empty";
  let parsed: URL;
  try {
    parsed = new URL(trimmed);
  } catch {
    return "malformed";
  }
  if (parsed.protocol !== "http:" && parsed.protocol !== "https:") return "not_http";
  if (!parsed.hostname) return "malformed";
  if (parsed.username || parsed.password) return "with_credentials";
  return null;
}

/** Comparison identity only; the submitted API URL preserves administrator input. */
export function canonicalCustomEndpointUrl(value: string): string {
  const parsed = new URL(value.trim());
  if (parsed.username || parsed.password) {
    throw new Error("Custom Endpoint must not contain credentials");
  }
  const pathname = parsed.pathname.replace(/\/+$/u, "");
  return `${parsed.protocol}//${parsed.host}${pathname}${parsed.search}${parsed.hash}`;
}

export const CUSTOM_PROTOCOLS: readonly AccountProtocol[] = [
  "chat_completions",
  "responses",
  "messages",
];

export function isCustomProtocol(value: unknown): value is AccountProtocol {
  return typeof value === "string" && CUSTOM_PROTOCOLS.includes(value as AccountProtocol);
}

export function customApiUrlPlaceholder(): string {
  return "https://api.example.com";
}

/** Root, `/v1`, and legacy standard endpoints have an unambiguous models URL. */
export function customApiUrlSupportsModelDiscovery(
  endpointUrl: string,
  protocol: AccountProtocol | null,
): boolean {
  if (!protocol || customEndpointUrlIssue(endpointUrl)) return false;
  try {
    const pathname = new URL(endpointUrl.trim()).pathname.replace(/\/+$/u, "");
    if (!pathname || pathname.endsWith("/v1")) return true;
    const standardPath = {
      chat_completions: "/chat/completions",
      responses: "/responses",
      messages: "/messages",
    } satisfies Record<AccountProtocol, string>;
    return pathname.endsWith(standardPath[protocol]);
  } catch {
    return false;
  }
}

/** Show the manual-model hint only for a valid API URL with no derivable models URL. */
export function customApiUrlNeedsManualModels(
  endpointUrl: string,
  protocol: AccountProtocol | null,
): boolean {
  return customEndpointUrlIssue(endpointUrl) === null
    && !customApiUrlSupportsModelDiscovery(endpointUrl, protocol);
}

/** Expand each declared model into exactly the selected upstream protocol. */
export function expandCustomModelCapabilities(
  modelIds: readonly string[],
  upstreamProtocol: AccountProtocol,
): Pick<AccountModelCapabilityInput, "model_id" | "protocol">[] {
  return modelIds.map((model_id) => ({ model_id, protocol: upstreamProtocol }));
}

export function normalizeCustomCapabilities(
  capabilities: readonly Pick<AccountModelCapabilityInput, "model_id" | "protocol">[],
  upstreamProtocol: AccountProtocol,
): AccountModelCapabilityInput[] {
  if (capabilities.length === 0) throw new CustomCapabilityError("missing");

  const seenRows = new Set<string>();
  return capabilities.map((capability) => {
    const model_id = capability.model_id.trim();
    if (Array.from(model_id).length > MAX_CUSTOM_MODEL_ID_CHARS) {
      throw new CustomCapabilityError("model_id_too_long");
    }
    if (/[\u0000-\u001F\u007F-\u009F]/u.test(model_id)) {
      throw new CustomCapabilityError("model_id_has_control_character");
    }
    if (capability.protocol !== upstreamProtocol) {
      throw new CustomCapabilityError("protocol_mismatch");
    }
    if (!model_id || seenRows.has(model_id)) {
      throw new CustomCapabilityError(!model_id ? "missing" : "duplicate_model_id");
    }
    seenRows.add(model_id);
    return { model_id, protocol: capability.protocol, source: "manual" };
  });
}

export type CustomAccountEditInput = {
  name: string;
  notes?: string;
  key?: string;
  endpoint_url?: string;
  upstream_protocol?: AccountProtocol;
  model_capabilities?: readonly Pick<AccountModelCapabilityInput, "model_id" | "protocol">[];
};

export type CustomAccountEditPlan = {
  account?: AccountUpdate;
  customConfig?: AccountCustomConfigUpdateInput;
};

export type CustomAccountEditWriters = {
  account: (update: AccountUpdate) => Promise<void>;
  customConfig: (config: AccountCustomConfigUpdateInput) => Promise<void>;
  /** Accepted for source compatibility; edits now atomically use customConfig. */
  capabilities?: (capabilities: AccountModelCapabilityInput[]) => Promise<void>;
};

function sameCapabilities(
  saved: readonly AccountModelCapability[],
  next: readonly AccountModelCapabilityInput[],
): boolean {
  return saved.length === next.length && saved.every((capability, index) => (
    capability.model_id.trim() === next[index]?.model_id
      && capability.protocol === next[index]?.protocol
  ));
}

/** Validate all Custom sections before any write, then combine config and models. */
export function planCustomAccountEdit(
  account: Account,
  input: CustomAccountEditInput,
): CustomAccountEditPlan {
  const config = account.custom_config;
  if (!config) throw new Error("Custom account configuration is missing");

  const endpoint_url = (input.endpoint_url ?? config.endpoint_url).trim();
  const endpointUrlIssue = customEndpointUrlIssue(endpoint_url);
  if (endpointUrlIssue) throw new Error(CUSTOM_ENDPOINT_URL_ISSUE_KEYS[endpointUrlIssue]);
  const canonicalEndpointUrl = canonicalCustomEndpointUrl(endpoint_url);
  const canonicalSavedEndpointUrl = canonicalCustomEndpointUrl(config.endpoint_url);
  const upstream_protocol = input.upstream_protocol ?? config.upstream_protocol;
  if (!isCustomProtocol(upstream_protocol)) throw new CustomCapabilityError("protocol_mismatch");

  const capabilities = normalizeCustomCapabilities(
    input.model_capabilities ?? account.model_capabilities,
    upstream_protocol,
  );
  const name = input.name.trim();
  const notes = input.notes ?? "";
  const keyReplacement = input.key !== undefined;
  const metadataChanged = name !== account.name || notes !== account.notes || keyReplacement;
  const capabilitiesChanged = !sameCapabilities(account.model_capabilities, capabilities);
  const configChanged = canonicalEndpointUrl !== canonicalSavedEndpointUrl
    || upstream_protocol !== config.upstream_protocol;

  return {
    ...(metadataChanged
      ? { account: { name, notes, ...(input.key === undefined ? {} : { key: input.key }) } }
      : {}),
    ...(configChanged || capabilitiesChanged || keyReplacement
      ? {
        customConfig: {
          endpoint_url,
          upstream_protocol,
          model_capabilities: capabilities,
        },
      }
      : {}),
  };
}

export async function applyCustomAccountEditPlan(
  plan: CustomAccountEditPlan,
  writers: CustomAccountEditWriters,
): Promise<void> {
  if (plan.account) await writers.account(plan.account);
  if (plan.customConfig) await writers.customConfig(plan.customConfig);
}

export async function executeCustomAccountEdit(
  account: Account,
  input: CustomAccountEditInput,
  writers: CustomAccountEditWriters,
): Promise<void> {
  await applyCustomAccountEditPlan(planCustomAccountEdit(account, input), writers);
}

export function customAccountNeedsVerification(
  account: Pick<Account, "provider_id" | "offering_id" | "verification_status">,
): boolean {
  return isCustomApiAccount(account)
    && (account.verification_status === "pending" || account.verification_status === "failed");
}
