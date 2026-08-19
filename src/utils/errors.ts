import { t } from "../i18n/index.ts";

const NETWORK_ERROR_PATTERN = /failed to fetch|network(?:error| request failed)|load failed/i;

export function userFacingError(error: unknown, networkFallback: string): string {
  if (error instanceof TypeError && NETWORK_ERROR_PATTERN.test(error.message)) {
    return networkFallback;
  }
  return error instanceof Error ? error.message : String(error);
}

/** Error text for dashboard API failures, with the shared network fallback. */
export function dashboardErrorDetail(error: unknown): string {
  return userFacingError(error, t("无法连接到本地服务，请确认程序正在运行后重试"));
}
