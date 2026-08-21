import type { ProviderCatalogEntry } from "../api/providers.ts";
import type { MessageKey } from "../i18n/index.ts";
import type { PlanDefinition } from "./plans.ts";
import {
  PLAN_DEFINITIONS,
  planFamilyLabel,
  planCreateDisabledReason,
} from "./plans.ts";

/**
 * Plan-option list for the Add Account modal. Every family is shown so users
 * see why a choice is unavailable. Selectable families may still be unroutable
 * today; the option exposes an honest `creationHint` instead of implying they
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

/**
 * Human-readable hint shown for selectable families that create a disabled
 * draft because the backend does not route them yet.
 */
function planCreationHint(plan: PlanDefinition): MessageKey | "" {
  if (plan.id === "command-code-goat") return "创建为禁用草稿；验证与路由尚未就绪";
  if (plan.id === "scnet") return "选择套餐后创建为禁用草稿；路由尚未就绪";
  if (plan.id === "custom-endpoint") return "创建为禁用账号，验证连接成功后手动启用。";
  return "";
}

export function buildPlanOptions(
  catalog: readonly ProviderCatalogEntry[] | null | undefined,
): PlanOption[] {
  return PLAN_DEFINITIONS.map((plan) => {
    if (plan.singleton) {
      return {
        plan,
        label: planFamilyLabel(plan, catalog),
        disabled: true,
        disabledReason: "单例方案由系统自动管理",
        creationHint: "",
        managed: false,
      };
    }

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
      creationHint: planCreationHint(plan),
      managed: plan.managed_registration,
    };
  });
}
