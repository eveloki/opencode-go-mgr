import assert from "node:assert/strict";
import test from "node:test";
import { userFacingError } from "./utils/errors.ts";
import {
  DEFAULT_LOCALE,
  LOCALE_OPTIONS,
  LOCALE_STORAGE_KEY,
  locale,
  matchLocale,
  readLocale,
  resolveLocale,
  setLocale,
  t,
  writeLocale,
} from "./i18n/index.ts";
import type { MessageKey } from "./i18n/index.ts";
import { deDEMessages } from "./i18n/messages/de-DE.ts";
import { enUSMessages } from "./i18n/messages/en-US.ts";
import { esESMessages } from "./i18n/messages/es-ES.ts";
import { frFRMessages } from "./i18n/messages/fr-FR.ts";
import { jaJPMessages } from "./i18n/messages/ja-JP.ts";
import { koKRMessages } from "./i18n/messages/ko-KR.ts";
import { ptBRMessages } from "./i18n/messages/pt-BR.ts";
import { ruRUMessages } from "./i18n/messages/ru-RU.ts";
import { zhTWMessages } from "./i18n/messages/zh-TW.ts";
import { managedAccountEnUSMessages } from "./i18n/messages/managed-account.ts";
import { formatCost } from "./utils/format.ts";

test("locale matching uses stored preference, browser languages, and a stable fallback", () => {
  assert.equal(matchLocale("zh-Hant-HK"), "zh-TW");
  assert.equal(matchLocale("pt_PT"), "pt-BR");
  assert.equal(matchLocale("es-MX"), "es-ES");
  assert.equal(resolveLocale("ru_RU", ["en-US"]), "ru-RU");
  assert.equal(resolveLocale("unknown", ["fr-CA", "en-US"]), "fr-FR");
  assert.equal(resolveLocale(null, ["unknown"]), DEFAULT_LOCALE);
});

test("locale preference can be read and written without requiring browser storage", () => {
  const values = new Map<string, string>();
  const storage = {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => { values.set(key, value); },
  };

  writeLocale(storage, "ja-JP");
  assert.equal(values.get(LOCALE_STORAGE_KEY), "ja-JP");
  assert.equal(readLocale(storage, ["en-US"]), "ja-JP");
  assert.equal(readLocale({ getItem: () => { throw new Error("blocked"); } }, ["ko-KR"]), "ko-KR");
});

test("all locale catalogs have identical keys and placeholders", () => {
  const expectedKeys = (Object.keys(enUSMessages) as MessageKey[]).sort();
  const placeholders = (value: string) => [...value.matchAll(/\{\w+\}/g)].map(([token]) => token).sort();
  const rawCatalogs = {
    "zh-TW": zhTWMessages,
    "en-US": enUSMessages,
    "ja-JP": jaJPMessages,
    "ko-KR": koKRMessages,
    "es-ES": esESMessages,
    "fr-FR": frFRMessages,
    "de-DE": deDEMessages,
    "pt-BR": ptBRMessages,
    "ru-RU": ruRUMessages,
  } as const;

  for (const [value, catalog] of Object.entries(rawCatalogs)) {
    assert.deepEqual(Object.keys(catalog).sort(), expectedKeys, value);
    for (const key of expectedKeys) {
      assert.deepEqual(placeholders(catalog[key]), placeholders(key), `${value}: ${key}`);
    }
  }
});

test("managed account copy is localized instead of inheriting English fallbacks", () => {
  const localizedCatalogs = {
    "ja-JP": jaJPMessages,
    "ko-KR": koKRMessages,
    "es-ES": esESMessages,
    "fr-FR": frFRMessages,
    "de-DE": deDEMessages,
    "pt-BR": ptBRMessages,
    "ru-RU": ruRUMessages,
  } as const;

  for (const [localeName, catalog] of Object.entries(localizedCatalogs)) {
    for (const [key, english] of Object.entries(managedAccountEnUSMessages)) {
      assert.notEqual(catalog[key as MessageKey], english, `${localeName}: ${key}`);
    }
  }
});

test("v2 plan, form, draft, pricing, and Alias copy has no English fallback in any non-English locale", () => {
  const localizedCatalogs = {
    "zh-TW": zhTWMessages,
    "ja-JP": jaJPMessages,
    "ko-KR": koKRMessages,
    "es-ES": esESMessages,
    "fr-FR": frFRMessages,
    "de-DE": deDEMessages,
    "pt-BR": ptBRMessages,
    "ru-RU": ruRUMessages,
  } as const;
  const keys = [
    "服务商目录加载失败",
    "服务套餐",
    "选择服务套餐",
    "例如：主号",
    "Base URL",
    "上游协议",
    "选择上游协议",
    "鉴权方式",
    "选择鉴权方式",
    "查看完整条款",
    "我已阅读并同意上述条款",
    "模型能力",
    "模型 ID",
    "选择 Alias（模型 ID）",
    "添加模型",
    "Key 须以 {prefix} 开头",
    "请填写 Base URL",
    "请阅读并同意条款",
    "请至少添加一个模型能力",
    "模型 ID 不能重复",
    "模型 ID 最多 200 个字符",
    "模型 ID 不能包含控制字符",
    "模型能力必须与上游协议一致",
    "创建为禁用草稿；验证与路由尚未就绪",
    "选择套餐后创建为禁用草稿；路由尚未就绪",
    "路由尚未就绪",
    "创建为禁用账号，验证连接成功后手动启用。",
    "验证连接",
    "连接验证成功，账号保持禁用，可手动启用。",
    "连接验证失败: {error}",
    "验证连接成功后才能启用",
    "目标端点由管理员自行选择并负责：使用 http:// 时 Key 将明文传输；验证连接会发送一次最小真实请求，可能产生服务商费用。",
    "Base URL 格式无效",
    "Base URL 必须是 http:// 或 https:// URL",
    "Base URL 不能包含用户名或密码",
    "{count} 个模型",
    "连接已验证：{time}",
    "连接已验证",
    "上次验证失败，请检查 Key 与端点配置后重试。",
    "以下为官方套餐参考，不是 OCG Manager 实时计价或用量。",
    "官方套餐参考 · 截至 {date}",
    "月费",
    "另加处理费",
    "每月含额度",
    "官方估算请求数",
    "滚动额度限制",
    "5 小时",
    "7 天",
    "请求数是官方估算，实际取决于模型、tokens 与缓存；部分模型有单独额度。",
    "套餐",
    "活动价",
    "原价",
    "每月 Credits",
    "基础版",
    "标准版",
    "高级版",
    "额度用尽不会转按量计费，到期未用额度不结转；实际价格和余额以 SCNet 控制台为准。",
    "仅限 AI 工具内交互式使用；禁止共享账号、自动化脚本、自定义应用后端及非交互批量调用。",
    "查看使用限制",
    "当前仍是禁用草稿；这里仅展示官方参考，不代表 OCG Manager 已支持路由、验证或实时用量。",
    "该方案暂不可路由",
    "该方案暂不可路由。",
    "待验证",
    "等待支持",
    "接入尚未就绪",
    "该方案验证功能暂不可用，创建后保持禁用草稿。",
    "账号待验证，验证通过前保持禁用。",
    "验证失败，请检查 Key 或等待该方案支持验证。",
    "验证失败",
    "验证中",
    "该方案无需价格表",
    "该方案未定价",
    "解析别名",
    "模型解析",
    "请求模型",
    "添加 {plan} 账号",
    "无法确定账号方案，请关闭后重试",
    "创建后不可修改",
    "账号创建失败，请重试",
  ] satisfies MessageKey[];

  for (const [localeName, catalog] of Object.entries(localizedCatalogs)) {
    for (const key of keys) {
      assert.notEqual(catalog[key], enUSMessages[key], `${localeName}: ${key}`);
    }
  }
});

test("translations react to locale changes and preserve interpolation", () => {
  setLocale("en-US");
  assert.equal(t("已复制 {label}", { label: "API Base URL" }), "Copied API Base URL");
  setLocale("zh-CN");
  assert.equal(t("已复制 {label}", { label: "Key" }), "已复制 Key");
});

test("a late lazy locale load cannot override a later locale selection", async () => {
  setLocale("ja-JP");
  setLocale("en-US");

  await new Promise<void>((resolve) => setTimeout(resolve, 0));

  assert.equal(locale.value, "en-US");
  assert.equal(t("已复制 {label}", { label: "Key" }), "Copied Key");
});

test("USD costs use the narrow dollar symbol and preserve requested precision", () => {
  for (const { value } of LOCALE_OPTIONS) {
    setLocale(value);
    assert.match(formatCost(0.00015, 5), /\$/);
    assert.doesNotMatch(formatCost(0.00015, 5), /US/);
  }
  setLocale(DEFAULT_LOCALE);
  assert.match(formatCost(0.00015, 5), /0\.00015/);
  assert.equal(formatCost(-5), "-$5.00");
  assert.equal(formatCost(-0.005), "-$0.0050");
});

test("network failures use a human-facing fallback without hiding server errors", () => {
  assert.equal(userFacingError(new TypeError("Failed to fetch"), "offline"), "offline");
  assert.equal(userFacingError(new TypeError("NetworkError when attempting to fetch resource."), "offline"), "offline");
  assert.equal(userFacingError(new TypeError("Load failed"), "offline"), "offline");
  assert.equal(userFacingError(new Error("server detail"), "offline"), "server detail");
  assert.equal(userFacingError("plain", "offline"), "plain");
  assert.equal(userFacingError(new TypeError("syntax boom"), "offline"), "syntax boom");
});
