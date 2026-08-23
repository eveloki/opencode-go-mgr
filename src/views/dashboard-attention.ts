import type { Account } from "../api/dashboard.ts";
import { isCooling, isFreeCooling } from "./accounts-usage.ts";
import { daysUntilDate } from "./account-lifecycle.ts";
import { isZenFreeAccount } from "./account-providers.ts";

/**
 * The Dashboard "needs attention" area: a single honest list of accounts that
 * need the operator, derived from the same state the Accounts page shows.
 * Disabled accounts are a deliberate choice, not a problem, so they never
 * appear here.
 */

export type AttentionReason =
  | "auth-error"
  | "expired"
  | "cooling"
  | "setup-incomplete"
  | "usage-load-failed"
  | "verification-failed";

export interface AttentionItem {
  accountId: string;
  accountName: string;
  reason: AttentionReason;
}

const REASON_PRIORITY: Record<AttentionReason, number> = {
  "auth-error": 0,
  "verification-failed": 1,
  expired: 2,
  cooling: 3,
  "setup-incomplete": 4,
  "usage-load-failed": 5,
};

function accountCooling(account: Account, now: number): boolean {
  return isZenFreeAccount(account)
    ? isFreeCooling(account, now)
    : isCooling(account, now);
}

export function buildNeedsAttention(
  accounts: readonly Account[],
  usageFailedAccountIds: ReadonlySet<string> = new Set(),
  now: number = Date.now(),
): AttentionItem[] {
  const items: AttentionItem[] = [];
  for (const account of accounts) {
    const ready = account.setup_step === "ready";
    if (ready && account.auth_error) {
      items.push({ accountId: account.id, accountName: account.name, reason: "auth-error" });
      continue;
    }
    if (ready && account.verification_status === "failed") {
      items.push({ accountId: account.id, accountName: account.name, reason: "verification-failed" });
      continue;
    }
    if (ready && account.enabled) {
      const expiryDays = account.expires_on ? daysUntilDate(account.expires_on, now) : Number.POSITIVE_INFINITY;
      if (Number.isFinite(expiryDays) && expiryDays < 0) {
        items.push({ accountId: account.id, accountName: account.name, reason: "expired" });
        continue;
      }
      if (accountCooling(account, now)) {
        items.push({ accountId: account.id, accountName: account.name, reason: "cooling" });
        continue;
      }
    }
    if (!ready) {
      items.push({ accountId: account.id, accountName: account.name, reason: "setup-incomplete" });
      continue;
    }
    if (usageFailedAccountIds.has(account.id)) {
      items.push({ accountId: account.id, accountName: account.name, reason: "usage-load-failed" });
    }
  }
  return items.sort(
    (left, right) => REASON_PRIORITY[left.reason] - REASON_PRIORITY[right.reason]
      || left.accountName.localeCompare(right.accountName),
  );
}
