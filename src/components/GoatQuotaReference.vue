<template>
  <div
    class="goat-pricing-reference"
    role="note"
    :aria-label="t('未知价格不会参与费用估算')"
  >
    <dl class="goat-pricing-summary">
      <div>
        <dt>{{ t("月费") }}</dt>
        <dd>${{ monthlyPriceUsd }}</dd>
      </div>
      <div>
        <dt>{{ t("5 小时额度") }}</dt>
        <dd>${{ GOAT_PRICING_REFERENCE.rollingLimitsUsd.window5h }}</dd>
      </div>
      <div>
        <dt>{{ t("周额度") }}</dt>
        <dd>${{ GOAT_PRICING_REFERENCE.rollingLimitsUsd.windowWeek }}</dd>
      </div>
      <div>
        <dt>{{ t("月额度") }}</dt>
        <dd>$20–$70 / {{ t("模型") }}</dd>
      </div>
      <div>
        <dt>{{ t("模型") }}</dt>
        <dd>{{ rows.length }}</dd>
      </div>
    </dl>

    <p class="goat-pricing-note">
      USD / 1M tokens · {{ t("未知价格不会参与费用估算") }}
    </p>

    <div class="goat-pricing-table-wrap" tabindex="0">
      <table aria-label="Command Code GOAT">
        <thead>
          <tr>
            <th scope="col">{{ t("模型") }}</th>
            <th scope="col">{{ t("输入") }}</th>
            <th scope="col">{{ t("输出") }}</th>
            <th scope="col">{{ t("缓存读") }}</th>
            <th scope="col">{{ t("缓存写") }}</th>
            <th scope="col">{{ t("月额度") }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="row in rows" :key="row.model">
            <th scope="row">{{ row.model }}</th>
            <td>{{ formatOfficialRate(row.input) }}</td>
            <td>{{ formatOfficialRate(row.output) }}</td>
            <td>{{ formatOfficialRate(row.cacheRead) }}</td>
            <td>{{ formatOfficialRate(row.cacheWrite) }}</td>
            <td>{{ formatMonthlyCredits(row.monthlyCreditsUsd) }}</td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { t } from "../i18n/index.ts";
import type { ProviderNeutralPricingSnapshot } from "../api/providers.ts";
import {
  GOAT_PRICING_REFERENCE,
  type GoatOfficialRate,
} from "../domain/pricing-references.ts";

const props = defineProps<{ snapshot?: ProviderNeutralPricingSnapshot | null }>();

const rows = computed(() => {
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
    } satisfies {
      model: string;
      input: GoatOfficialRate;
      output: GoatOfficialRate;
      cacheRead: GoatOfficialRate;
      cacheWrite: GoatOfficialRate;
      monthlyCreditsUsd: number | "free";
    };
  });
});

const monthlyPriceUsd = computed(() => (
  props.snapshot?.values.find((row) => row.paid_plan_price !== null)?.paid_plan_price
  ?? GOAT_PRICING_REFERENCE.monthlyPriceUsd
));

function formatOfficialRate(value: GoatOfficialRate): string {
  if (value === "free") return t("免费");
  if (value === null) return "—";
  return `$${value}`;
}

function formatMonthlyCredits(value: number | "free"): string {
  return value === "free" ? t("免费") : `$${value}`;
}
</script>

<style scoped>
.goat-pricing-reference {
  display: grid;
  gap: 12px;
  min-width: 0;
}

.goat-pricing-summary {
  display: grid;
  grid-template-columns: repeat(5, minmax(112px, 1fr));
  gap: 1px;
  margin: 0;
  overflow: hidden;
  border: 1px solid var(--ocg-border);
  border-radius: 10px;
  background: var(--ocg-border);
}

.goat-pricing-summary > div {
  min-width: 0;
  padding: 10px 12px;
  background: var(--ocg-canvas);
}

.goat-pricing-summary dt {
  margin-bottom: 4px;
  color: var(--ocg-subtle);
  font-size: var(--ocg-font-xs);
}

.goat-pricing-summary dd {
  margin: 0;
  color: var(--ocg-ink);
  font-weight: 600;
}

.goat-pricing-note {
  margin: 0;
  color: var(--ocg-text-3);
  font-size: var(--ocg-font-size-12);
}

.goat-pricing-table-wrap {
  max-height: 520px;
  overflow: auto;
  border: 1px solid var(--ocg-border);
  border-radius: 10px;
}

.goat-pricing-table-wrap:focus-visible {
  outline: 2px solid var(--ocg-primary);
  outline-offset: 2px;
}

table {
  width: 100%;
  min-width: 760px;
  border-collapse: collapse;
  font-size: var(--ocg-font-size-12);
}

th,
td {
  padding: 9px 12px;
  border-bottom: 1px solid var(--ocg-border);
  text-align: right;
  white-space: nowrap;
}

th:first-child {
  text-align: left;
}

thead th {
  position: sticky;
  z-index: 1;
  top: 0;
  color: var(--ocg-subtle);
  background: var(--ocg-canvas);
}

tbody th {
  color: var(--ocg-ink);
  font-weight: 600;
}

tbody tr:last-child th,
tbody tr:last-child td {
  border-bottom: 0;
}

@media (max-width: 900px) {
  .goat-pricing-summary {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }
}

@media (max-width: 640px) {
  .goat-pricing-summary {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}
</style>
