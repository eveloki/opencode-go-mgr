import type { AccountInput } from "../api/tauri.ts";
import type { ProviderCatalogEntry } from "../api/providers.ts";
import type { MessageKey } from "../i18n/index.ts";
import type { PlanDefinition } from "./plans.ts";

export type UpstreamProtocol = "chat_completions" | "responses" | "messages";
export type AuthScheme = "bearer" | "x-api-key";

export interface AccountCreateCapability {
  model_id: string;
  protocol: UpstreamProtocol;
}

export interface AccountCreateFormValues {
  name: string;
  username?: string;
  key: string;
  purchase_date?: string;
  notes?: string;
  base_url?: string;
  upstream_protocol?: UpstreamProtocol;
  auth_scheme?: AuthScheme;
  acknowledgement_accepted?: boolean;
  model_capabilities?: AccountCreateCapability[];
}

export type AccountCreatePayloadErrorCode =
  | "missing_offering"
  | "missing_name"
  | "missing_key"
  | "missing_base_url"
  | "missing_upstream_protocol"
  | "missing_auth_scheme"
  | "missing_model_capabilities"
  | "custom_fields_not_allowed"
  | "risk_acknowledgement_required";

const ACCOUNT_CREATE_PAYLOAD_ERROR_KEYS = {
  missing_offering: "无法确定账号方案，请关闭后重试",
  missing_name: "名称不能为空",
  missing_key: "请填写 API Key",
  missing_base_url: "请填写 Base URL",
  missing_upstream_protocol: "选择上游协议",
  missing_auth_scheme: "选择鉴权方式",
  missing_model_capabilities: "请至少添加一个模型能力",
  custom_fields_not_allowed: "账号创建失败，请重试",
  risk_acknowledgement_required: "请阅读并同意条款",
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
 * family, offering, form values, and the matching catalog entry.
 *
 * This function is pure and testable in Node; it does not depend on Vue or the
 * i18n runtime.
 */
export function buildCreateAccountPayload(
  plan: PlanDefinition,
  offeringId: string | undefined,
  values: AccountCreateFormValues,
  catalogEntry: ProviderCatalogEntry | undefined,
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
    if (!values.upstream_protocol) {
      throw new AccountCreatePayloadError("missing_upstream_protocol");
    }
    if (!values.auth_scheme) {
      throw new AccountCreatePayloadError("missing_auth_scheme");
    }
    if (!values.model_capabilities || values.model_capabilities.length === 0) {
      throw new AccountCreatePayloadError("missing_model_capabilities");
    }
    payload.custom_config = {
      base_url: values.base_url.trim(),
      upstream_protocol: values.upstream_protocol,
      auth_scheme: values.auth_scheme,
    };
    payload.model_capabilities = values.model_capabilities.map((cap) => ({
      model_id: cap.model_id.trim(),
      protocol: cap.protocol,
      source: "manual",
    }));
  } else {
    if (
      values.base_url?.trim()
      || values.upstream_protocol
      || values.auth_scheme
      || values.model_capabilities?.length
    ) {
      throw new AccountCreatePayloadError("custom_fields_not_allowed");
    }
  }

  if (catalogEntry?.risk_notice) {
    if (!values.acknowledgement_accepted) {
      throw new AccountCreatePayloadError("risk_acknowledgement_required");
    }
    payload.acknowledgements = [{
      acknowledgement_id: catalogEntry.risk_notice.acknowledgement_id,
      version: catalogEntry.risk_notice.version,
    }];
  }

  return payload;
}
