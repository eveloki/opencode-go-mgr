/**
 * Client-facing Alias selected in the active application controls. Catalog
 * aliases describe availability; they must never override the user's current
 * selector value or make copy depend on catalog fetch success.
 */
export function selectedApplicationAlias(
  modelFields: readonly string[] | undefined,
  modelValues: Readonly<Record<string, string>>,
  selectedModel: string | null | undefined,
): string {
  if (modelFields?.length) {
    const first = modelFields[0];
    return first ? modelValues[first]?.trim() ?? "" : "";
  }
  return selectedModel?.trim() ?? "";
}
