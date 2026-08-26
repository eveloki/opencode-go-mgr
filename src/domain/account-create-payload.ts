import type { AccountInput } from "../api/dashboard.ts";
import type { MessageKey } from "../i18n/index.ts";
import type { PlanDefinition } from "./plans.ts";
import {
  canonicalCustomProtocols,
  CustomCapabilityError,
  customBaseUrlIssue,
  expandCustomModelCapabilities,
  normalizeCustomCapabilities,
} from "./custom-account.ts";

export type UpstreamProtocol = "chat_completions" | "responses" | "messages";
export type AuthScheme = "bearer" | "x-api-key";

/** The form declares plain model IDs; protocols come from the account-level set. */
export interface AccountCreateCapability {
  model_id: string;
}

export interface AccountCreateFormValues {
  name: string;
  username?: string;
  key: string;
  purchase_date?: string;
  notes?: string;
  base_url?: string;
  upstream_protocols?: UpstreamProtocol[];
  auth_scheme?: AuthScheme;
  model_capabilities?: AccountCreateCapability[];
}

export type AccountCreatePayloadErrorCode =
  | "missing_offering"
  | "missing_name"
  | "missing_key"
  | "missing_base_url"
  | "invalid_base_url"
  | "base_url_not_http"
  | "base_url_with_credentials"
  | "missing_upstream_protocol"
  | "missing_auth_scheme"
  | "missing_model_capabilities"
  | "duplicate_model_id"
  | "model_id_too_long"
  | "model_id_has_control_character"
  | "capability_protocol_mismatch"
  | "custom_fields_not_allowed";

const ACCOUNT_CREATE_PAYLOAD_ERROR_KEYS = {
  missing_offering: "无法确定账号方案，请关闭后重试",
  missing_name: "名称不能为空",
  missing_key: "请填写 API Key",
  missing_base_url: "请填写 Base URL",
  invalid_base_url: "Base URL 格式无效",
  base_url_not_http: "Base URL 必须是 http:// 或 https:// URL",
  base_url_with_credentials: "Base URL 不能包含用户名或密码",
  missing_upstream_protocol: "请至少选择一个上游协议",
  missing_auth_scheme: "选择鉴权方式",
  missing_model_capabilities: "请至少添加一个模型能力",
  duplicate_model_id: "模型 ID 不能重复",
  model_id_too_long: "模型 ID 最多 200 个字符",
  model_id_has_control_character: "模型 ID 不能包含控制字符",
  capability_protocol_mismatch: "模型能力协议必须属于所选上游协议",
  custom_fields_not_allowed: "账号创建失败，请重试",
} as const satisfies Record<AccountCreatePayloadErrorCode, MessageKey>;

export class AccountCreatePayloadError extends Error {
  readonly code: AccountCreatePayloadErrorCode;

  constructor(code: AccountCreatePayloadErrorCode) {
    super(code);
    this.name = "AccountCreatePayloadError";
    this.code = code;
  }
}

export function accountCreatePayloadErrorKey(error: unknown): MessageKey {
  if (error instanceof AccountCreatePayloadError) {
    return ACCOUNT_CREATE_PAYLOAD_ERROR_KEYS[error.code];
  }
  return "账号创建失败，请重试";
}

function trimOptional(value: string | undefined): string | undefined {
  const trimmed = value?.trim();
  return trimmed && trimmed.length > 0 ? trimmed : undefined;
}

/**
 * Build the exact `POST /dashboard/api/accounts` payload from the chosen plan
 * family, offering, and form values.
 *
 * This function is pure and testable in Node; it does not depend on Vue or the
 * i18n runtime.
 */
export function buildCreateAccountPayload(
  plan: PlanDefinition,
  offeringId: string | undefined,
  values: AccountCreateFormValues,
): AccountInput {
  const selectedOfferingId = offeringId && plan.offering_ids.includes(offeringId)
    ? offeringId
    : plan.offering_ids[0];
  if (!selectedOfferingId) {
    throw new AccountCreatePayloadError("missing_offering");
  }

  if (!values.name.trim()) {
    throw new AccountCreatePayloadError("missing_name");
  }
  if (!values.key.trim()) {
    throw new AccountCreatePayloadError("missing_key");
  }

  const isCustom = plan.id === "custom-endpoint";
  const payload: AccountInput = {
    name: values.name.trim(),
    provider_id: plan.provider_id,
    offering_id: selectedOfferingId,
    key: values.key.trim(),
  };

  const username = trimOptional(values.username);
  if (username) payload.username = username;

  const purchaseDate = trimOptional(values.purchase_date);
  if (purchaseDate) payload.purchase_date = purchaseDate;

  const notes = trimOptional(values.notes);
  if (notes) payload.notes = notes;

  if (isCustom) {
    if (!values.base_url?.trim()) {
      throw new AccountCreatePayloadError("missing_base_url");
    }
    // Trusted destinations (LAN, localhost, metadata IPs, plain HTTP) are
    // allowed; only malformed input, non-http(s) schemes, and URL-embedded
    // credentials are rejected before the backend sees the payload.
    const baseUrlIssue = customBaseUrlIssue(values.base_url);
    if (baseUrlIssue === "malformed") {
      throw new AccountCreatePayloadError("invalid_base_url");
    }
    if (baseUrlIssue === "not_http") {
      throw new AccountCreatePayloadError("base_url_not_http");
    }
    if (baseUrlIssue === "with_credentials") {
      throw new AccountCreatePayloadError("base_url_with_credentials");
    }
    if (!values.upstream_protocols || values.upstream_protocols.length === 0) {
      throw new AccountCreatePayloadError("missing_upstream_protocol");
    }
    const upstream_protocols = canonicalCustomProtocols(values.upstream_protocols);
    if (!values.auth_scheme) {
      throw new AccountCreatePayloadError("missing_auth_scheme");
    }
    if (!values.model_capabilities || values.model_capabilities.length === 0) {
      throw new AccountCreatePayloadError("missing_model_capabilities");
    }
    payload.custom_config = {
      base_url: values.base_url.trim(),
      upstream_protocols,
      auth_scheme: values.auth_scheme,
    };
    try {
      // The backend only accepts the exact model × protocol-set expansion.
      payload.model_capabilities = normalizeCustomCapabilities(
        expandCustomModelCapabilities(
          values.model_capabilities.map((capability) => capability.model_id),
          upstream_protocols,
        ),
        upstream_protocols,
      );
    } catch (error) {
      if (error instanceof CustomCapabilityError) {
        const code = ({
          missing: "missing_model_capabilities",
          duplicate_model_id: "duplicate_model_id",
          model_id_too_long: "model_id_too_long",
          model_id_has_control_character: "model_id_has_control_character",
          protocol_mismatch: "capability_protocol_mismatch",
        } as const)[error.issue];
        throw new AccountCreatePayloadError(code);
      }
      throw error;
    }
  } else {
    if (
      values.base_url?.trim()
      || values.upstream_protocols?.length
      || values.auth_scheme
      || values.model_capabilities?.length
    ) {
      throw new AccountCreatePayloadError("custom_fields_not_allowed");
    }
  }

  return payload;
}
