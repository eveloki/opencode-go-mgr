/**
 * Domain-neutral semver comparison shared by the settings update flow.
 * Kept in utils so non-API modules can compare versions without pulling in
 * the dashboard API surface.
 */
export function isVersionAtLeast(current: string, target: string): boolean {
  const parse = (version: string): [number, number, number] | null => {
    const match = /^v?(\d+)\.(\d+)\.(\d+)$/.exec(version.trim());
    if (!match) return null;
    return [Number(match[1]), Number(match[2]), Number(match[3])];
  };
  const currentParts = parse(current);
  const targetParts = parse(target);
  if (!currentParts || !targetParts) return false;
  for (let index = 0; index < currentParts.length; index += 1) {
    if (currentParts[index] !== targetParts[index]) {
      return currentParts[index] > targetParts[index];
    }
  }
  return true;
}
