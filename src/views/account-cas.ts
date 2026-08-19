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
