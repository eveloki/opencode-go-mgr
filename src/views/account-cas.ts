/**
 * Shared fail-closed boundary for account control-plane writes.  A missing
 * revision is never treated as an invitation to issue an unguarded request.
 */
export const ACCOUNT_REVISION_UNAVAILABLE_MESSAGE = "无法加载最新设置版本，未执行任何更改。请检查连接后重试。";

export async function withFreshAccountRevision<T>(
  loadRevision: () => Promise<number | null>,
  mutate: (revision: number) => Promise<T>,
): Promise<T> {
  const revision = await loadRevision();
  if (revision === null) throw new Error(ACCOUNT_REVISION_UNAVAILABLE_MESSAGE);
  return mutate(revision);
}

/**
 * Reconcile the account open in the edit modal against a reloaded list after
 * a control-plane conflict: returns the fresh copy when it still exists, or
 * null when it was deleted elsewhere. The caller must close the edit modal on
 * null — a null editing account would otherwise flip the modal into create
 * mode, since the form derives edit-vs-create from account presence.
 */
export function reconcileEditingAccount<T extends { id: string }>(
  accounts: readonly T[],
  editingId: string | null | undefined,
): T | null {
  if (!editingId) return null;
  return accounts.find(({ id }) => id === editingId) ?? null;
}
