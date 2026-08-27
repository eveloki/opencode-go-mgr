import type { ProviderCatalogEntry } from "../api/providers.ts";
import type { MessageKey } from "../i18n/index.ts";
import type { PlanDefinition } from "./plans.ts";
import {
  PLAN_DEFINITIONS,
  findCatalogEntry,
  planFamilyLabel,
  planCreateDisabledReason,
} from "./plans.ts";

/**
 * Plan-option list for the Add Account chooser. Backend-owned singletons
 * (Zen Free) are omitted: they are not created here. Remaining families stay
 * visible so unavailable choices still explain why they cannot be created.
 * Unroutable-but-creatable families appear as drafts instead of implying they
 * will route.
 */

export interface PlanOption {
  plan: PlanDefinition;
  label: string;
  disabled: boolean;
  disabledReason: MessageKey | "";
  /** Honest copy for selectable-but-not-yet-routable families. */
  creationHint: MessageKey | "";
  managed: boolean;
}

export type PlanChooserGroupId = "available" | "draft" | "unavailable";

export interface PlanChooserGroup {
  id: PlanChooserGroupId;
  label: MessageKey;
  options: PlanOption[];
}

const GROUP_ORDER: readonly PlanChooserGroupId[] = ["available", "draft", "unavailable"];

const GROUP_LABEL: Record<PlanChooserGroupId, MessageKey> = {
  available: "可添加",
  draft: "草稿方案",
  unavailable: "暂不可用",
};

/**
 * Human-readable hint shown for selectable families whose post-create state
 * needs honest copy. GOAT is live without a Key-verification gate; Custom may
 * still be verified explicitly after creation.
 */
function planCreationHint(
  plan: PlanDefinition,
  _catalog: readonly ProviderCatalogEntry[] | null | undefined,
): MessageKey | "" {
  if (plan.id === "custom-endpoint") return "创建为禁用账号，验证连接成功后手动启用。";
  return "";
}

/** True when any family offering is routable according to the catalog. */
function planFamilyRoutable(
  plan: PlanDefinition,
  catalog: readonly ProviderCatalogEntry[] | null | undefined,
): boolean {
  if (!catalog?.length) return false;
  return plan.offering_ids.some((offeringId) => {
    const entry = findCatalogEntry(catalog, plan.provider_id, offeringId);
    return entry?.routable === true;
  });
}

export function buildPlanOptions(
  catalog: readonly ProviderCatalogEntry[] | null | undefined,
): PlanOption[] {
  return PLAN_DEFINITIONS.filter((plan) => !plan.singleton).map((plan) => {
    const reason = planCreateDisabledReason(plan, catalog);
    if (reason) {
      return {
        plan,
        label: planFamilyLabel(plan, catalog),
        disabled: true,
        disabledReason: reason,
        creationHint: "",
        managed: false,
      };
    }

    return {
      plan,
      label: planFamilyLabel(plan, catalog),
      disabled: false,
      disabledReason: "",
      creationHint: planCreationHint(plan, catalog),
      managed: plan.managed_registration,
    };
  });
}

export function planChooserGroupId(
  option: PlanOption,
  catalog: readonly ProviderCatalogEntry[] | null | undefined,
): PlanChooserGroupId {
  if (option.disabled) return "unavailable";
  if (!catalog?.length) return "available";
  return planFamilyRoutable(option.plan, catalog) ? "available" : "draft";
}

export function buildPlanChooserGroups(
  catalog: readonly ProviderCatalogEntry[] | null | undefined,
): PlanChooserGroup[] {
  const buckets: Record<PlanChooserGroupId, PlanOption[]> = {
    available: [],
    draft: [],
    unavailable: [],
  };
  for (const option of buildPlanOptions(catalog)) {
    buckets[planChooserGroupId(option, catalog)].push(option);
  }
  return GROUP_ORDER
    .filter((id) => buckets[id].length > 0)
    .map((id) => ({ id, label: GROUP_LABEL[id], options: buckets[id] }));
}
