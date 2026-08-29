<template>
  <n-modal
    v-model:show="visible"
    preset="card"
    :title="mode === 'export' ? t('导出账号迁移包') : t('导入账号迁移包')"
    class="account-transfer-modal"
    style="width: 680px; max-width: calc(100vw - 32px)"
    :mask-closable="!operationLocked"
    :close-on-esc="!operationLocked"
  >
    <template v-if="mode === 'export'">
      <n-alert type="info" :show-icon="false" class="transfer-note">
        {{ t('迁移包由服务端加密，浏览器不会读取或显示明文 Key。需要管理员账号进行一次授权，并使用独立的迁移包密码。') }}
      </n-alert>
      <n-alert v-if="!sessionInitialized" type="warning" :show-icon="false" class="transfer-note">
        {{ t('尚未初始化管理员账号。此处创建的管理员账号用于保护本机控制台和确认账号迁移，请妥善保存。') }}
      </n-alert>
      <n-form label-placement="top" @submit.prevent="exportBundle">
        <n-form-item :label="t('管理员用户名')" required>
          <n-input v-model:value="adminUsername" autocomplete="username" :disabled="operationLocked" />
        </n-form-item>
        <n-form-item :label="t('管理员密码')" required>
          <n-input v-model:value="adminPassword" type="password" show-password-on="click" :autocomplete="sessionInitialized ? 'current-password' : 'new-password'" :disabled="operationLocked" />
        </n-form-item>
        <n-form-item v-if="!sessionInitialized" :label="t('确认管理员密码')" required>
          <n-input v-model:value="adminPasswordConfirmation" type="password" show-password-on="click" autocomplete="new-password" :disabled="operationLocked" />
        </n-form-item>
        <n-form-item :label="t('迁移包密码')" :feedback="t('至少 12 个字符；此密码只用于加密迁移文件，不是管理员密码。密码丢失后无法找回。')" required>
          <n-input v-model:value="bundlePassword" type="password" show-password-on="click" autocomplete="new-password" :disabled="operationLocked" />
        </n-form-item>
        <n-form-item :label="t('确认迁移包密码')" required>
          <n-input v-model:value="bundlePasswordConfirmation" type="password" show-password-on="click" autocomplete="new-password" :disabled="operationLocked" />
        </n-form-item>
      </n-form>
      <p class="transfer-lifecycle">{{ t('不迁移 Zen Free、浏览器资料与 Cookie、登录密码、邀请码、用量和冷却状态。Custom API 导入后保持禁用和待验证，请在启用前核对 Endpoint 与 Key，验证仍为可选；未完成的托管账号会重置注册流程，已完成托管账号的 Key 可迁移但浏览器登录不会迁移。') }}</p>
      <n-alert v-if="errorText" type="error" :title="errorText" class="transfer-note" />
      <n-alert v-if="resultText" type="success" :title="resultText" class="transfer-note" />
      <div class="transfer-actions">
        <n-button :disabled="operationLocked" @click="visible = false">{{ t('取消') }}</n-button>
        <n-button type="primary" :loading="busy" :disabled="!canExport || operationLocked" @click="exportBundle">
          {{ t('下载加密迁移包') }}
        </n-button>
      </div>
    </template>

    <template v-else>
      <n-alert type="info" :show-icon="false" class="transfer-note">
        {{ t('选择由 OCG Manager 导出的加密 .ocgbackup 文件。文件和密码仅在此窗口内存中使用。') }}
      </n-alert>
      <input
        ref="fileInput"
        class="sr-only"
        type="file"
        accept=".ocgbackup,application/json"
        :disabled="operationLocked"
        aria-describedby="account-transfer-file-help"
        @change="readBundleFile"
      />
      <n-form label-placement="top" @submit.prevent="previewBundle">
        <n-form-item :label="t('迁移包文件')" required>
          <n-space align="center" wrap>
            <n-button :disabled="operationLocked" @click="fileInput?.click()">{{ t('选择文件') }}</n-button>
            <span id="account-transfer-file-help" class="transfer-file-name">{{ fileName || t('尚未选择文件') }}</span>
          </n-space>
        </n-form-item>
        <n-form-item :label="t('迁移包密码')" required>
          <n-input v-model:value="bundlePassword" type="password" show-password-on="click" autocomplete="current-password" :disabled="operationLocked" @update:value="clearPreview" />
        </n-form-item>
      </n-form>
      <p class="transfer-lifecycle">{{ t('不迁移 Zen Free、浏览器资料与 Cookie、登录密码、邀请码、用量和冷却状态。Custom API 导入后保持禁用和待验证，请在启用前核对 Endpoint 与 Key，验证仍为可选；未完成的托管账号会重置注册流程，已完成托管账号的 Key 可迁移但浏览器登录不会迁移。') }}</p>

      <n-alert v-if="errorText" type="error" :title="errorText" class="transfer-note" />
      <n-alert v-if="resultText" type="success" :title="resultText" class="transfer-note" />

      <template v-if="preview">
        <n-alert type="info" :show-icon="false" class="transfer-note">
          {{ t('预览：可导入 {importable} 项，重复 {duplicate} 项。重复项不会覆盖现有账号。', { importable: preview.importableAccounts, duplicate: preview.duplicateAccounts }) }}
        </n-alert>
        <div class="transfer-preview" role="region" :aria-label="t('迁移预览')">
          <div v-for="item in preview.items" :key="item.index" class="transfer-preview-row">
            <span class="transfer-disposition">{{ dispositionLabel(item.disposition) }}</span>
            <span>{{ item.name }}</span>
            <span class="mono">{{ item.providerId }}/{{ item.offeringId }}</span>
            <span v-if="item.reason" class="transfer-reason">{{ dispositionReason(item.disposition) }}</span>
          </div>
        </div>
        <n-checkbox v-model:checked="importConfirmed" :disabled="operationLocked" class="transfer-confirmation">
          {{ t('我确认导入将写入当前账号列表；重复账号不会覆盖。') }}
        </n-checkbox>
      </template>

      <span class="sr-only" aria-live="polite" aria-atomic="true">{{ liveSummary }}</span>
      <div class="transfer-actions">
        <n-button :disabled="operationLocked" @click="visible = false">{{ t('取消') }}</n-button>
        <n-button :loading="previewing" :disabled="!canPreview || operationLocked" @click="previewBundle">
          {{ t('预览导入') }}
        </n-button>
        <n-button type="primary" :loading="busy" :disabled="!canImport" @click="importBundle">
          {{ t('确认导入') }}
        </n-button>
      </div>
    </template>
  </n-modal>
</template>

<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { NAlert, NButton, NCheckbox, NForm, NFormItem, NInput, NModal, NSpace } from "naive-ui";
import { dashboardApi } from "../api/dashboard.ts";
import type { AccountImportDisposition, AccountImportPreview } from "../api/generated/dashboard-v3.ts";
import { useSessionStore } from "../stores/session.ts";
import { t } from "../i18n/index.ts";
import { dashboardErrorDetail } from "../utils/errors.ts";

const MAX_BUNDLE_BYTES = 4 * 1024 * 1024;

const props = defineProps<{
  show: boolean;
  mode: "import" | "export";
}>();
const emit = defineEmits<{
  "update:show": [value: boolean];
  imported: [count: number];
}>();

const session = useSessionStore();
const fileInput = ref<HTMLInputElement | null>(null);
const adminUsername = ref("");
const adminPassword = ref("");
const adminPasswordConfirmation = ref("");
const bundlePassword = ref("");
const bundlePasswordConfirmation = ref("");
const bundle = ref("");
const fileName = ref("");
const preview = ref<AccountImportPreview | null>(null);
const previewBundleSnapshot = ref("");
const previewPasswordSnapshot = ref("");
const importConfirmed = ref(false);
const busy = ref(false);
const previewing = ref(false);
let previewEpoch = 0;
const errorText = ref("");
const resultText = ref("");

const visible = computed({
  get: () => props.show,
  set: (value: boolean) => emit("update:show", value),
});
const sessionInitialized = computed(() => session.status?.initialized ?? true);
const operationLocked = computed(() => busy.value || previewing.value);
const canExport = computed(() => (
  Boolean(adminUsername.value.trim())
  && Boolean(adminPassword.value)
  && bundlePassword.value.length >= 12
  && bundlePassword.value === bundlePasswordConfirmation.value
  && (sessionInitialized.value || adminPassword.value === adminPasswordConfirmation.value)
));
const canPreview = computed(() => Boolean(bundle.value) && bundlePassword.value.length >= 12);
const canImport = computed(() => (
  Boolean(preview.value)
  && (preview.value?.importableAccounts ?? 0) > 0
  && Boolean(bundle.value)
  && bundlePassword.value.length >= 12
  && bundle.value === previewBundleSnapshot.value
  && bundlePassword.value === previewPasswordSnapshot.value
  && importConfirmed.value
  && !busy.value
));
const liveSummary = computed(() => resultText.value || errorText.value || (preview.value
  ? t('预览：可导入 {importable} 项，重复 {duplicate} 项。', {
    importable: preview.value.importableAccounts,
    duplicate: preview.value.duplicateAccounts,
  })
  : ""));

watch(() => props.show, async (show) => {
  if (show && props.mode === "export" && !session.status) {
    try {
      await session.loadStatus();
    } catch (error) {
      errorText.value = dashboardErrorDetail(error);
    }
  }
  if (!show) clearTransient();
});
watch(() => props.mode, clearTransient);

function clearPreview(): void {
  previewEpoch += 1;
  preview.value = null;
  previewBundleSnapshot.value = "";
  previewPasswordSnapshot.value = "";
  importConfirmed.value = false;
  resultText.value = "";
  errorText.value = "";
}

function clearTransient(): void {
  clearPreview();
  adminUsername.value = "";
  adminPassword.value = "";
  adminPasswordConfirmation.value = "";
  bundlePassword.value = "";
  bundlePasswordConfirmation.value = "";
  bundle.value = "";
  fileName.value = "";
  busy.value = false;
  previewing.value = false;
  errorText.value = "";
  resultText.value = "";
  if (fileInput.value) fileInput.value.value = "";
}

function dispositionLabel(disposition: AccountImportDisposition): string {
  return disposition === "import" || disposition === "imported" ? t('将导入') : t('重复，跳过');
}

function dispositionReason(disposition: AccountImportDisposition): string {
  return disposition === "duplicate" ? t('同一 Plan 和名称的账号已存在') : "";
}

async function readBundleFile(event: Event): Promise<void> {
  if (operationLocked.value) return;
  const input = event.target as HTMLInputElement;
  const file = input.files?.[0];
  clearPreview();
  const epoch = previewEpoch;
  bundle.value = "";
  fileName.value = "";
  if (!file) return;
  if (file.size > MAX_BUNDLE_BYTES) {
    errorText.value = t('迁移包文件不能超过 4 MiB');
    input.value = "";
    return;
  }
  if (!/\.(ocgbackup|json)$/iu.test(file.name)) {
    errorText.value = t('请选择 .ocgbackup 或 JSON 迁移包文件');
    input.value = "";
    return;
  }
  try {
    const nextBundle = await file.text();
    if (epoch !== previewEpoch) return;
    bundle.value = nextBundle;
    fileName.value = file.name;
  } catch (error) {
    if (epoch === previewEpoch) errorText.value = dashboardErrorDetail(error);
    input.value = "";
  }
}

function downloadBundle(bundleText: string, filename: string): void {
  const url = URL.createObjectURL(new Blob([bundleText], { type: "application/octet-stream" }));
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  URL.revokeObjectURL(url);
}

async function exportBundle(): Promise<void> {
  if (!canExport.value || busy.value) return;
  busy.value = true;
  errorText.value = "";
  try {
    const authStatus = session.status ?? await session.loadStatus();
    if (!authStatus.initialized) await session.register(adminUsername.value.trim(), adminPassword.value);
    const exported = await dashboardApi.exportAccountTransfer({
      adminUsername: adminUsername.value.trim(),
      adminPassword: adminPassword.value,
      bundlePassword: bundlePassword.value,
    });
    downloadBundle(exported.bundle, exported.filename);
    resultText.value = t('已下载加密迁移包：导出 {exported} 项，跳过 {skipped} 项。', {
      exported: exported.exportedAccounts,
      skipped: exported.skippedAccounts,
    });
    adminPassword.value = "";
    adminPasswordConfirmation.value = "";
    bundlePassword.value = "";
    bundlePasswordConfirmation.value = "";
  } catch (error) {
    errorText.value = dashboardErrorDetail(error);
  } finally {
    busy.value = false;
  }
}

async function previewBundle(): Promise<void> {
  if (!canPreview.value || previewing.value || busy.value) return;
  previewing.value = true;
  clearPreview();
  const epoch = previewEpoch;
  const requestBundle = bundle.value;
  const requestPassword = bundlePassword.value;
  try {
    const nextPreview = await dashboardApi.previewAccountTransfer({
      bundle: requestBundle,
      password: requestPassword,
    });
    if (
      epoch !== previewEpoch
      || bundle.value !== requestBundle
      || bundlePassword.value !== requestPassword
    ) return;
    preview.value = nextPreview;
    previewBundleSnapshot.value = requestBundle;
    previewPasswordSnapshot.value = requestPassword;
  } catch (error) {
    if (epoch === previewEpoch) errorText.value = dashboardErrorDetail(error);
  } finally {
    if (epoch === previewEpoch) previewing.value = false;
  }
}

async function importBundle(): Promise<void> {
  if (!canImport.value || busy.value) return;
  busy.value = true;
  errorText.value = "";
  try {
    const result = await dashboardApi.importAccountTransfer({
      bundle: bundle.value,
      password: bundlePassword.value,
    });
    resultText.value = t('已导入 {count} 项账号配置。', { count: result.importedAccounts });
    bundlePassword.value = "";
    previewEpoch += 1;
    preview.value = null;
    previewBundleSnapshot.value = "";
    previewPasswordSnapshot.value = "";
    importConfirmed.value = false;
    bundle.value = "";
    fileName.value = "";
    if (fileInput.value) fileInput.value.value = "";
    emit("imported", result.importedAccounts);
  } catch (error) {
    errorText.value = dashboardErrorDetail(error);
  } finally {
    busy.value = false;
  }
}
</script>

<style scoped>
.transfer-note { margin-bottom: 12px; }
.transfer-lifecycle { color: var(--ocg-subtle); font-size: var(--ocg-font-sm); line-height: 1.55; }
.transfer-actions { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 8px; margin-top: 16px; }
.transfer-file-name { min-width: 0; overflow-wrap: anywhere; color: var(--ocg-subtle); }
.transfer-preview { max-height: 260px; overflow: auto; border: 1px solid var(--ocg-border); border-radius: 6px; margin: 12px 0; }
.transfer-preview-row { display: grid; grid-template-columns: minmax(84px, auto) minmax(0, 1fr) minmax(0, 1.2fr); gap: 8px; padding: 8px 12px; border-bottom: 1px solid var(--ocg-border); font-size: var(--ocg-font-sm); }
.transfer-preview-row:last-child { border-bottom: 0; }
.transfer-disposition { font-weight: 600; }
.transfer-reason { grid-column: 2 / -1; color: var(--ocg-subtle); }
.transfer-confirmation { display: flex; align-items: flex-start; margin-top: 12px; }
@media (max-width: 560px) { .transfer-preview-row { grid-template-columns: minmax(0, 1fr); gap: 4px; } .transfer-reason { grid-column: auto; } .transfer-actions > * { flex: 1 1 auto; } }
</style>
