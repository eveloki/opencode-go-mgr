<template>
  <div class="provider-pricing-reference">
    <div class="provider-pricing-reference__meta">
      <n-tag type="warning" size="small" :bordered="false">
        {{ t("订阅制") }}
      </n-tag>
      <n-button
        tag="a"
        text
        :href="sourceUrl"
        target="_blank"
        rel="noopener noreferrer"
      >{{ t("官方来源") }}</n-button>
    </div>

    <GoatQuotaReference :snapshot="snapshot" />
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { NButton, NTag } from "naive-ui";
import { t } from "../i18n/index.ts";
import { GOAT_PRICING_REFERENCE } from "../domain/pricing-references.ts";
import GoatQuotaReference from "./GoatQuotaReference.vue";
import type { ProviderNeutralPricingSnapshot } from "../api/providers.ts";

const props = defineProps<{
  snapshot?: ProviderNeutralPricingSnapshot | null;
}>();

const sourceUrl = computed(() => props.snapshot?.source_url ?? GOAT_PRICING_REFERENCE.sourceUrl);
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

.reference-note {
  margin: 0;
  color: var(--ocg-muted);
}

@media (max-width: 640px) {
  .provider-pricing-reference__meta {
    align-items: flex-start;
    flex-direction: column;
  }
}
</style>
