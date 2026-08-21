import { nextTick, watch, type Ref } from "vue";
import { effectiveCatalog, locale, t } from "../i18n/index.ts";

/**
 * naive-ui's shared NBaseClose hardcodes `aria-label="close"` and ignores the
 * n-config-provider locale, so every modal close control announces English
 * even in a localized UI. These helpers rewrite the rendered close button's
 * accessible name without touching keyboard behavior or the visible design.
 */

/** Localized accessible name for a modal close control. */
export function modalCloseAriaLabel(): string {
  return t("关闭对话框");
}

/** Rewrite naive-ui's hardcoded English close label under `root`. */
export function applyModalCloseAriaLabel(root: ParentNode, modalClass: string): void {
  const label = modalCloseAriaLabel();
  root.querySelectorAll(`.${modalClass} .n-base-close`).forEach((el) => {
    el.setAttribute("aria-label", label);
  });
}

/**
 * Re-apply the localized close label whenever the modal (re)opens or the
 * effective translation catalog changes. Tracking `effectiveCatalog` (not just
 * `locale`) also covers a lazy startup catalog activating while `locale` stays
 * put, e.g. launching with a stored non-zh locale and an already-open modal.
 * The modal body is teleported to document.body, so the patch runs after the
 * next render tick on `document`.
 */
export function useLocalizedModalCloseLabel(show: Ref<boolean>, modalClass: string): void {
  watch([show, locale, effectiveCatalog], async ([visible]) => {
    if (!visible || typeof document === "undefined") return;
    await nextTick();
    applyModalCloseAriaLabel(document, modalClass);
  }, { immediate: true });
}
