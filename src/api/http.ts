import { t } from "../i18n/index.ts";

/**
 * Domain-neutral HTTP transport for the dashboard API: base-URL resolution,
 * JSON body encoding, and the shared error types every endpoint wrapper in
 * the Dashboard clients build on. Keeping this layer free of endpoint DTOs
 * lets provider-specific API modules reuse it without coupling their contracts.
 */

export const DASHBOARD_AUTH_REQUIRED_EVENT = "ocg-dashboard-auth-required";
/**
 * Kept in sync with the V3 transport event so an already-loaded legacy SPA
 * can hand a retired V2 endpoint to the shell's existing refresh banner.
 */
export const DASHBOARD_GONE_EVENT = "ocg-dashboard-gone";

const DASHBOARD_V2_REMOVED_CODE = "dashboardV2Removed";

function dashboardV2RemovedGuidance(): string {
  return "页面版本与服务不匹配，请刷新页面后重试；若仍失败请升级到最新版本";
}

export class DashboardAuthError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "DashboardAuthError";
  }
}

export class DashboardRequestError extends Error {
  readonly status: number;
  readonly retryAfterSeconds: number | null;
  readonly nextAllowedAt: string | null;

  constructor(
    message: string,
    status: number,
    retryAfterSeconds: number | null = null,
    nextAllowedAt: string | null = null,
  ) {
    super(message);
    this.name = "DashboardRequestError";
    this.status = status;
    this.retryAfterSeconds = retryAfterSeconds;
    this.nextAllowedAt = nextAllowedAt;
  }
}

function dashboardAuthError(message: string): DashboardAuthError {
  const error = new DashboardAuthError(message);
  window.dispatchEvent(new CustomEvent(DASHBOARD_AUTH_REQUIRED_EVENT, { detail: message }));
  return error;
}

export function apiBase(): string {
  if (window.location.pathname.startsWith("/dashboard")) {
    return "/dashboard/api";
  }
  // 回退仅覆盖 Gateway 监听默认端口 9042 的纯静态托管场景（如直接打开构建产物）
  return "http://127.0.0.1:9042/dashboard/api";
}

export async function request<T>(
  path: string,
  init: RequestInit = {},
  notifyAuthRequired = true,
): Promise<T> {
  const headers = new Headers(init.headers);
  if (init.body && !headers.has("Content-Type")) {
    headers.set("Content-Type", "application/json");
  }
  const response = await fetch(`${apiBase()}${path}`, {
    ...init,
    headers,
    credentials: "same-origin",
  });
  if (!response.ok) {
    if (response.status === 401 && notifyAuthRequired) {
      throw dashboardAuthError(t("登录已失效，请重新登录"));
    }
    let message = `${response.status} ${response.statusText}`;
    let nextAllowedAt: string | null = null;
    const responseText = await response.text().catch(() => "");
    if (responseText) {
      try {
        const body = JSON.parse(responseText) as {
          error?: unknown;
          code?: unknown;
          message?: unknown;
          next_allowed_at?: unknown;
        };
        if (typeof body.error === "string") message = body.error;
        // V2 removal is deliberately a standalone migration envelope rather
        // than the V3 `V3Error` shape. Preserve the legacy error type for
        // callers, but notify the shell so it presents the refresh/upgrade
        // prompt instead of a generic 410.
        if (response.status === 410 && body.code === DASHBOARD_V2_REMOVED_CODE) {
          if (typeof body.message === "string" && body.message) message = body.message;
          window.dispatchEvent(new CustomEvent(DASHBOARD_GONE_EVENT, {
            detail: { message, guidance: dashboardV2RemovedGuidance(), path },
          }));
        }
        if (typeof body.next_allowed_at === "string") nextAllowedAt = body.next_allowed_at;
      } catch {
        message = responseText;
      }
    }
    const retryAfterHeader = response.headers.get("Retry-After");
    const retryAfterSeconds = retryAfterHeader && /^\d+$/.test(retryAfterHeader)
      ? Number(retryAfterHeader)
      : null;
    throw new DashboardRequestError(message, response.status, retryAfterSeconds, nextAllowedAt);
  }
  if (response.status === 204) return undefined as T;
  return response.json() as Promise<T>;
}

export function jsonBody(value: unknown): BodyInit {
  return JSON.stringify(value);
}
