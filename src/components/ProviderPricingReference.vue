<template>
  <div class="provider-pricing-reference">
    <GoatQuotaReference :snapshot="snapshot" />
  </div>
</template>

<script setup lang="ts">
import GoatQuotaReference from "./GoatQuotaReference.vue";
import type { ProviderNeutralPricingSnapshot } from "../api/providers.ts";

defineProps<{
  snapshot?: ProviderNeutralPricingSnapshot | null;
}>();
</script>

<style scoped>
.provider-pricing-reference {
  min-width: 0;
}
</style>
