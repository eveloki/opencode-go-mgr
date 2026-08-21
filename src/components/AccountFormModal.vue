<template>
  <n-modal
    :show="show"
    preset="card"
    :title="title"
    class="account-modal"
    style="width: 600px; max-width: calc(100vw - 32px)"
    :mask-closable="false"
    @update:show="$emit('update:show', $event)"
  >
    <n-form
      ref="formRef"
      :model="form"
      :rules="rules"
      label-placement="top"
    >
      <n-alert v-if="formError" type="error" class="form-error" role="alert">
        {{ formError }}
      </n-alert>
      <n-alert
        v-if="isCustomPlan"
        type="warning"
        :show-icon="false"
        class="form-error"
      >
        {{ t("目标端点由管理员自行选择并负责：使用 http:// 时 Key 将明文传输；验证连接会发送一次最小真实请求，可能产生服务商费用。") }}
      </n-alert>
      <div class="modal-grid">
        <n-form-item v-if="!isEdit && offeringOptions.length > 1" path="offeringId" :label="t('服务套餐')">
          <n-select
            v-model:value="form.offeringId"
            :options="offeringOptions"
            :placeholder="t('选择服务套餐')"
          />
        </n-form-item>

        <n-form-item path="name" :label="t('名称')">
          <n-input
            :value="form.name"
            :input-props="{ 'aria-label': t('名称') }"
            :placeholder="t('例如：主号')"
            @update:value="handleNameUpdate"
          />
        </n-form-item>

        <n-form-item
          v-if="hasField('username')"
          path="username"
          :label="t('账号')"
        >
          <n-input
            :value="form.username"
            :input-props="{ 'aria-label': t('登录账号') }"
            :placeholder="t('OpenCode-Go 账号')"
            @update:value="form.username = $event"
          />
        </n-form-item>

        <n-form-item
          v-if="hasField('purchase_date')"
          path="purchaseDate"
          :label="t('购买日期')"
        >
          <n-date-picker
            v-model:value="form.purchaseDate"
            type="date"
            format="yyyy-MM-dd"
            :actions="['now']"
            :clearable="!purchaseDateRequired"
            :is-date-disabled="isPurchaseDateDisabled"
            :input-props="{ 'aria-label': t('购买日期') }"
          />
        </n-form-item>

        <n-form-item
          v-if="hasField('key')"
          path="key"
          :label="t('API Key')"
          class="key-field"
        >
          <n-input
            v-model:value="form.key"
            :input-props="{ 'aria-label': t('API Key') }"
            type="password"
            show-password-on="click"
            :placeholder="keyPlaceholder"
          />
          <p v-if="keyPrefixHint" class="field-hint">{{ keyPrefixHint }}</p>
        </n-form-item>

        <n-form-item
          v-if="hasField('base_url')"
          path="baseUrl"
          :label="t('Base URL')"
          class="full-width-field"
        >
          <n-input
            v-model:value="form.baseUrl"
            :input-props="{ 'aria-label': t('Base URL') }"
            :placeholder="t('https://api.example.com/v1')"
          />
        </n-form-item>

        <n-form-item
          v-if="hasField('upstream_protocol')"
          path="upstreamProtocol"
          :label="t('上游协议')"
        >
          <n-select
            v-model:value="form.upstreamProtocol"
            :options="upstreamProtocolOptions"
            :disabled="fieldImmutableAfterCreate('upstream_protocol')"
            :placeholder="t('协议')"
          />
          <p v-if="fieldImmutableAfterCreate('upstream_protocol')" class="field-hint">
            {{ t("创建后不可修改") }}
          </p>
        </n-form-item>

        <n-form-item
          v-if="hasField('auth_scheme')"
          path="authScheme"
          :label="t('鉴权方式')"
        >
          <n-select
            v-model:value="form.authScheme"
            :options="authSchemeOptions"
            :disabled="fieldImmutableAfterCreate('auth_scheme')"
            :placeholder="t('鉴权方式')"
          />
          <p v-if="fieldImmutableAfterCreate('auth_scheme')" class="field-hint">
            {{ t("创建后不可修改") }}
          </p>
        </n-form-item>

        <n-form-item
          v-if="hasField('acknowledgement')"
          path="acknowledgementAccepted"
          class="full-width-field"
        >
          <template v-if="riskNotice">
            <n-alert type="warning" :show-icon="false" class="risk-notice">
              <p>{{ riskNotice.body }}</p>
              <a
                :href="riskNotice.source_url"
                target="_blank"
                rel="noopener noreferrer"
              >{{ t("查看完整条款") }}</a>
            </n-alert>
            <n-checkbox v-model:checked="form.acknowledgementAccepted">
              {{ t("我已阅读并同意上述条款") }}
            </n-checkbox>
          </template>
        </n-form-item>

        <n-form-item
          v-if="hasField('model_capabilities')"
          path="modelCapabilities"
          :label="t('模型能力')"
          class="full-width-field"
        >
          <div class="capability-rows">
            <div
              v-for="(cap, index) in form.modelCapabilities"
              :key="index"
              class="capability-row"
            >
              <n-input
                v-model:value="cap.model_id"
                :input-props="{ 'aria-label': `${t('模型 ID')} ${index + 1}` }"
                :placeholder="t('模型 ID')"
              />
              <n-tag size="small" :bordered="false">{{ capabilityProtocol }}</n-tag>
              <n-button
                circle
                quaternary
                size="small"
                :aria-label="`${t('删除')} ${t('模型能力')} ${index + 1}`"
                @click="removeCapability(index)"
              >
                <template #icon><n-icon :component="MinusCircleOutlined" /></template>
              </n-button>
            </div>
            <n-button size="small" secondary @click="addCapability">
              <template #icon><n-icon :component="PlusOutlined" /></template>
              {{ t("添加模型") }}
            </n-button>
          </div>
        </n-form-item>

        <n-form-item
          v-if="hasField('notes')"
          path="notes"
          :label="t('备注')"
          class="full-width-field"
        >
          <n-input
            v-model:value="form.notes"
            type="textarea"
            :autosize="{ minRows: 4, maxRows: 10 }"
            :maxlength="4000"
            show-count
            :placeholder="t('可填写任意备注')"
            :input-props="{ 'aria-label': t('备注') }"
          />
        </n-form-item>
      </div>
    </n-form>
    <template #footer>
      <div class="modal-footer">
        <n-button
          v-if="isEdit && isCooling"
          text
          size="small"
          type="warning"
          @click="$emit('resetCooldown')"
        >
          {{ t("重置冷却") }}
        </n-button>
        <n-space>
          <n-button @click="$emit('update:show', false)">{{ t("取消") }}</n-button>
          <n-button type="primary" :loading="busy" @click="handleSave">{{ t("保存") }}</n-button>
        </n-space>
      </div>
    </template>
  </n-modal>
</template>

<script setup lang="ts">
import { computed, ref, toRef, watch } from "vue";
import type { FormInst, FormRules } from "naive-ui";
import {
  NAlert,
  NButton,
  NCheckbox,
  NDatePicker,
  NForm,
  NFormItem,
  NIcon,
  NInput,
  NModal,
  NSelect,
  NSpace,
  NTag,
} from "naive-ui";
import { MinusCircleOutlined, PlusOutlined } from "@vicons/antd";
import type { Account, AccountInput } from "../api/tauri";
import type { ProviderCatalogEntry, ProviderCatalogFormField } from "../api/providers.ts";
import { t } from "../i18n/index.ts";
import { useLocalizedModalCloseLabel } from "../utils/modal-close-label.ts";
import { localDateString } from "../views/account-lifecycle";
import { findCatalogEntry, findPlanDefinition, planFamilyLabel } from "../views/plans.ts";
import type { PlanDefinition } from "../views/plans.ts";
import {
  accountFormFieldIsImmutable,
  resolveAccountFormFields,
} from "../views/account-form-fields.ts";
import {
  accountCreatePayloadErrorKey,
  buildCreateAccountPayload,
  type AccountCreateCapability,
  type AccountCreateFormValues,
} from "../views/account-create-payload.ts";
import {
  CUSTOM_BASE_URL_ISSUE_KEYS,
  customBaseUrlIssue,
} from "../views/custom-account.ts";

export type AccountFormPayload = {
  name: string;
  username: string;
  key?: string;
  provider_id?: string;
  offering_id?: string;
  purchase_date?: string;
  notes: string;
  /** Custom API edit only; persisted via the dedicated custom-config route. */
  base_url?: string;
  /** Custom API edit only; persisted via the dedicated capabilities route. */
  model_capabilities?: AccountCreateCapability[];
};

type FormModel = {
  name: string;
  username: string;
  key: string;
  purchaseDate: number | null;
  notes: string;
  offeringId: string;
  baseUrl: string;
  upstreamProtocol: "chat_completions" | "responses" | "messages" | null;
  authScheme: "bearer" | "x-api-key" | null;
  acknowledgementAccepted: boolean;
  modelCapabilities: AccountCreateCapability[];
};

const props = withDefaults(defineProps<{
  show: boolean;
  account: Account | null;
  isCooling?: boolean;
  busy?: boolean;
  /** The selected plan family when creating an account. */
  plan: PlanDefinition | null;
  /** Provider catalog; when null, only the legacy OpenCode Go path is supported. */
  catalog: readonly ProviderCatalogEntry[] | null;
}>(), {
  account: null,
  isCooling: false,
  busy: false,
  plan: null,
  catalog: null,
});

const emit = defineEmits<{
  (e: "update:show", value: boolean): void;
  (e: "save", payload: AccountInput | AccountFormPayload): void;
  (e: "resetCooldown"): void;
}>();

useLocalizedModalCloseLabel(toRef(props, "show"), "account-modal");

const formRef = ref<FormInst | null>(null);
const form = ref<FormModel>(blankForm());
const nameWasEdited = ref(false);
const formError = ref("");

const isEdit = computed(() => !!props.account);
const title = computed(() => {
  if (isEdit.value) return t("编辑账号");
  const plan = effectivePlan.value;
  return plan
    ? t("添加 {plan} 账号", { plan: planFamilyLabel(plan, props.catalog) })
    : t("导入已有 Key");
});

const effectivePlan = computed<PlanDefinition | null>(() => {
  if (isEdit.value) {
    const account = props.account!;
    return findPlanDefinition(account.provider_id, account.offering_id) ?? null;
  }
  return props.plan;
});

const isCustomPlan = computed(() => effectivePlan.value?.id === "custom-endpoint");

const offeringOptions = computed(() => {
  const plan = effectivePlan.value;
  if (!plan) return [];
  return plan.offering_ids
    .map((offeringId) => findCatalogEntry(props.catalog, plan.provider_id, offeringId))
    .filter((entry): entry is ProviderCatalogEntry => !!entry)
    .map((entry) => ({ value: entry.offering_id, label: entry.display_name }));
});

const selectedOfferingId = computed(() => {
  if (form.value.offeringId) return form.value.offeringId;
  return offeringOptions.value[0]?.value ?? effectivePlan.value?.offering_ids[0] ?? "";
});

const catalogEntry = computed<ProviderCatalogEntry | undefined>(() => {
  const plan = effectivePlan.value;
  if (!plan) return undefined;
  return findCatalogEntry(props.catalog, plan.provider_id, selectedOfferingId.value);
});

const formFields = computed<ProviderCatalogFormField[]>(() => {
  return resolveAccountFormFields(effectivePlan.value, catalogEntry.value);
});

const fieldMap = computed(() => new Map(formFields.value.map((field) => [field.id, field])));

function hasField(id: string): boolean {
  return fieldMap.value.has(id);
}

function fieldRequired(id: string): boolean {
  return fieldMap.value.get(id)?.required ?? false;
}

function fieldImmutableAfterCreate(id: string): boolean {
  return accountFormFieldIsImmutable(fieldMap.value.get(id), isEdit.value);
}

const keyPrefixHint = computed(() => {
  const prefix = catalogEntry.value?.key_prefix;
  if (!prefix) return "";
  return t("Key 须以 {prefix} 开头", { prefix });
});

const keyPlaceholder = computed(() => {
  const prefix = catalogEntry.value?.key_prefix;
  if (prefix) return prefix + "...";
  return "sk-...";
});

const purchaseDateRequired = computed(() => fieldRequired("purchase_date"));

const upstreamProtocolOptions = computed(() => {
  const protocols = catalogEntry.value?.upstream_protocols ?? ["chat_completions", "responses", "messages"];
  return protocols.map((value) => ({ value, label: value }));
});

const capabilityProtocol = computed(() => form.value.upstreamProtocol ?? "—");

const authSchemeOptions = computed(() => {
  const schemes = catalogEntry.value?.auth_schemes ?? ["bearer"];
  return schemes.map((value) => ({ value, label: value }));
});

const riskNotice = computed(() => catalogEntry.value?.risk_notice ?? null);

const rules = computed<FormRules>(() => {
  const base: FormRules = {
    name: {
      required: true,
      whitespace: true,
      message: t("名称不能为空"),
      trigger: ["input", "blur"],
    },
  };

  if (fieldRequired("purchase_date")) {
    base.purchaseDate = [
      {
        required: true,
        type: "number",
        message: t("请选择购买日期"),
        trigger: ["change", "blur"],
      },
      {
        validator: (_rule: unknown, value: number | null) => {
          if (value === null) return true;
          return localDateString(value) <= localDateString();
        },
        message: t("购买日期不能晚于今天"),
        trigger: ["change", "blur"],
      },
    ];
  }

  if (hasField("key") && !isEdit.value) {
    base.key = {
      required: true,
      whitespace: true,
      message: t("请填写 API Key"),
      trigger: ["input", "blur"],
    };
  }

  if (hasField("base_url") && fieldRequired("base_url")) {
    base.baseUrl = {
      required: true,
      validator: (_rule: unknown, value: string) => {
        const issue = customBaseUrlIssue(value ?? "");
        return issue ? new Error(t(CUSTOM_BASE_URL_ISSUE_KEYS[issue])) : true;
      },
      trigger: ["input", "blur"],
    };
  }

  if (hasField("upstream_protocol") && fieldRequired("upstream_protocol")) {
    base.upstreamProtocol = {
      required: true,
      type: "string",
      message: t("协议"),
      trigger: ["change", "blur"],
    };
  }

  if (hasField("auth_scheme") && fieldRequired("auth_scheme")) {
    base.authScheme = {
      required: true,
      type: "string",
      message: t("鉴权方式"),
      trigger: ["change", "blur"],
    };
  }

  if (hasField("acknowledgement") && riskNotice.value) {
    base.acknowledgementAccepted = {
      required: true,
      type: "boolean",
      validator: (_rule: unknown, value: boolean) => value === true,
      message: t("请阅读并同意条款"),
      trigger: ["change"],
    };
  }

  if (hasField("model_capabilities") && fieldRequired("model_capabilities")) {
    base.modelCapabilities = {
      required: true,
      type: "array",
      validator: (_rule: unknown, value: AccountCreateCapability[]) =>
        Array.isArray(value) && value.length > 0 && value.every((cap) => cap.model_id.trim() && cap.protocol),
      message: t("请至少添加一个模型能力"),
      trigger: ["change"],
    };
  }

  return base;
});

watch(() => props.show, (show) => {
  if (show) {
    form.value = props.account ? formFromAccount(props.account) : blankForm();
    nameWasEdited.value = isEdit.value;
    formRef.value?.restoreValidation();
    formError.value = "";
  }
});

watch(() => props.plan, (plan) => {
  if (!isEdit.value && plan && !form.value.offeringId) {
    form.value.offeringId = plan.offering_ids[0] ?? "";
  }
}, { immediate: true });

function timestampFromLocalDate(value: string): number | null {
  const parts = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (!parts) return null;
  const year = Number(parts[1]);
  const month = Number(parts[2]);
  const day = Number(parts[3]);
  const date = new Date(year, month - 1, day);
  return date.getFullYear() === year && date.getMonth() === month - 1 && date.getDate() === day
    ? date.getTime()
    : null;
}

function blankForm(): FormModel {
  const plan = props.plan;
  return {
    name: "",
    username: "",
    key: "",
    purchaseDate: timestampFromLocalDate(localDateString()) ?? Date.now(),
    notes: "",
    offeringId: plan?.offering_ids[0] ?? "",
    baseUrl: "",
    upstreamProtocol: null,
    authScheme: null,
    acknowledgementAccepted: false,
    modelCapabilities: [],
  };
}

function formFromAccount(account: Account): FormModel {
  return {
    name: account.name,
    username: account.username,
    key: "",
    purchaseDate: timestampFromLocalDate(account.purchase_date)
      ?? timestampFromLocalDate(localDateString())
      ?? Date.now(),
    notes: account.notes ?? "",
    offeringId: account.offering_id,
    baseUrl: account.custom_config?.base_url ?? "",
    upstreamProtocol: account.custom_config?.upstream_protocol ?? null,
    authScheme: account.custom_config?.auth_scheme ?? null,
    acknowledgementAccepted: account.acknowledgements.length > 0,
    modelCapabilities: account.model_capabilities.map((cap) => ({
      model_id: cap.model_id,
      protocol: account.custom_config?.upstream_protocol ?? cap.protocol,
    })),
  };
}

function handleNameUpdate(value: string) {
  form.value.name = value;
  if (!isEdit.value && !nameWasEdited.value) {
    form.value.name = value;
  }
}

function isPurchaseDateDisabled(timestamp: number): boolean {
  return localDateString(timestamp) > localDateString();
}

function addCapability() {
  const protocol = form.value.upstreamProtocol;
  if (!protocol) return;
  form.value.modelCapabilities.push({
    model_id: "",
    protocol,
  });
}

watch(
  () => form.value.upstreamProtocol,
  (protocol) => {
    if (!protocol) return;
    for (const capability of form.value.modelCapabilities) capability.protocol = protocol;
  },
);

function removeCapability(index: number) {
  form.value.modelCapabilities.splice(index, 1);
}

async function handleSave() {
  try {
    await formRef.value?.validate();
  } catch {
    return;
  }

  if (isEdit.value) {
    const payload: AccountFormPayload = {
      name: form.value.name.trim(),
      username: form.value.username.trim(),
      notes: form.value.notes,
    };
    if (!isCustomPlan.value) {
      // Custom forms have no purchase-date field; sending today's date would
      // silently reset the account's monthly window.
      payload.purchase_date = form.value.purchaseDate === null ? undefined : localDateString(form.value.purchaseDate);
    }
    if (form.value.key.trim()) {
      payload.key = form.value.key.trim();
    }
    if (isCustomPlan.value) {
      payload.base_url = form.value.baseUrl.trim();
      payload.model_capabilities = form.value.modelCapabilities.map((cap) => ({
        model_id: cap.model_id.trim(),
        protocol: cap.protocol,
      }));
    }
    emit("save", payload);
    return;
  }

  const plan = effectivePlan.value;
  if (!plan) {
    formError.value = t("无法确定账号方案，请关闭后重试");
    return;
  }

  const values: AccountCreateFormValues = {
    name: form.value.name,
    username: form.value.username,
    key: form.value.key,
    purchase_date: form.value.purchaseDate === null ? undefined : localDateString(form.value.purchaseDate),
    notes: form.value.notes,
    base_url: form.value.baseUrl,
    upstream_protocol: form.value.upstreamProtocol ?? undefined,
    auth_scheme: form.value.authScheme ?? undefined,
    acknowledgement_accepted: form.value.acknowledgementAccepted,
    model_capabilities: form.value.modelCapabilities.length > 0 ? form.value.modelCapabilities : undefined,
  };

  try {
    const payload = buildCreateAccountPayload(plan, form.value.offeringId, values, catalogEntry.value);
    emit("save", payload);
  } catch (error) {
    // Never submit a degraded payload: the backend rejects incomplete Custom
    // and acknowledgement-gated plans, so keep the draft editable instead.
    formError.value = t(accountCreatePayloadErrorKey(error));
  }
}
</script>

<style scoped>
.modal-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 12px;
  align-items: start;
}

.form-error {
  margin-bottom: 12px;
}

.full-width-field,
.key-field,
.notes-field {
  grid-column: 1 / -1;
}

.field-hint {
  margin: 6px 0 0;
  color: var(--ocg-muted);
  font-size: var(--ocg-font-xs);
}

.risk-notice {
  margin-bottom: 10px;
}

.risk-notice p {
  margin: 0 0 6px;
}

.capability-rows {
  display: grid;
  gap: 8px;
}

.capability-row {
  display: grid;
  grid-template-columns: 1fr 140px auto;
  gap: 8px;
  align-items: center;
}

.modal-grid :deep(.n-date-picker) {
  width: 100%;
}

.modal-footer {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 12px;
}

@media (max-width: 640px) {
  .modal-grid {
    grid-template-columns: 1fr;
  }

  .capability-row {
    grid-template-columns: 1fr;
  }
}
</style>
