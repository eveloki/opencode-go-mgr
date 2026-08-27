<template>
  <div class="goat-pricing-reference">
    <dl class="pricing-ledger">
      <div class="pricing-ledger__revision">
        <dt>{{ t("修订版本") }}</dt>
        <dd><code>{{ snapshot?.revision ?? `static-${PRICING_REFERENCE_CHECKED_AT}` }}</code></dd>
      </div>
      <div>
        <dt>{{ t("启用时间") }}</dt>
        <dd>{{ snapshot ? formatTimestamp(snapshot.activated_at) : "—" }}</dd>
      </div>
      <div>
        <dt>{{ t("文档更新时间") }}</dt>
        <dd>{{ documentUpdatedAt }}</dd>
      </div>
      <div>
        <dt>{{ t("月费") }}</dt>
        <dd>{{ formatUsd(monthlyPriceUsd) }}</dd>
      </div>
      <div>
        <dt>{{ t("5 小时额度") }}</dt>
        <dd>{{ formatUsd(GOAT_PRICING_REFERENCE.rollingLimitsUsd.window5h) }}</dd>
      </div>
      <div>
        <dt>{{ t("周额度") }}</dt>
        <dd>{{ formatUsd(GOAT_PRICING_REFERENCE.rollingLimitsUsd.windowWeek) }}</dd>
      </div>
    </dl>

    <p class="pricing-note">
      USD / 1M tokens · {{ t("未知价格不会参与费用估算") }}
    </p>

    <n-data-table
      :columns="columns"
      :data="rows"
      :pagination="false"
      :row-key="rowKey"
      :scroll-x="870"
      size="small"
    />
  </div>
</template>

<script setup lang="ts">
import { computed, h } from "vue";
import { NDataTable, NTooltip } from "naive-ui";
import type { DataTableColumns } from "naive-ui";
import { locale, t } from "../i18n/index.ts";
import type { ProviderNeutralPricingSnapshot } from "../api/providers.ts";
import { formatPricingRate } from "../domain/pricing-view.ts";
import {
  GOAT_PRICING_REFERENCE,
  PRICING_REFERENCE_CHECKED_AT,
  type GoatOfficialRate,
} from "../domain/pricing-references.ts";

interface GoatPricingRow {
  model: string;
  input: GoatOfficialRate;
  output: GoatOfficialRate;
  cacheRead: GoatOfficialRate;
  cacheWrite: GoatOfficialRate;
  monthlyCreditsUsd: number | "free";
}

const props = defineProps<{ snapshot?: ProviderNeutralPricingSnapshot | null }>();

const rows = computed<GoatPricingRow[]>(() => {
  if (!props.snapshot) return [...GOAT_PRICING_REFERENCE.models];
  return props.snapshot.values.map((row) => {
    const free = row.input_per_million === null
      && row.output_per_million === null
      && row.cache_read_per_million === null;
    return {
      model: row.display_name,
      input: free ? "free" : row.input_per_million,
      output: free ? "free" : row.output_per_million,
      cacheRead: free ? "free" : row.cache_read_per_million,
      cacheWrite: row.cache_write_per_million,
      monthlyCreditsUsd: free ? "free" : (row.model_allowance ?? 0),
    };
  });
});

const monthlyPriceUsd = computed(() => (
  props.snapshot?.values.find((row) => row.paid_plan_price !== null)?.paid_plan_price
  ?? GOAT_PRICING_REFERENCE.monthlyPriceUsd
));

const documentUpdatedAt = computed(() => {
  const value = props.snapshot?.document_updated_at;
  return value ? formatTimestamp(value) : PRICING_REFERENCE_CHECKED_AT;
});

function formatTimestamp(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat(locale.value, {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(date);
}

function formatUsd(value: number): string {
  return new Intl.NumberFormat(locale.value, {
    style: "currency",
    currency: "USD",
    currencyDisplay: "narrowSymbol",
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  }).format(value);
}

function renderOfficialRate(value: GoatOfficialRate) {
  if (value === "free") return t("免费");
  const formatted = formatPricingRate(value, locale.value);
  if (!formatted.exact) return formatted.label;
  const exactLabel = t("精确值：{value} / 百万 tokens", { value: formatted.exact });
  return h(NTooltip, { trigger: "focus" }, {
    trigger: () => h("span", {
      class: "tiny-rate",
      tabindex: 0,
      title: exactLabel,
      "aria-label": `${formatted.label}, ${exactLabel}`,
    }, formatted.label),
    default: () => exactLabel,
  });
}

function renderMonthlyCredits(value: number | "free") {
  return value === "free" ? t("免费") : formatUsd(value);
}

const columns = computed<DataTableColumns<GoatPricingRow>>(() => [
  {
    title: t("模型"),
    key: "model",
    width: 190,
    fixed: "left",
    ellipsis: { tooltip: true },
  },
  { title: t("输入"), key: "input", width: 112, align: "right", render: (row) => renderOfficialRate(row.input) },
  { title: t("输出"), key: "output", width: 112, align: "right", render: (row) => renderOfficialRate(row.output) },
  { title: t("缓存读"), key: "cacheRead", width: 112, align: "right", render: (row) => renderOfficialRate(row.cacheRead) },
  { title: t("缓存写"), key: "cacheWrite", width: 112, align: "right", render: (row) => renderOfficialRate(row.cacheWrite) },
  { title: t("月额度"), key: "monthlyCreditsUsd", width: 130, align: "right", render: (row) => renderMonthlyCredits(row.monthlyCreditsUsd) },
]);

function rowKey(row: GoatPricingRow): string {
  return row.model;
}
</script>

<style scoped>
.goat-pricing-reference {
  min-width: 0;
  width: 100%;
}

.pricing-ledger {
  display: grid;
  grid-template-columns: minmax(180px, 1.4fr) repeat(5, minmax(112px, 1fr));
  gap: 1px;
  margin: 0 0 14px;
  overflow: hidden;
  border: 1px solid var(--ocg-border);
  border-radius: 10px;
  background: var(--ocg-border);
  font-variant-numeric: tabular-nums;
}

.pricing-ledger > div {
  min-width: 0;
  padding: 10px 12px;
  background: var(--ocg-canvas);
}

.pricing-ledger dt {
  margin-bottom: 4px;
  color: var(--ocg-subtle);
  font-size: var(--ocg-font-xs);
}

.pricing-ledger dd {
  overflow: hidden;
  margin: 0;
  color: var(--ocg-ink);
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.pricing-ledger code {
  font-family: "Cascadia Mono", Consolas, monospace;
}

.pricing-note {
  margin: 0 0 10px;
  color: var(--ocg-subtle);
  font-size: var(--ocg-font-sm);
}

:deep(.n-data-table-td) {
  font-variant-numeric: tabular-nums;
}

:deep(.tiny-rate) {
  border-bottom: 1px dotted currentColor;
  cursor: help;
}

@media (max-width: 900px) {
  .pricing-ledger {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }
}

@media (max-width: 640px) {
  .pricing-ledger {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}
</style>
