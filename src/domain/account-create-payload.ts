import type { AccountInput } from "../api/dashboard.ts";
import type { PlanDefinition } from "./plans.ts";
import {
  CustomCapabilityError,
  customEndpointUrlIssue,
  expandCustomModelCapabilities,
  isCustomProtocol,
  normalizeCustomCapabilities,
} from "./custom-account.ts";

export type UpstreamProtocol = "chat_completions" | "responses" | "messages";

/** The form declares plain model IDs; one account-level protocol applies to all. */
export interface AccountCreateCapability {
  model_id: string;
}

export interface AccountCreateFormValues {
  name: string;
  username?: string;
  key: string;
  purchase_date?: string;
  notes?: string;
  endpoint_url?: string;
  upstream_protocol?: UpstreamProtocol;
  model_capabilities?: AccountCreateCapability[];
}

export type AccountCreatePayloadErrorCode =
  | "missing_offering"
  | "missing_name"
  | "missing_key"
  | "missing_endpoint_url"
  | "invalid_endpoint_url"
  | "endpoint_url_not_http"
  | "endpoint_url_with_credentials"
  | "missing_upstream_protocol"
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
  missing_endpoint_url: "请填写 API 地址",
  invalid_endpoint_url: "Endpoint 格式无效",
  endpoint_url_not_http: "Endpoint 必须是 http:// 或 https:// URL",
  endpoint_url_with_credentials: "Endpoint 不能包含用户名或密码",
  missing_upstream_protocol: "请选择上游协议",
  missing_model_capabilities: "请至少添加一个模型能力",
  duplicate_model_id: "模型 ID 不能重复",
  model_id_too_long: "模型 ID 最多 200 个字符",
  model_id_has_control_character: "模型 ID 不能包含控制字符",
  capability_protocol_mismatch: "模型能力必须使用所选上游协议",
  custom_fields_not_allowed: "账号创建失败，请重试",
} as const satisfies Record<AccountCreatePayloadErrorCode, string>;

export class AccountCreatePayloadError extends Error {
  readonly code: AccountCreatePayloadErrorCode;

  constructor(code: AccountCreatePayloadErrorCode) {
    super(code);
    this.name = "AccountCreatePayloadError";
    this.code = code;
  }
}

export function accountCreatePayloadErrorKey(error: unknown) {
  return error instanceof AccountCreatePayloadError
    ? ACCOUNT_CREATE_PAYLOAD_ERROR_KEYS[error.code]
    : "账号创建失败，请重试";
}

function trimOptional(value: string | undefined): string | undefined {
  const trimmed = value?.trim();
  return trimmed && trimmed.length > 0 ? trimmed : undefined;
}

/** Build the exact V3 create payload from the chosen plan and form values. */
export function buildCreateAccountPayload(
  plan: PlanDefinition,
  offeringId: string | undefined,
  values: AccountCreateFormValues,
): AccountInput {
  const selectedOfferingId = offeringId && plan.offering_ids.includes(offeringId)
    ? offeringId
    : plan.offering_ids[0];
  if (!selectedOfferingId) throw new AccountCreatePayloadError("missing_offering");
  if (!values.name.trim()) throw new AccountCreatePayloadError("missing_name");
  if (!values.key.trim()) throw new AccountCreatePayloadError("missing_key");

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
    if (!values.endpoint_url?.trim()) throw new AccountCreatePayloadError("missing_endpoint_url");
    const endpointIssue = customEndpointUrlIssue(values.endpoint_url);
    if (endpointIssue === "malformed") throw new AccountCreatePayloadError("invalid_endpoint_url");
    if (endpointIssue === "not_http") throw new AccountCreatePayloadError("endpoint_url_not_http");
    if (endpointIssue === "with_credentials") throw new AccountCreatePayloadError("endpoint_url_with_credentials");
    if (!isCustomProtocol(values.upstream_protocol)) {
      throw new AccountCreatePayloadError("missing_upstream_protocol");
    }
    if (!values.model_capabilities || values.model_capabilities.length === 0) {
      throw new AccountCreatePayloadError("missing_model_capabilities");
    }
    payload.custom_config = {
      endpoint_url: values.endpoint_url.trim(),
      upstream_protocol: values.upstream_protocol,
    };
    try {
      payload.model_capabilities = normalizeCustomCapabilities(
        expandCustomModelCapabilities(
          values.model_capabilities.map((capability) => capability.model_id),
          values.upstream_protocol,
        ),
        values.upstream_protocol,
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
  } else if (
    values.endpoint_url?.trim()
    || values.upstream_protocol
    || values.model_capabilities?.length
  ) {
    throw new AccountCreatePayloadError("custom_fields_not_allowed");
  }

  return payload;
}
