<template>
  <div v-if="usage?.quota_windows.length" class="provider-quota-summary">
    <div v-for="window in usage.quota_windows" :key="window.window_kind" class="provider-quota-row">
      <div class="provider-quota-row__heading">
        <span>{{ windowLabel(window) }}</span>
        <span>{{ remainingLabel(window) }}</span>
      </div>
      <n-progress
        type="line"
        :percentage="remainingPercent(window)"
        :show-indicator="false"
        :height="6"
        :border-radius="4"
      />
      <time v-if="window.resets_at" class="provider-quota-row__reset">
        {{ new Date(window.resets_at).toLocaleString() }}
      </time>
    </div>
  </div>
</template>

<script setup lang="ts">
import { NProgress } from "naive-ui";
import type { ProviderQuotaWindow, ProviderUsageResponse } from "../api/providers.ts";
import { t } from "../i18n/index.ts";

defineProps<{ usage: ProviderUsageResponse | null }>();

function windowLabel(window: ProviderQuotaWindow): string {
  const kind = window.window_kind;
  if (kind.startsWith("minimax_current:")) {
    const started = window.started_at ? Date.parse(window.started_at) : Number.NaN;
    const ended = window.resets_at ? Date.parse(window.resets_at) : Number.NaN;
    const hours = Math.round((ended - started) / 3_600_000);
    const period = Number.isFinite(hours) && hours > 0 ? ` · ${hours}${t("小时")}` : "";
    return `${kind.slice(16)}${period}`;
  }
  if (kind.startsWith("minimax_weekly:")) return `${kind.slice(15)} · ${t("本周")}`;
  if (kind === "kimi_usage") return t("本周");
  if (kind === "kimi_5h") return t("5小时");
  return kind.replaceAll("_", " ");
}

function remainingPercent(window: ProviderQuotaWindow): number {
  if (window.limit_value === null || window.limit_value <= 0) return 100;
  if (window.unit === "percent") {
    return Math.max(0, Math.min(100, window.limit_value - window.used));
  }
  return Math.max(0, Math.min(100, ((window.limit_value - window.used) / window.limit_value) * 100));
}

function remainingLabel(window: ProviderQuotaWindow): string {
  if (window.limit_value === null) return "∞";
  const remaining = Math.max(0, window.limit_value - window.used);
  if (window.unit === "percent") return `${remaining.toLocaleString()}%`;
  return `${remaining.toLocaleString()} / ${window.limit_value.toLocaleString()}`;
}
</script>

<style scoped>
.provider-quota-summary {
  display: grid;
  gap: 10px;
}

.provider-quota-row {
  display: grid;
  gap: 5px;
}

.provider-quota-row__heading {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  color: var(--ocg-ink);
  font-size: var(--ocg-font-sm);
}

.provider-quota-row__reset {
  color: var(--ocg-muted);
  font-size: var(--ocg-font-xs);
}
</style>
