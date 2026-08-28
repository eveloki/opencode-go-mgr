<template>
  <div class="provider-pricing-reference">
    <GoatQuotaReference
      :snapshot="snapshot"
      :saving-model-id="savingModelId"
      :disabled="disabled"
      @save-multiplier="(modelId, multiplier) => emit('save-multiplier', modelId, multiplier)"
    />
  </div>
</template>

<script setup lang="ts">
import GoatQuotaReference from "./GoatQuotaReference.vue";
import type { ProviderNeutralPricingSnapshot } from "../api/providers.ts";

defineProps<{
  snapshot?: ProviderNeutralPricingSnapshot | null;
  savingModelId?: string | null;
  disabled?: boolean;
}>();

const emit = defineEmits<{
  "save-multiplier": [modelId: string, multiplier: number];
}>();
</script>

<style scoped>
.provider-pricing-reference {
  min-width: 0;
}
</style>
