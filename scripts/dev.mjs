import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";

export const DEFAULT_DEV_GATEWAY_PORT = "19042";

export function devEnvironment(source = process.env) {
  return {
    ...source,
    OCG_GATEWAY_PORT: source.OCG_GATEWAY_PORT?.trim() || DEFAULT_DEV_GATEWAY_PORT,
  };
}

const isMain = process.argv[1]
  && fileURLToPath(import.meta.url).toLowerCase() === process.argv[1].toLowerCase();

if (isMain) {
  const tauriCli = fileURLToPath(new URL("../node_modules/@tauri-apps/cli/tauri.js", import.meta.url));
  const env = devEnvironment();
  console.log(`Gateway development port: ${env.OCG_GATEWAY_PORT}`);

  const child = spawn(process.execPath, [tauriCli, "dev"], {
    cwd: process.cwd(),
    env,
    stdio: "inherit",
    windowsHide: false,
  });

  for (const signal of ["SIGINT", "SIGTERM"]) {
    process.once(signal, () => {
      if (!child.killed) child.kill(signal);
    });
  }

  child.once("error", (error) => {
    console.error(`Failed to start Tauri development mode: ${error.message}`);
    process.exitCode = 1;
  });
  child.once("exit", (code, signal) => {
    process.exitCode = code ?? (signal === "SIGINT" ? 130 : 1);
  });
}
