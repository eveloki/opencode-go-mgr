import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const design = readFileSync(new URL("../../DESIGN.md", import.meta.url), "utf8");
const userEn = readFileSync(new URL("../../docs/USER.md", import.meta.url), "utf8");
const userZh = readFileSync(new URL("../../docs/USER.zh-CN.md", import.meta.url), "utf8");
const docsIndex = readFileSync(new URL("../../docs/README.md", import.meta.url), "utf8");

function headings(markdown: string): string[] {
  return markdown.split(/\r?\n/).filter((line) => /^#{2,3} /.test(line));
}

function tocBlock(markdown: string, title: string): string[] {
  const start = markdown.indexOf(title);
  assert.ok(start >= 0, `missing ${title}`);
  const next = markdown.indexOf("\n## ", start + title.length);
  assert.ok(next > start, `TOC for ${title} has no following H2`);
  return markdown
    .slice(start, next)
    .split(/\r?\n/)
    .filter((line) => /^\s*- \[/.test(line));
}

test("DESIGN.md names Providers as the fourth of seven views", () => {
  assert.match(
    design,
    /Dashboard, Access Keys, Accounts, Providers, Applications, Logs, Settings/,
  );
  assert.doesNotMatch(design, /Accounts, Pricing, Applications/);
  assert.match(design, /Providers is the supplier control plane/);
  assert.match(design, /Chat Completions \/ Responses \/ Messages switches/);
  assert.match(design, /may consume quota/);
  assert.match(design, /never automatic on page load/);
  assert.match(design, /Do call the access credential “Key”/);
});

test("USER guides keep matching TOC structure with Providers replacing Pricing", () => {
  const enToc = tocBlock(userEn, "## Table Of Contents");
  const zhToc = tocBlock(userZh, "## 目录");
  assert.equal(enToc.length, zhToc.length);
  assert.match(userEn, /- \[Providers\]\(#providers\)/);
  assert.match(userZh, /- \[供应商\]\(#供应商\)/);
  assert.doesNotMatch(userEn, /- \[Pricing\]\(#pricing\)/);
  assert.doesNotMatch(userZh, /- \[价格表\]\(#价格表\)/);
  assert.equal(headings(userEn).length, headings(userZh).length);
  assert.match(userEn, /### Providers/);
  assert.match(userZh, /### 供应商/);
});

test("USER guides describe the Providers control plane and drop stale locations", () => {
  assert.match(userEn, /Configurable HTTP adapter, not a base class/);
  assert.match(userZh, /Configurable HTTP 适配器，不是基类/);
  assert.match(userEn, /`Provider\(provider_id\)`/);
  assert.match(userZh, /`Provider\(provider_id\)`/);
  assert.match(userEn, /`CustomEndpoint\(account_id\)`/);
  assert.match(userZh, /`CustomEndpoint\(account_id\)`/);
  assert.match(userEn, /Chat Completions, Responses,\s+and Messages/);
  assert.match(userZh, /Chat Completions、Responses、Messages/);
  assert.match(userEn, /may consume quota/);
  assert.match(userZh, /可能消耗\s*额度/);
  assert.match(userEn, /\?view=pricing/);
  assert.match(userZh, /\?view=pricing/);
  assert.match(userEn, /Schema v26/);
  assert.match(userZh, /schema v26/);
  assert.match(userEn, /\*\*Open\s+provider\*\*/);
  assert.match(userZh, /\*\*前往供应商\*\*/);
  assert.doesNotMatch(userEn, /There is no separate provider page/);
  assert.doesNotMatch(userZh, /没有独立的供应商页/);
  assert.doesNotMatch(userEn, /Use the card's \*\*Fetch models\*\* action/);
  assert.doesNotMatch(userZh, /通过卡片的 \*\*获取模型\*\* 动作刷新/);
});

test("USER guides keep GOAT/SCNet non-routable and refresh/probe manual-only", () => {
  assert.match(userEn, /probes cannot promote them to production\s+routing/);
  assert.match(userZh, /探测不能把它们提升为生产路由/);
  assert.match(userEn, /Refresh is never automatic/);
  assert.match(userZh, /刷新绝不会自动发生/);
  assert.match(userEn, /Client requests never probe/);
  assert.match(userZh, /客户端请求不会探测/);
  assert.match(userEn, /effective enabled protocol/);
  assert.match(userZh, /有效启用协议/);
});

test("docs index routes user facts through Providers and the contract module", () => {
  assert.match(docsIndex, /Provider contracts \/ 供应商合约/);
  assert.match(docsIndex, /provider_contracts\.rs/);
  assert.match(docsIndex, /ConfigurableHttpAdapter/);
  assert.match(docsIndex, /Do not claim there is\s+no supplier page/);
  assert.match(docsIndex, /不要写“没有供应商页”/);
});
