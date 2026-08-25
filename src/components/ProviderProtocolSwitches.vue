<template>
  <fieldset class="protocol-policy" :disabled="disabled">
    <legend class="protocol-policy__legend">{{ t("上游协议策略") }}</legend>
    <p class="protocol-policy__copy">
      {{ t("关闭或开启任一协议会立即作用于该范围下的全部账号，并影响生产路由。") }}
    </p>
    <div class="protocol-policy__switches">
      <label
        v-for="protocol in protocols"
        :key="protocol"
        class="protocol-switch"
      >
        <n-switch
          :value="switches[protocol]"
          :loading="loadingProtocol === protocol"
          :disabled="disabled || (loadingProtocol !== null && loadingProtocol !== protocol)"
          :aria-label="switches[protocol] ? t('禁用 {protocol}', { protocol: protocolDisplayName(protocol) }) : t('启用 {protocol}', { protocol: protocolDisplayName(protocol) })"
          @update:value="(enabled: boolean) => emit('change', protocol, enabled)"
        />
        <span>{{ protocolDisplayName(protocol) }}</span>
      </label>
    </div>
  </fieldset>
</template>

<script setup lang="ts">
import { NSwitch } from "naive-ui";
import type { ProviderProtocol, ProtocolSwitches } from "../api/providers.ts";
import { t } from "../i18n/index.ts";
import { PROVIDER_PROTOCOLS, protocolDisplayName } from "../domain/provider-contracts.ts";

defineProps<{
  switches: ProtocolSwitches;
  loadingProtocol: ProviderProtocol | null;
  disabled: boolean;
}>();

const emit = defineEmits<{
  change: [protocol: ProviderProtocol, enabled: boolean];
}>();

const protocols = PROVIDER_PROTOCOLS;
</script>

<style scoped>
.protocol-policy {
  min-width: 0;
  margin: 0;
  padding: 16px;
  border: 1px solid var(--ocg-border);
  border-radius: 10px;
}
.protocol-policy__legend {
  padding: 0 6px;
  color: var(--ocg-ink);
  font-size: var(--ocg-font-lg);
  font-weight: 650;
}
.protocol-policy__copy {
  margin: 0 0 12px;
  color: var(--ocg-muted);
  font-size: var(--ocg-font-sm);
}
.protocol-policy__switches {
  display: flex;
  flex-wrap: wrap;
  gap: 12px 20px;
}
.protocol-switch {
  display: flex;
  align-items: center;
  gap: 8px;
  min-height: 40px;
  color: var(--ocg-ink);
  font-size: var(--ocg-font-md);
}
</style>
