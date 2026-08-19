/** The only valid Zen Free states are Deny, Explicit, and Prefer. */
export type ZenFreeSettings = {
  enabled: boolean;
  free_alias_enabled: boolean;
};

/** Enabled switch: Prefer/Explicit -> Deny, then Deny -> Explicit. */
export function toggleZenFreeEnabled(current: ZenFreeSettings): ZenFreeSettings {
  return current.enabled
    ? { enabled: false, free_alias_enabled: false }
    : { enabled: true, free_alias_enabled: false };
}

/** Alias is only meaningful while Zen Free is enabled. */
export function toggleZenFreeAlias(current: ZenFreeSettings): ZenFreeSettings {
  if (!current.enabled) return { enabled: false, free_alias_enabled: false };
  return { enabled: true, free_alias_enabled: !current.free_alias_enabled };
}
