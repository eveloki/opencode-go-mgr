import assert from "node:assert/strict";
import test from "node:test";
import type { ProviderCatalogEntry } from "../api/providers.ts";
import { buildPlanChooserGroups, buildPlanOptions } from "./account-plan-options.ts";

function catalogEntry(
  provider_id: string,
  offering_id: string,
  extra: Partial<ProviderCatalogEntry> = {},
): ProviderCatalogEntry {
  return {
    provider_id,
    offering_id,
    display_name: `${provider_id}/${offering_id}`,
    display_family: provider_id,
    credential_kind: "api_key",
    quota_scope: "key",
    singleton: false,
    creation_availability: "available",
    verification_policy: "required",
    verification_runtime_availability: "unavailable",
    routable: false,
    managed_registration: false,
    pricing_availability: "unavailable",
    usage_availability: "unavailable",
    manual_usage_calibration: false,
    quota_unit: "credits",
    model_source: "test",
    auth_schemes: ["bearer"],
    upstream_protocols: ["chat_completions"],
    form_fields: [],
    model_aliases: [],
    ...extra,
  };
}

test("empty or failed catalogs keep the explicit OpenCode Go import option", () => {
  for (const catalog of [null, undefined, []] as const) {
    const options = buildPlanOptions(catalog);
    const go = options.find(({ plan }) => plan.id === "opencode-go")!;
    assert.equal(go.disabled, false);
    assert.equal(go.managed, true);
    assert.equal(go.label, "OpenCode Go");
    assert.equal(options.some(({ plan }) => plan.id === "zen-free"), false);
  }
});

test("add-account chooser omits singleton Zen Free and groups remaining families", () => {
  const catalog = [
    catalogEntry("opencode", "go", {
      display_name: "OpenCode Go Catalog",
      routable: true,
      creation_availability: "available",
    }),
    catalogEntry("command-code", "goat", { routable: false, creation_availability: "available" }),
    catalogEntry("scnet", "token-plan-basic", { routable: false, creation_availability: "available" }),
    catalogEntry("custom", "api", { routable: true, creation_availability: "available" }),
  ];
  const options = buildPlanOptions(catalog);
  assert.deepEqual(options.map(({ plan }) => plan.id), [
    "opencode-go",
    "command-code-goat",
    "scnet",
    "custom-endpoint",
  ]);
  assert.deepEqual(
    buildPlanChooserGroups(catalog).map((group) => [group.id, group.options.map(({ plan }) => plan.id)]),
    [
      ["available", ["opencode-go", "custom-endpoint"]],
      ["draft", ["command-code-goat"]],
      ["unavailable", ["scnet"]],
    ],
  );
});

test("GOAT follows the catalog: routable means addable with verify-then-enable copy", () => {
  const routable = buildPlanChooserGroups([
    catalogEntry("command-code", "goat", { routable: true, creation_availability: "available" }),
  ]);
  const available = routable.find((group) => group.id === "available")!;
  const goat = available.options.find(({ plan }) => plan.id === "command-code-goat")!;
  assert.equal(goat.disabled, false);
  assert.equal(goat.creationHint, "创建为禁用账号，验证连接成功后手动启用。");

  const draft = buildPlanChooserGroups([
    catalogEntry("command-code", "goat", { routable: false, creation_availability: "available" }),
  ]);
  const draftGoat = draft
    .find((group) => group.id === "draft")!
    .options.find(({ plan }) => plan.id === "command-code-goat")!;
  assert.equal(draftGoat.disabled, false);
  assert.equal(draftGoat.creationHint, "创建为禁用草稿；验证与路由尚未就绪");
});

test("SCNet is sealed: archived, non-selectable, and never a draft", () => {
  for (const catalog of [
    null,
    [catalogEntry("scnet", "token-plan-basic", { routable: true, creation_availability: "available" })],
  ] as const) {
    const scnet = buildPlanOptions(catalog).find(({ plan }) => plan.id === "scnet")!;
    assert.equal(scnet.disabled, true);
    assert.equal(scnet.disabledReason, "该方案已归档，暂不支持创建");
    assert.equal(scnet.creationHint, "");
    const groups = buildPlanChooserGroups(catalog);
    const home = groups.find((group) => group.options.some(({ plan }) => plan.id === "scnet"))!;
    assert.equal(home.id, "unavailable");
  }
});

test("plan hints and disabled reasons are translation keys", () => {
  const catalog = [
    catalogEntry("opencode", "go", { display_name: "OpenCode Go Catalog" }),
    catalogEntry("scnet", "token-plan-basic"),
    catalogEntry("scnet", "token-plan-standard"),
    catalogEntry("scnet", "token-plan-premium"),
  ];
  const options = buildPlanOptions(catalog);
  const go = options.find(({ plan }) => plan.id === "opencode-go")!;
  const scnet = options.find(({ plan }) => plan.id === "scnet")!;
  const custom = options.find(({ plan }) => plan.id === "custom-endpoint")!;

  assert.equal(go.label, "OpenCode Go Catalog");
  assert.equal(scnet.label, "SCNet");
  assert.equal(scnet.disabled, true);
  assert.equal(custom.disabledReason, "服务商目录未提供该方案");

  const unavailable = buildPlanOptions([
    catalogEntry("command-code", "goat", {
      creation_availability: "unavailable",
      creation_unavailable_reason: "Raw backend English must not leak",
    }),
  ]).find(({ plan }) => plan.id === "command-code-goat")!;
  assert.equal(unavailable.disabledReason, "该方案暂不可用");
});
