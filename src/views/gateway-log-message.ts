import { t } from "../i18n/index.ts";

/**
 * Display mapping for known structured runtime-log messages emitted by the
 * local gateway (see the `消息` column in the Logs view). The backend writes
 * these in English; the UI localizes only the messages it recognizes and
 * passes every other string through untouched, so unknown backend detail —
 * especially error text — is never translated or hidden.
 */

// crates/ocg-core/src/dashboard.rs logs `created account {name}` when an
// account is created through the dashboard API. Everything after the prefix
// is the account name, interpolated verbatim as a display parameter.
const CREATED_ACCOUNT_PATTERN = /^created account ([\s\S]+)$/;

/** Localized display text for a runtime-log message. */
export function gatewayLogMessage(message: string): string {
  const created = CREATED_ACCOUNT_PATTERN.exec(message);
  if (created) {
    return t("已创建账号 {name}", { name: created[1] });
  }
  return message;
}
