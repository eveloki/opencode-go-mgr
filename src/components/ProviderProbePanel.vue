<template>
  <section class="probe-panel" aria-labelledby="provider-probe-title">
    <h2 id="provider-probe-title">{{ t("协议探测") }}</h2>
    <n-alert
      v-if="unavailable"
      type="info"
      :title="t('该方案暂不支持协议探测')"
      :show-icon="false"
    />
    <form v-else class="probe-panel__form" @submit.prevent="submit">
      <n-alert type="warning" :show-icon="false">
        {{ t("探测会向供应商发送最小真实请求，可能消耗额度。") }}
      </n-alert>
      <n-form-item :label="t('选择测试账号')" path="account">
        <n-select
          :value="accountId"
          :options="accountOptions"
          :placeholder="t('选择测试账号')"
          :aria-label="t('选择测试账号')"
          :disabled="inFlight || accountOptions.length === 0"
          @update:value="emit('update:accountId', $event)"
        />
      </n-form-item>
      <n-form-item :label="t('选择模型')" path="model">
        <n-select
          :value="modelId"
          :options="modelOptions"
          :placeholder="t('选择模型')"
          :aria-label="t('选择模型')"
          filterable
          :disabled="inFlight || modelOptions.length === 0"
          @update:value="emit('update:modelId', $event)"
        />
      </n-form-item>
      <fieldset class="probe-panel__protocols">
        <legend>{{ t("选择协议") }}</legend>
        <label
          v-for="protocol in protocolList"
          :key="protocol"
          class="probe-panel__check"
        >
          <n-checkbox
            :checked="protocols.includes(protocol)"
            :disabled="inFlight"
            :aria-label="protocolDisplayName(protocol)"
            @update:checked="toggleProtocol(protocol, $event)"
          />
          <span>{{ protocolDisplayName(protocol) }}</span>
        </label>
      </fieldset>
      <n-checkbox
        :checked="confirmed"
        :disabled="inFlight"
        @update:checked="emit('update:confirmed', $event)"
      >
        {{ t("我了解这会发送真实最小请求，并可能消耗额度") }}
      </n-checkbox>
      <n-button
        attr-type="submit"
        type="primary"
        :loading="inFlight"
        :disabled="inFlight || !confirmed"
      >
        {{ inFlight ? t("协议探测进行中") : t("发送探测") }}
      </n-button>
    </form>
    <ul v-if="results.length > 0" class="probe-panel__results">
      <li
        v-for="result in results"
        :key="result.protocol"
      >
        <strong>{{ protocolDisplayName(result.protocol) }}</strong>
        <span v-if="result.skipped">{{ t("探测已跳过") }}</span>
        <span v-else-if="result.success">{{ t("探测完成") }}</span>
        <span v-else>{{ t("探测失败: {error}", { error: result.error || "" }) }}</span>
      </li>
    </ul>
  </section>
</template>

<script setup lang="ts">
import { computed, watch } from "vue";
import {
  NAlert,
  NButton,
  NCheckbox,
  NFormItem,
  NSelect,
} from "naive-ui";
import type {
  EffectiveModelContract,
  ProtocolProbeResult,
  ProviderAccountChoice,
  ProviderProtocol,
} from "../api/providers.ts";
import { t } from "../i18n/index.ts";
import { PROVIDER_PROTOCOLS, protocolDisplayName, uniqueProtocols } from "../domain/provider-contracts.ts";

const props = defineProps<{
  unavailable: boolean;
  accountId: string | null;
  modelId: string | null;
  protocols: ProviderProtocol[];
  confirmed: boolean;
  inFlight: boolean;
  accounts: readonly ProviderAccountChoice[];
  models: readonly string[];
  modelContracts: readonly EffectiveModelContract[];
  results: readonly ProtocolProbeResult[];
}>();

const emit = defineEmits<{
  "update:accountId": [value: string | null];
  "update:modelId": [value: string | null];
  "update:protocols": [value: ProviderProtocol[]];
  "update:confirmed": [value: boolean];
  probe: [];
}>();

// Only protocols inside the selected model's safety ceiling (available
// evidence) are probe candidates; anything else is rejected by the backend.
const protocolList = computed<ProviderProtocol[]>(() => {
  const contract = props.modelContracts.find((model) => model.model_id === props.modelId);
  if (!contract) return [];
  return PROVIDER_PROTOCOLS.filter((protocol) => contract.protocols[protocol]?.available);
});
const accountOptions = computed(() => props.accounts.map((account) => ({
  label: account.name,
  value: account.id,
})));
const modelOptions = computed(() => props.models.map((modelId) => ({
  label: modelId,
  value: modelId,
})));

// A model switch can leave checked protocols outside the new ceiling.
watch(protocolList, (allowed) => {
  const pruned = props.protocols.filter((protocol) => allowed.includes(protocol));
  if (pruned.length !== props.protocols.length) emit("update:protocols", pruned);
});

function toggleProtocol(protocol: ProviderProtocol, checked: boolean) {
  const next = checked
    ? uniqueProtocols([...props.protocols, protocol])
    : props.protocols.filter((item) => item !== protocol);
  emit("update:protocols", next);
}

function submit() {
  if (props.unavailable || props.inFlight || !props.confirmed) return;
  emit("probe");
}
</script>

<style scoped>
.probe-panel {
  min-width: 0;
}
.probe-panel h2 {
  margin: 0 0 10px;
  color: var(--ocg-ink);
  font: 700 var(--ocg-font-lg)/1.3 "Bahnschrift", "Segoe UI Variable Display", sans-serif;
}
.probe-panel__form {
  display: grid;
  gap: 12px;
  min-width: 0;
}
.probe-panel__protocols {
  min-width: 0;
  margin: 0;
  padding: 12px;
  border: 1px solid var(--ocg-border);
  border-radius: 10px;
}
.probe-panel__protocols legend {
  padding: 0 6px;
  color: var(--ocg-ink);
  font-size: var(--ocg-font-sm);
}
.probe-panel__check {
  display: flex;
  align-items: center;
  gap: 8px;
  min-height: 40px;
}
.probe-panel__results {
  display: grid;
  gap: 6px;
  margin: 12px 0 0;
  padding: 0;
  list-style: none;
  color: var(--ocg-ink);
  font-size: var(--ocg-font-sm);
}
.probe-panel__results span {
  margin-left: 8px;
  color: var(--ocg-muted);
}
</style>
