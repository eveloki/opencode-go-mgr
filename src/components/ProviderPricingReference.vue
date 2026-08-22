<template>
  <div class="provider-pricing-reference">
    <div class="provider-pricing-reference__meta">
      <n-tag type="warning" size="small" :bordered="false">
        {{ t("官方套餐参考 · 截至 {date}", { date: PRICING_REFERENCE_CHECKED_AT }) }}
      </n-tag>
      <n-button
        tag="a"
        text
        :href="sourceUrl"
        target="_blank"
        rel="noopener noreferrer"
      >{{ t("官方来源") }}</n-button>
    </div>

    <template v-if="kind === 'goat'">
      <dl class="reference-metrics">
        <div>
          <dt>{{ t("月费") }}</dt>
          <dd>
            <span>${{ GOAT_PRICING_REFERENCE.monthlyPriceUsd }}</span>
            <small>{{ t("另加处理费") }}</small>
          </dd>
        </div>
        <div>
          <dt>{{ t("每月含额度") }}</dt>
          <dd>${{ GOAT_PRICING_REFERENCE.monthlyCreditsUsd }}</dd>
        </div>
        <div>
          <dt>{{ t("官方估算请求数") }}</dt>
          <dd>≈ {{ GOAT_PRICING_REFERENCE.approximateRequests }}</dd>
        </div>
      </dl>

      <h3>{{ t("滚动额度限制") }}</h3>
      <dl class="rolling-limits">
        <div v-for="limit in GOAT_PRICING_REFERENCE.rollingLimits" :key="limit.window">
          <dt>{{ t(limit.window) }}</dt>
          <dd>${{ limit.creditsUsd }}</dd>
        </div>
      </dl>
      <p class="reference-note">
        {{ t("请求数是官方估算，实际取决于模型、tokens 与缓存；部分模型有单独额度。") }}
      </p>
    </template>

    <template v-else>
      <div class="reference-table-scroll">
        <table class="reference-table">
          <thead>
            <tr>
              <th scope="col">{{ t("套餐") }}</th>
              <th scope="col">{{ t("活动价") }}</th>
              <th scope="col">{{ t("原价") }}</th>
              <th scope="col">{{ t("每月 Credits") }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="tier in SCNET_PRICING_REFERENCE.tiers" :key="tier.id">
              <th scope="row">{{ t(tier.label) }}</th>
              <td><strong>¥{{ tier.promotionalPriceCny }}</strong></td>
              <td><del>¥{{ tier.listPriceCny }}</del></td>
              <td>{{ formatNumber(tier.monthlyCredits) }}</td>
            </tr>
          </tbody>
        </table>
      </div>
      <p class="reference-note">
        {{ t("额度用尽不会转按量计费，到期未用额度不结转；实际价格和余额以 SCNet 控制台为准。") }}
      </p>
      <n-alert type="warning" :show-icon="false">
        {{ t("仅限 AI 工具内交互式使用；禁止共享账号、自动化脚本、自定义应用后端及非交互批量调用。") }}
        <n-button
          tag="a"
          text
          :href="SCNET_PRICING_REFERENCE.restrictionsUrl"
          target="_blank"
          rel="noopener noreferrer"
        >{{ t("查看使用限制") }}</n-button>
      </n-alert>
    </template>

    <n-alert type="info" :show-icon="false">
      {{ t("当前仍是禁用草稿；这里仅展示官方参考，不代表 OCG Manager 已支持路由、验证或实时用量。") }}
    </n-alert>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { NAlert, NButton, NTag } from "naive-ui";
import { locale, t } from "../i18n/index.ts";
import {
  GOAT_PRICING_REFERENCE,
  PRICING_REFERENCE_CHECKED_AT,
  SCNET_PRICING_REFERENCE,
} from "../views/pricing-references.ts";

const props = defineProps<{ kind: "goat" | "scnet" }>();

const sourceUrl = computed(() => props.kind === "goat"
  ? GOAT_PRICING_REFERENCE.sourceUrl
  : SCNET_PRICING_REFERENCE.sourceUrl);

function formatNumber(value: number): string {
  return new Intl.NumberFormat(locale.value).format(value);
}
</script>

<style scoped>
.provider-pricing-reference {
  display: grid;
  gap: 16px;
}

.provider-pricing-reference__meta {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.reference-metrics,
.rolling-limits {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 10px;
  margin: 0;
}

.reference-metrics > div,
.rolling-limits > div {
  padding: 14px;
  border: 1px solid var(--ocg-divider);
  border-radius: 10px;
  background: var(--ocg-canvas);
}

.reference-metrics dt,
.rolling-limits dt {
  color: var(--ocg-subtle);
  font-size: var(--ocg-font-xs);
}

.reference-metrics dd,
.rolling-limits dd {
  margin: 4px 0 0;
  color: var(--ocg-ink);
  font-size: var(--ocg-font-xl);
  font-weight: 700;
  font-variant-numeric: tabular-nums;
}

.reference-metrics small {
  color: var(--ocg-subtle);
  display: block;
  font-size: var(--ocg-font-xs);
  font-weight: 400;
}

.provider-pricing-reference h3 {
  margin: 0;
  color: var(--ocg-ink);
  font-size: var(--ocg-font-md);
}

.rolling-limits {
  grid-template-columns: repeat(2, minmax(0, 1fr));
}

.reference-table-scroll {
  overflow-x: auto;
  border: 1px solid var(--ocg-divider);
  border-radius: 10px;
}

.reference-table {
  width: 100%;
  min-width: 520px;
  border-collapse: collapse;
}

.reference-table th,
.reference-table td {
  padding: 12px 14px;
  border-bottom: 1px solid var(--ocg-divider);
  text-align: left;
  font-variant-numeric: tabular-nums;
}

.reference-table thead th {
  background: var(--ocg-canvas);
  color: var(--ocg-subtle);
  font-size: var(--ocg-font-xs);
}

.reference-table tbody tr:last-child > * {
  border-bottom: 0;
}

.reference-table td strong {
  color: var(--ocg-primary);
}

.reference-table del {
  color: var(--ocg-subtle);
}

.reference-note {
  margin: 0;
  color: var(--ocg-muted);
}

@media (max-width: 640px) {
  .reference-metrics,
  .rolling-limits {
    grid-template-columns: 1fr;
  }

  .provider-pricing-reference__meta {
    align-items: flex-start;
    flex-direction: column;
  }
}
</style>
