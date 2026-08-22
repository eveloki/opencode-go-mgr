<template>
  <section class="model-contracts" aria-labelledby="provider-models-title">
    <h2 id="provider-models-title">{{ t("模型合约") }}</h2>
    <p v-if="models.length === 0" class="model-contracts__empty">{{ t("暂无模型合约") }}</p>
    <n-collapse v-else arrow-placement="right">
      <n-collapse-item
        v-for="model in models"
        :key="model.model_id"
        :name="model.model_id"
      >
        <template #header>
          <div class="model-contracts__header">
            <code>{{ model.model_id }}</code>
            <span>{{ t("首选协议：{protocol}", { protocol: protocolDisplayName(model.preferred_protocol) }) }}</span>
          </div>
        </template>
        <ul class="model-contracts__protocols">
          <li
            v-for="protocol in protocols"
            :key="protocol"
            class="model-protocol"
          >
            <span class="model-protocol__name">{{ protocolDisplayName(protocol) }}</span>
            <span class="model-protocol__status">{{ statusLabel(protocol, model) }}</span>
            <span
              v-if="failureDetail(protocol, model)"
              class="model-protocol__error"
            >{{ failureDetail(protocol, model) }}</span>
          </li>
        </ul>
        <ul v-if="model.disabled_reasons.length > 0" class="model-contracts__reasons">
          <li v-for="reason in model.disabled_reasons" :key="reason">{{ reason }}</li>
        </ul>
      </n-collapse-item>
    </n-collapse>
  </section>
</template>

<script setup lang="ts">
import { NCollapse, NCollapseItem } from "naive-ui";
import type {
  EffectiveModelContract,
  ProviderProtocol,
  ProtocolSwitches,
} from "../api/providers.ts";
import { t } from "../i18n/index.ts";
import type { MessageKey } from "../i18n/index.ts";
import {
  PROVIDER_PROTOCOLS,
  protocolDisplayName,
  protocolEvidenceStatus,
} from "../views/provider-contracts.ts";

const props = defineProps<{
  models: readonly EffectiveModelContract[];
  switches: ProtocolSwitches;
}>();

const protocols = PROVIDER_PROTOCOLS;

const statusKeys: Record<string, MessageKey> = {
  globally_closed: "全局关闭",
  unavailable: "不可用",
  unsupported: "不支持",
  static: "静态",
  preset: "预设",
  probe_confirmed: "探测已确认",
  probe_failure: "最近探测失败",
};

function statusLabel(protocol: ProviderProtocol, model: EffectiveModelContract): string {
  const status = protocolEvidenceStatus(protocol, model.protocols[protocol], props.switches);
  return t(statusKeys[status] ?? "不可用");
}

function formatTime(value: string | null): string {
  if (!value) return "";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString();
}

function failureDetail(protocol: ProviderProtocol, model: EffectiveModelContract): string {
  const evidence = model.protocols[protocol];
  if (!evidence || protocolEvidenceStatus(protocol, evidence, props.switches) !== "probe_failure") {
    return "";
  }
  const parts: string[] = [];
  if (evidence.last_probe_error) {
    parts.push(t("探测失败: {error}", { error: evidence.last_probe_error }));
  }
  if (evidence.last_probe_at) {
    parts.push(t("上次探测：{time}", { time: formatTime(evidence.last_probe_at) }));
  }
  return parts.join(" · ");
}
</script>

<style scoped>
.model-contracts {
  min-width: 0;
}
.model-contracts h2 {
  margin: 0 0 10px;
  color: var(--ocg-ink);
  font: 700 var(--ocg-font-lg)/1.3 "Bahnschrift", "Segoe UI Variable Display", sans-serif;
}
.model-contracts__empty,
.model-contracts__header span,
.model-protocol__status,
.model-protocol__error,
.model-contracts__reasons {
  color: var(--ocg-muted);
  font-size: var(--ocg-font-sm);
}
.model-contracts__header {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px 16px;
  min-width: 0;
}
.model-contracts__header code {
  overflow-wrap: anywhere;
  color: var(--ocg-ink);
  font-size: var(--ocg-font-sm);
}
.model-contracts__protocols,
.model-contracts__reasons {
  margin: 0;
  padding: 0 0 8px 4px;
  list-style: none;
}
.model-protocol {
  display: grid;
  gap: 2px;
  padding: 8px 0;
  border-top: 1px solid var(--ocg-divider);
}
.model-protocol__name {
  color: var(--ocg-ink);
  font-size: var(--ocg-font-md);
}
.model-protocol__error,
.model-contracts__reasons li {
  overflow-wrap: anywhere;
}
</style>
