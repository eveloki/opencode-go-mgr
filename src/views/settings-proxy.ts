import type { ProxyMode } from "../api/tauri";

export function normalizeProxyUrl(mode: ProxyMode, value: string): string {
  const trimmed = value.trim();
  if (!trimmed) {
    if (mode === "manual") throw new Error("手动代理模式需要填写代理地址");
    return "";
  }

  try {
    return canonicalizeProxyUrl(trimmed);
  } catch (error) {
    if (mode === "manual") throw error;
    return trimmed;
  }
}

function canonicalizeProxyUrl(value: string): string {
  let parsed: URL;
  try {
    parsed = new URL(value);
  } catch {
    throw new Error("代理地址格式无效");
  }
  if (parsed.protocol !== "http:" && parsed.protocol !== "https:") {
    throw new Error("代理地址必须是 http:// 或 https:// URL");
  }
  if (!parsed.hostname) throw new Error("代理地址必须包含主机");
  if (parsed.username || parsed.password) throw new Error("代理地址不能包含用户名或密码");
  if ((parsed.pathname && parsed.pathname !== "/") || parsed.search || parsed.hash) {
    throw new Error("代理地址不能包含路径、查询参数或片段");
  }
  return parsed.origin;
}
