import type { Account, AccountCredentialKind, AccountQuotaScope } from "../api/tauri";

/**
 * Built-in provider/offering registry. The backend owns the DTO fields
 * (`provider_id`, `offering_id`, `credential_kind`, `quota_scope`,
 * `free_alias_enabled`); this module only holds the frontend's static
 * knowledge of the built-in pairs so forms and cards can branch without
 * inventing new endpoints.
 */

export type ProviderOffering = {
  provider_id: string;
  offering_id: string;
  /** Display name shown in the account form and cards. */
  label: string;
  credential_kind: AccountCredentialKind;
  quota_scope: AccountQuotaScope;
  /** Managed registration wizard is only available for this pair. */
  managed_registration: boolean;
};

/** Existing and migrated accounts default to OpenCode Go. */
export const DEFAULT_PROVIDER_ID = "opencode";
export const DEFAULT_OFFERING_ID = "go";

/** Built-in singleton Zen Free account; created and owned by the backend. */
export const ZEN_FREE_ACCOUNT_ID = "00000000-0000-0000-0000-000000000002";
export const ZEN_FREE_PROVIDER_ID = "opencode-zen-free";
export const ZEN_FREE_OFFERING_ID = "anonymous-free";

export const PROVIDER_OFFERINGS: readonly ProviderOffering[] = [
  {
    provider_id: DEFAULT_PROVIDER_ID,
    offering_id: DEFAULT_OFFERING_ID,
    label: "OpenCode Go",
    credential_kind: "api_key",
    quota_scope: "key",
    managed_registration: true,
  },
  {
    provider_id: "command-code",
    offering_id: "goat",
    label: "Command Code GOAT",
    credential_kind: "api_key",
    quota_scope: "key",
    managed_registration: false,
  },
];

export const ZEN_FREE_OFFERING: ProviderOffering = {
  provider_id: ZEN_FREE_PROVIDER_ID,
  offering_id: ZEN_FREE_OFFERING_ID,
  label: "Zen Free",
  credential_kind: "none",
  quota_scope: "egress-ip",
  managed_registration: false,
};

export function isZenFreeAccount(
  account: Pick<Account, "id" | "provider_id">,
): boolean {
  return account.id === ZEN_FREE_ACCOUNT_ID
    || account.provider_id === ZEN_FREE_PROVIDER_ID;
}

export function findProviderOffering(
  providerId: string,
  offeringId: string,
): ProviderOffering | undefined {
  if (
    providerId === ZEN_FREE_OFFERING.provider_id
    && offeringId === ZEN_FREE_OFFERING.offering_id
  ) {
    return ZEN_FREE_OFFERING;
  }
  return PROVIDER_OFFERINGS.find(
    (offering) => offering.provider_id === providerId && offering.offering_id === offeringId,
  );
}

export function providerOfferingLabel(
  account: Pick<Account, "provider_id" | "offering_id">,
): string {
  return findProviderOffering(account.provider_id, account.offering_id)?.label
    ?? `${account.provider_id}/${account.offering_id}`;
}
