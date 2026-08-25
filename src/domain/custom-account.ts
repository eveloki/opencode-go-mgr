import type {
  Account,
  AccountCustomConfigInput,
  AccountModelCapability,
  AccountModelCapabilityInput,
  AccountUpdate,
} from "../api/dashboard.ts";
import type { MessageKey } from "../i18n/index.ts";

/**
 * Custom API accounts are administrator-trusted endpoints: the UI accepts any
 * backend-valid http:// or https:// base URL, including LAN, localhost, and
 * metadata addresses, and never re-imposes public-only/HTTPS-only/private-host
 * blockers. Client-side validation only rejects obviously malformed input,
 * non-http(s) schemes, and URL-embedded credentials — everything else is the
 * backend's call.
 */

export const CUSTOM_PROVIDER_ID = "custom";
export const CUSTOM_OFFERING_ID = "api";

export function isCustomApiAccount(
  account: Pick<Account, "provider_id" | "offering_id">,
): boolean {
  return account.provider_id === CUSTOM_PROVIDER_ID
    && account.offering_id === CUSTOM_OFFERING_ID;
}

export type CustomBaseUrlIssue = "empty" | "malformed" | "not_http" | "with_credentials";

export const CUSTOM_BASE_URL_ISSUE_KEYS = {
  empty: "请填写 Base URL",
  malformed: "Base URL 格式无效",
  not_http: "Base URL 必须是 http:// 或 https:// URL",
  with_credentials: "Base URL 不能包含用户名或密码",
} as const satisfies Record<CustomBaseUrlIssue, MessageKey>;

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
  protocol_mismatch: "模型能力必须与上游协议一致",
} as const satisfies Record<CustomCapabilityIssue, MessageKey>;

export class CustomCapabilityError extends Error {
  readonly issue: CustomCapabilityIssue;

  constructor(issue: CustomCapabilityIssue) {
    super(issue);
    this.issue = issue;
  }
}

/**
 * Validate — never normalize — a Custom base URL. Trusted destinations such as
 * `http://192.168.1.10:8080/v1`, `http://localhost:3000`, and metadata IPs are
 * valid; only shape, scheme, and embedded credentials are rejected.
 */
export function customBaseUrlIssue(value: string): CustomBaseUrlIssue | null {
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

/**
 * Stable comparison identity for a backend-valid Custom base URL. This is
 * deliberately not the persisted payload: callers keep the administrator's
 * trimmed input while avoiding a config write for URL spellings that the URL
 * parser treats as the same endpoint.
 */
export function canonicalCustomBaseUrl(value: string): string {
  const parsed = new URL(value.trim());
  if (parsed.username || parsed.password) {
    throw new Error("Custom base URL must not contain credentials");
  }
  const pathname = parsed.pathname.replace(/\/+$/u, "");
  return `${parsed.protocol}//${parsed.host}${pathname}${parsed.search}${parsed.hash}`;
}

/**
 * Mirror the Custom capability constraints enforced by the backend before any
 * account mutation is sent. The normalized model ID is the backend's trimmed
 * value, so duplicates are caught even when users only differ by whitespace.
 */
export function normalizeCustomCapabilities(
  capabilities: readonly Pick<AccountModelCapabilityInput, "model_id" | "protocol">[],
  upstreamProtocol: AccountCustomConfigInput["upstream_protocol"],
): AccountModelCapabilityInput[] {
  if (capabilities.length === 0) throw new CustomCapabilityError("missing");

  const seenModelIds = new Set<string>();
  return capabilities.map((capability) => {
    const model_id = capability.model_id.trim();
    if (Array.from(model_id).length > MAX_CUSTOM_MODEL_ID_CHARS) {
      throw new CustomCapabilityError("model_id_too_long");
    }
    if (/[\u0000-\u001F\u007F-\u009F]/u.test(model_id)) {
      throw new CustomCapabilityError("model_id_has_control_character");
    }
    if (!model_id || seenModelIds.has(model_id)) {
      throw new CustomCapabilityError(!model_id ? "missing" : "duplicate_model_id");
    }
    seenModelIds.add(model_id);
    if (capability.protocol !== upstreamProtocol) {
      throw new CustomCapabilityError("protocol_mismatch");
    }
    return { model_id, protocol: upstreamProtocol, source: "manual" };
  });
}

export type CustomAccountEditInput = {
  name: string;
  notes?: string;
  key?: string;
  base_url?: string;
  model_capabilities?: readonly Pick<AccountModelCapabilityInput, "model_id" | "protocol">[];
};

export type CustomAccountEditPlan = {
  account?: AccountUpdate;
  customConfig?: AccountCustomConfigInput;
  capabilities?: AccountModelCapabilityInput[];
};

export type CustomAccountEditWriters = {
  account: (update: AccountUpdate) => Promise<void>;
  customConfig: (config: AccountCustomConfigInput) => Promise<void>;
  capabilities: (capabilities: AccountModelCapabilityInput[]) => Promise<void>;
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

/**
 * Compute only the Custom account sections that actually changed. Validation
 * intentionally runs first so invalid capabilities cannot leave a metadata
 * PATCH behind before the dedicated route rejects the remaining edit.
 */
export function planCustomAccountEdit(
  account: Account,
  input: CustomAccountEditInput,
): CustomAccountEditPlan {
  const config = account.custom_config;
  if (!config) throw new Error("Custom account configuration is missing");

  const base_url = (input.base_url ?? config.base_url).trim();
  const baseUrlIssue = customBaseUrlIssue(base_url);
  if (baseUrlIssue) throw new Error(CUSTOM_BASE_URL_ISSUE_KEYS[baseUrlIssue]);
  const canonicalBaseUrl = canonicalCustomBaseUrl(base_url);
  const canonicalSavedBaseUrl = canonicalCustomBaseUrl(config.base_url);

  const capabilities = normalizeCustomCapabilities(
    input.model_capabilities ?? account.model_capabilities,
    config.upstream_protocol,
  );
  const name = input.name.trim();
  const notes = input.notes ?? "";
  const keyReplacement = input.key !== undefined;
  const metadataChanged = name !== account.name || notes !== account.notes || keyReplacement;

  return {
    ...(metadataChanged
      ? { account: { name, notes, ...(input.key === undefined ? {} : { key: input.key }) } }
      : {}),
    ...(canonicalBaseUrl !== canonicalSavedBaseUrl
      ? {
        customConfig: {
          base_url,
          upstream_protocol: config.upstream_protocol,
          auth_scheme: config.auth_scheme,
        },
      }
      : {}),
    ...(keyReplacement || !sameCapabilities(account.model_capabilities, capabilities)
      ? { capabilities }
      : {}),
  };
}

/** Apply an already-validated edit plan in the only safe order. */
export async function applyCustomAccountEditPlan(
  plan: CustomAccountEditPlan,
  writers: CustomAccountEditWriters,
): Promise<void> {
  if (plan.account) await writers.account(plan.account);
  if (plan.customConfig) await writers.customConfig(plan.customConfig);
  if (plan.capabilities) await writers.capabilities(plan.capabilities);
}

/** Validate before handing any write to the dashboard transport. */
export async function executeCustomAccountEdit(
  account: Account,
  input: CustomAccountEditInput,
  writers: CustomAccountEditWriters,
): Promise<void> {
  await applyCustomAccountEditPlan(planCustomAccountEdit(account, input), writers);
}

/** Pending or failed Custom accounts expose the verify-connection action. */
export function customAccountNeedsVerification(
  account: Pick<Account, "provider_id" | "offering_id" | "verification_status">,
): boolean {
  return isCustomApiAccount(account)
    && (account.verification_status === "pending" || account.verification_status === "failed");
}

/**
 * Verification is never enablement: the normal enable switch only becomes
 * interactive once the backend reports the connection as verified.
 */
export function customAccountToggleBlocked(
  account: Pick<Account, "provider_id" | "offering_id" | "verification_status">,
): boolean {
  return isCustomApiAccount(account) && account.verification_status !== "verified";
}
