<template>
  <n-modal
    :show="show"
    preset="card"
    :title="t('注册新账号（Beta）：{name}', { name: account.name })"
    class="managed-wizard-modal"
    style="width: 820px; max-width: calc(100vw - 32px)"
    :mask-closable="false"
    @update:show="$emit('update:show', $event)"
  >
    <n-steps :current="currentStep" size="small" class="managed-wizard__steps">
      <n-step :title="t('Google 账号')" />
      <n-step :title="t('邀请注册')" />
      <n-step :title="t('完成支付')" />
      <n-step :title="t('验证 Key')" />
    </n-steps>

    <n-alert type="warning" class="managed-wizard__alert">
      {{ t("托管注册与独立浏览器 Profile 为 Beta 功能，尚未经过充分测试，请勿依赖其用于生产环境。") }}
    </n-alert>

    <n-alert v-if="browserCapabilities.mode === 'unsupported'" type="error" class="managed-wizard__alert">
      {{ browserCapabilities.reason || t("当前环境不支持独立浏览器") }}
    </n-alert>

    <section class="managed-wizard__quick-links" aria-labelledby="managed-quick-links-title">
      <div>
        <h3 id="managed-quick-links-title">{{ t("重新打开已完成页面") }}</h3>
        <p>{{ t("这些按钮始终使用该账号自己的浏览器 Profile，不会改变已保存的注册进度。") }}</p>
      </div>
      <n-space wrap>
        <n-button
          size="small"
          secondary
          :disabled="!browserAvailable"
          :loading="openingTarget === 'google_signup'"
          @click="$emit('openBrowser', 'google_signup')"
        >{{ t("打开 Google") }}</n-button>
        <n-button
          v-if="currentStep >= 2"
          size="small"
          secondary
          :disabled="!browserAvailable"
          :loading="openingTarget === 'invite'"
          @click="$emit('openBrowser', 'invite')"
        >{{ t("打开邀请链接") }}</n-button>
        <n-button
          v-if="currentStep >= 3"
          size="small"
          secondary
          :disabled="!browserAvailable"
          :loading="openingTarget === 'console'"
          @click="$emit('openBrowser', 'console')"
        >{{ t("打开 OpenCode 官网") }}</n-button>
      </n-space>
    </section>

    <section class="managed-wizard__stage">
      <template v-if="account.setup_step === 'google_account'">
        <p class="managed-wizard__kicker">{{ t("第 1 步，共 4 步") }}</p>
        <h2>{{ t("创建或登录 Google 账号") }}</h2>
        <p>{{ t("打开一个没有该账号登录状态的独立浏览器 Profile，在 Google 页面中由你手动注册或登录。") }}</p>
        <n-alert type="info" :show-icon="false">
          {{ t("OCG Manager 不保存 Google 密码，不处理验证码，也不会自动填写注册信息。") }}
        </n-alert>
        <div class="managed-wizard__actions">
          <n-button
            secondary
            :disabled="!browserAvailable"
            :loading="openingTarget === 'google_signup'"
            @click="$emit('openBrowser', 'google_signup')"
          >{{ t("打开 Google 注册页面") }}</n-button>
          <n-button type="primary" :loading="busy" @click="$emit('advance', 'opencode_registration')">
            {{ t("我已完成 Google 注册或登录") }}
          </n-button>
        </div>
      </template>

      <template v-else-if="account.setup_step === 'opencode_registration'">
        <p class="managed-wizard__kicker">{{ t("第 2 步，共 4 步") }}</p>
        <h2>{{ t("通过邀请链接注册 OpenCode Go") }}</h2>
        <p>{{ t("使用同一个浏览器 Profile 打开设置中的邀请链接，并使用刚才的 Google 账号完成 OpenCode 登录或注册。") }}</p>
        <div class="managed-wizard__actions">
          <n-button
            secondary
            :disabled="!browserAvailable"
            :loading="openingTarget === 'invite'"
            @click="$emit('openBrowser', 'invite')"
          >{{ t("打开邀请链接") }}</n-button>
          <n-button type="primary" :loading="busy" @click="$emit('advance', 'payment')">
            {{ t("我已完成 OpenCode 注册") }}
          </n-button>
        </div>
      </template>

      <template v-else-if="account.setup_step === 'payment'">
        <p class="managed-wizard__kicker">{{ t("第 3 步，共 4 步") }}</p>
        <h2>{{ t("在 OpenCode 中完成支付") }}</h2>
        <p>{{ t("在浏览器中检查套餐、金额和支付信息。真实支付只会由你在 OpenCode 页面中明确执行。") }}</p>
        <n-alert type="warning" :show-icon="false">
          {{ t("OCG Manager 不读取支付页面，也不会自动点击支付按钮。") }}
        </n-alert>
        <div class="managed-wizard__actions">
          <n-button
            secondary
            :disabled="!browserAvailable"
            :loading="openingTarget === 'console'"
            @click="$emit('openBrowser', 'console')"
          >{{ t("返回 OpenCode 页面") }}</n-button>
          <n-button type="primary" :loading="busy" @click="$emit('advance', 'key_verification')">
            {{ t("我已完成支付") }}
          </n-button>
        </div>
      </template>

      <template v-else-if="account.setup_step === 'key_verification'">
        <p class="managed-wizard__kicker">{{ t("第 4 步，共 4 步") }}</p>
        <h2>{{ t("复制并验证 Key") }}</h2>
        <p>{{ t("在 OpenCode 官网复制 Key，填入下方后由 OCG Manager 真实请求上游验证。只有验证成功才会启用账号。") }}</p>
        <n-input
          v-model:value="keyDraft"
          type="password"
          show-password-on="click"
          class="managed-wizard__key"
          placeholder="sk-..."
          :input-props="{ 'aria-label': t('API Key') }"
          @keydown.enter.prevent="verifyKey"
        />
        <div class="managed-wizard__actions">
          <n-button
            secondary
            :disabled="!browserAvailable"
            :loading="openingTarget === 'console'"
            @click="$emit('openBrowser', 'console')"
          >{{ t("打开 OpenCode 官网") }}</n-button>
          <n-button type="primary" :disabled="!keyDraft.trim()" :loading="busy" @click="verifyKey">
            {{ t("保存并实测 Key") }}
          </n-button>
        </div>
      </template>
    </section>

    <template #footer>
      <div class="managed-wizard__footer">
        <span>{{ t("关闭后可随时从账号列表继续，浏览器登录状态会保留。") }}</span>
        <n-button @click="$emit('update:show', false)">{{ t("暂时关闭") }}</n-button>
      </div>
    </template>
  </n-modal>
</template>

<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { NAlert, NButton, NInput, NModal, NSpace, NStep, NSteps } from "naive-ui";
import type {
  Account,
  AccountSetupStep,
  BrowserCapabilities,
  BrowserTarget,
} from "../api/tauri";
import { t } from "../i18n/index.ts";
import { setupStepIndex } from "../views/managed-account";

const props = defineProps<{
  show: boolean;
  account: Account;
  browserCapabilities: BrowserCapabilities;
  openingTarget: BrowserTarget | null;
  busy: boolean;
}>();

const emit = defineEmits<{
  (event: "update:show", value: boolean): void;
  (event: "openBrowser", target: BrowserTarget): void;
  (event: "advance", setupStep: AccountSetupStep): void;
  (event: "verifyKey", key: string): void;
}>();

const keyDraft = ref("");
const currentStep = computed(() => Math.min(4, setupStepIndex(props.account.setup_step) + 1));
const browserAvailable = computed(() => props.browserCapabilities.mode !== "unsupported");

watch(() => [props.show, props.account.id, props.account.setup_step] as const, () => {
  keyDraft.value = "";
});

function verifyKey(): void {
  const key = keyDraft.value.trim();
  if (key) emit("verifyKey", key);
}
</script>

<style scoped>
.managed-wizard__steps {
  margin-bottom: 20px;
}

.managed-wizard__alert {
  margin-bottom: 14px;
}

.managed-wizard__quick-links {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 14px 16px;
  border: 1px solid var(--ocg-divider);
  border-radius: 12px;
  background: var(--ocg-canvas);
}

.managed-wizard__quick-links h3,
.managed-wizard__stage h2 {
  margin: 0;
}

.managed-wizard__quick-links h3 {
  font-size: var(--ocg-font-md);
}

.managed-wizard__quick-links p,
.managed-wizard__stage p,
.managed-wizard__footer {
  color: var(--ocg-muted);
  font-size: var(--ocg-font-sm);
  line-height: 1.6;
}

.managed-wizard__quick-links p {
  margin: 4px 0 0;
}

.managed-wizard__stage {
  display: grid;
  gap: 16px;
  min-height: 260px;
  margin-top: 16px;
  padding: 22px;
  border: 1px solid var(--ocg-divider);
  border-radius: 14px;
}

.managed-wizard__stage p {
  margin: 0;
}

.managed-wizard__kicker {
  color: var(--ocg-primary) !important;
  font-weight: 700;
}

.managed-wizard__key {
  max-width: 560px;
}

.managed-wizard__actions,
.managed-wizard__footer {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

@media (max-width: 640px) {
  .managed-wizard__quick-links,
  .managed-wizard__footer {
    align-items: stretch;
    flex-direction: column;
  }
}
</style>
