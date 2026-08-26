<template>
  <div class="provider-model-matrix">
    <div class="matrix-scroll">
      <table class="matrix-table">
        <thead>
          <tr>
            <th class="matrix-cell matrix-cell--model-header">{{ t("模型") }}</th>
            <th
              v-for="protocol in PROVIDER_PROTOCOLS"
              :key="protocol"
              class="matrix-cell matrix-cell--protocol-header"
            >
              <div class="protocol-header">
                <span>{{ protocolDisplayName(protocol) }}</span>
                <n-dropdown
                  :options="columnBatchOptions(protocol)"
                  trigger="click"
                  @select="(key) => applyColumnBatch(protocol, String(key) as ProtocolOverrideState)"
                >
                  <n-button
                    text
                    size="tiny"
                    :disabled="loading"
                    :aria-label="t('本列全部')"
                  >
                    <template #icon>
                      <n-icon :component="MoreOutlined" />
                    </template>
                  </n-button>
                </n-dropdown>
              </div>
            </th>
            <th class="matrix-cell matrix-cell--actions-header">{{ t("操作") }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="modelId in matrixModels" :key="modelId">
            <td class="matrix-cell matrix-cell--model">
              <code>{{ modelId }}</code>
            </td>
            <td
              v-for="protocol in PROVIDER_PROTOCOLS"
              :key="protocol"
              class="matrix-cell matrix-cell--state"
            >
              <div class="cell-content">
                <n-radio-group
                  :value="cellOverride(modelId, protocol)"
                  size="small"
                  :disabled="loading"
                  @update:value="(state) => updateSingle(modelId, protocol, state as ProtocolOverrideState)"
                >
                  <n-radio-button value="auto" :label="t('自动')" />
                  <n-radio-button
                    value="force_on"
                    :label="t('开')"
                    :disabled="!canForceOn(modelId, protocol)"
                  />
                  <n-radio-button value="force_off" :label="t('关')" />
                </n-radio-group>
                <span
                  v-if="cellForced(modelId, protocol)"
                  class="override-badge"
                >{{ t("强制") }}</span>
                <span
                  class="status-label"
                  :class="`status-label--${cellStatus(modelId, protocol)}`"
                >{{ cellStatusLabel(modelId, protocol) }}</span>
              </div>
            </td>
            <td class="matrix-cell matrix-cell--actions">
              <n-dropdown
                :options="rowBatchOptions"
                trigger="click"
                @select="(key) => applyRowBatch(modelId, String(key) as ProtocolOverrideState)"
              >
                <n-button
                  text
                  size="tiny"
                  :disabled="loading"
                  :aria-label="t('本行全部')"
                >
                  <template #icon>
                    <n-icon :component="MoreOutlined" />
                  </template>
                </n-button>
              </n-dropdown>
              <n-popconfirm
                v-if="scope.scope_kind !== 'custom_endpoint'"
                @positive-click="runRowProbe(modelId)"
              >
                <template #trigger>
                  <n-button
                    size="tiny"
                    :loading="probingModels?.has(modelId) ?? false"
                    :disabled="loading || (probingModels?.has(modelId) ?? false) || probeAccounts.length === 0"
                  >
                    {{ t("测试") }}
                  </n-button>
                </template>
                {{ t("探测会向上游发送真实最小请求，可能消耗额度。是否继续？") }}
              </n-popconfirm>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import {
  NButton,
  NDropdown,
  NIcon,
  NPopconfirm,
  NRadioButton,
  NRadioGroup,
} from "naive-ui";
import { MoreOutlined } from "@vicons/antd";
import type { DropdownOption } from "naive-ui";
import type {
  ContractScopeKind,
  EffectiveModelContract,
  EffectiveProtocolEvidence,
  ModelProtocolOverrideUpdate,
  ProviderAccountChoice,
  ProviderProtocol,
  ProtocolOverrideState,
} from "../api/providers.ts";
import type { ProviderScopeView } from "../domain/provider-contracts.ts";
import { t } from "../i18n/index.ts";
import type { MessageKey } from "../i18n/index.ts";
import {
  protocolDisplayName,
  protocolEvidenceStatus,
  PROVIDER_PROTOCOLS,
  scopeAccounts,
} from "../domain/provider-contracts.ts";

const props = defineProps<{
  scope: ProviderScopeView;
  loading?: boolean;
  probingModels?: Set<string>;
}>();

const emit = defineEmits<{
  (
    e: "update:overrides",
    payload: {
      scopeKind: ContractScopeKind;
      scopeId: string;
      overrides: ModelProtocolOverrideUpdate[];
    },
  ): void;
  (e: "probe", payload: { modelId: string; accountId: string }): void;
  (e: "error", message: string): void;
}>();

const statusKeys: Record<string, MessageKey> = {
  unavailable: "不可用",
  unsupported: "不支持",
  static: "静态",
  preset: "预设",
  probe_confirmed: "探测已确认",
  probe_failure: "最近探测失败",
};

const matrixModels = computed(() => {
  const ids = new Set<string>(props.scope.catalog.models);
  for (const model of props.scope.models) ids.add(model.model_id);
  return [...ids].sort();
});

const probeAccounts = computed<ProviderAccountChoice[]>(() => {
  const accounts = scopeAccounts(props.scope);
  if (props.scope.provider_id !== "command-code") return accounts;
  return accounts.filter((account) => account.verification_status === "verified");
});

function modelContract(modelId: string): EffectiveModelContract | undefined {
  return props.scope.models.find((model) => model.model_id === modelId);
}

function cellEvidence(modelId: string, protocol: ProviderProtocol): EffectiveProtocolEvidence | undefined {
  return modelContract(modelId)?.protocols[protocol];
}

function cellOverride(modelId: string, protocol: ProviderProtocol): ProtocolOverrideState {
  return cellEvidence(modelId, protocol)?.override ?? "auto";
}

function cellForced(modelId: string, protocol: ProviderProtocol): boolean {
  return cellOverride(modelId, protocol) !== "auto";
}

function canForceOn(modelId: string, protocol: ProviderProtocol): boolean {
  return cellEvidence(modelId, protocol)?.available === true;
}

function cellStatus(modelId: string, protocol: ProviderProtocol): string {
  return protocolEvidenceStatus(protocol, cellEvidence(modelId, protocol));
}

function cellStatusLabel(modelId: string, protocol: ProviderProtocol): string {
  return t(statusKeys[cellStatus(modelId, protocol)] ?? "不可用");
}

function makeOverrides(
  modelIds: string[],
  protocols: ProviderProtocol[],
  state: ProtocolOverrideState,
): ModelProtocolOverrideUpdate[] {
  const overrides: ModelProtocolOverrideUpdate[] = [];
  for (const modelId of modelIds) {
    for (const protocol of protocols) {
      overrides.push({ model_id: modelId, protocol, state });
    }
  }
  return overrides;
}

function emitOverrides(overrides: ModelProtocolOverrideUpdate[]): void {
  if (overrides.length === 0) return;
  emit("update:overrides", {
    scopeKind: props.scope.scope_kind,
    scopeId: props.scope.scope_id,
    overrides,
  });
}

function updateSingle(
  modelId: string,
  protocol: ProviderProtocol,
  state: ProtocolOverrideState,
): void {
  emitOverrides([{ model_id: modelId, protocol, state }]);
}

function applyRowBatch(modelId: string, state: ProtocolOverrideState): void {
  emitOverrides(makeOverrides([modelId], [...PROVIDER_PROTOCOLS], state));
}

function applyColumnBatch(protocol: ProviderProtocol, state: ProtocolOverrideState): void {
  emitOverrides(makeOverrides(matrixModels.value, [protocol], state));
}

function runRowProbe(modelId: string): void {
  const account = probeAccounts.value[0];
  if (!account) {
    emit("error", t("该模型无测试账号"));
    return;
  }
  emit("probe", { modelId, accountId: account.id });
}

const rowBatchOptions: DropdownOption[] = [
  { key: "auto", label: t("本行全部：自动") },
  { key: "force_on", label: t("本行全部：强制开启") },
  { key: "force_off", label: t("本行全部：强制关闭") },
];

function columnBatchOptions(protocol: ProviderProtocol): DropdownOption[] {
  const name = protocolDisplayName(protocol);
  return [
    { key: "auto", label: t("本列全部：自动", { protocol: name }) },
    { key: "force_on", label: t("本列全部：强制开启", { protocol: name }) },
    { key: "force_off", label: t("本列全部：强制关闭", { protocol: name }) },
  ];
}
</script>

<style scoped>
.provider-model-matrix {
  min-width: 0;
}
.matrix-scroll {
  overflow-x: auto;
}
.matrix-table {
  width: 100%;
  min-width: 720px;
  border-collapse: collapse;
  font-size: var(--ocg-font-sm);
}
.matrix-cell {
  padding: 10px 12px;
  border-bottom: 1px solid var(--ocg-divider);
  text-align: left;
  vertical-align: middle;
}
.matrix-cell--model-header,
.matrix-cell--protocol-header,
.matrix-cell--actions-header {
  position: sticky;
  top: 0;
  z-index: 1;
  color: var(--ocg-subtle);
  font-size: var(--ocg-font-xs);
  font-weight: 600;
  background: var(--ocg-surface);
}
.matrix-cell--model {
  min-width: 180px;
  max-width: 260px;
}
.matrix-cell--model code {
  overflow-wrap: anywhere;
  color: var(--ocg-ink);
  font-size: var(--ocg-font-sm);
}
.matrix-cell--state {
  min-width: 220px;
}
.matrix-cell--actions {
  min-width: 96px;
  white-space: nowrap;
}
.protocol-header {
  display: flex;
  align-items: center;
  gap: 8px;
}
.cell-content {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
}
.status-label {
  padding: 1px 6px;
  border-radius: 4px;
  font-size: var(--ocg-font-xs);
  font-weight: 600;
}
.status-label--unsupported {
  color: var(--ocg-muted);
  background: var(--ocg-canvas);
}
.status-label--unavailable {
  color: var(--ocg-muted);
  background: var(--ocg-canvas);
}
.status-label--static {
  color: var(--ocg-info);
  background: color-mix(in srgb, var(--ocg-info) 10%, transparent);
}
.status-label--preset {
  color: var(--ocg-info);
  background: color-mix(in srgb, var(--ocg-info) 10%, transparent);
}
.status-label--probe_confirmed {
  color: var(--ocg-success);
  background: var(--ocg-success-soft);
}
.status-label--probe_failure {
  color: var(--ocg-error);
  background: color-mix(in srgb, var(--ocg-error) 10%, transparent);
}
.override-badge {
  padding: 1px 6px;
  border-radius: 4px;
  color: var(--ocg-warning);
  background: var(--ocg-warning-soft);
  font-size: var(--ocg-font-xs);
  font-weight: 600;
}
</style>
