import { existsSync, mkdirSync, readdirSync, rmSync, renameSync, mkdtempSync } from "node:fs";
import { basename, dirname, join } from "node:path";
import { spawnSync } from "node:child_process";

export const SECRET_ENV_PATTERNS = [
  "TAURI_SIGNING_PRIVATE_KEY",
  "TAURI_SIGNING_PRIVATE_KEY_PASSWORD",
  "GH_TOKEN",
  "GITHUB_TOKEN",
  "ACTIONS_RUNTIME_TOKEN",
  "ACTIONS_ID_TOKEN_REQUEST_TOKEN",
  "SIGNING_SECRET",
  "NODE_EXTRA_CA_CERTS",
  "NPM_TOKEN",
  "CARGO_REGISTRY_TOKEN",
  "DOCKER_PASSWORD",
  "DOCKER_TOKEN",
  "AWS_ACCESS_KEY_ID",
  "AWS_SECRET_ACCESS_KEY",
  "AWS_SESSION_TOKEN",
];

/**
 * Build a sanitized environment for the appimagetool subprocess.
 *
 * The default environment contains signing keys, CI tokens, and other secrets
 * that should never be passed to an external binary. This function starts from
 * an empty object and only copies known-safe variables, preventing any secret
 * from leaking to appimagetool.
 *
 * @returns {Record<string, string>}
 */
export function sanitizedEnv() {
  const safe = {};

  const safeKeys = new Set([
    "PATH",
    "HOME",
    "USER",
    "LOGNAME",
    "TMPDIR",
    "TEMP",
    "TMP",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "LC_MESSAGES",
    "XDG_RUNTIME_DIR",
    "DISPLAY",
    "WAYLAND_DISPLAY",
    "DBUS_SESSION_BUS_ADDRESS",
    "LD_LIBRARY_PATH",
    "TERM",
    "SHELL",
  ]);

  for (const key of safeKeys) {
    if (process.env[key] !== undefined) safe[key] = process.env[key];
  }

  // AppImage-specific overrides
  safe.ARCH = "x86_64";
  safe.APPIMAGE_EXTRACT_AND_RUN = "1";

  return safe;
}

/**
 * Audit the sanitized env and reject if any known secret patterns leaked through.
 *
 * @param {Record<string, string>} env
 */
export function assertNoSecretsInEnv(env) {
  for (const pattern of SECRET_ENV_PATTERNS) {
    if (pattern in env) {
      throw new Error(
        `Refusing to pass secret-bearing env var "${pattern}" to appimagetool subprocess.`,
      );
    }
  }
}

/**
 * Remove bundled libwayland-* libraries from a Linux AppImage.
 *
 * The AppImage produced by Tauri bundles libwayland-client, libwayland-cursor,
 * and libwayland-egl from the build host (typically Ubuntu). On Arch Linux and
 * other rolling-release distributions these bundled libraries conflict with the
 * system Mesa/EGL stack, causing a dynamic-loader symbol resolution error:
 *   /usr/lib/libEGL_mesa.so.0: undefined symbol: wl_fixes_interface
 *
 * This function extracts the AppImage, deletes the conflicting libraries,
 * repacks the AppImage using appimagetool, and verifies the result.
 *
 * @param {string} appImagePath - Path to the .AppImage file to prune.
 * @param {object} [options]
 * @param {string} [options.appimagetoolPath] - Path to appimagetool executable.
 *   Defaults to the APPIMAGETOOL environment variable.
 * @param {boolean} [options.failClosed=false] - When true, any failure or
 *   unexpected condition (missing tool, no libraries found, extraction/repack
 *   error) throws instead of silently returning the original path.
 * @param {Function} [options.run] - Inject a custom spawn runner for testing.
 *   Signature: (cmd: string, args: string[], opts?: object) => void.
 * @returns {string} Path to the pruned AppImage.
 */
export function pruneAppImageWaylandLibs(appImagePath, options = {}) {
  const { appimagetoolPath, failClosed = false, run: customRun } = options;
  const toolPath = appimagetoolPath || process.env.APPIMAGETOOL;

  if (!toolPath) {
    const msg = "appimagetool not found (set APPIMAGETOOL env var or pass options.appimagetoolPath).";
    if (failClosed) throw new Error(msg);
    console.warn(`${msg} Skipping AppImage Wayland library pruning.`);
    return appImagePath;
  }

  const appImageName = basename(appImagePath);
  // Colocate the scratch dir with the artifact: same filesystem, so the final
  // renameSync cannot hit EXDEV, and mkdtemp keeps concurrent runs isolated.
  const extractDir = mkdtempSync(join(dirname(appImagePath), ".appimage-prune-"));

  function defaultRun(cmd, args, opts = {}) {
    console.log(`> ${basename(cmd)} ${args.join(" ")}`);
    const result = spawnSync(cmd, args, { stdio: "inherit", ...opts });
    if (result.error) throw new Error(`${cmd} failed: ${result.error.message}`);
    if (result.status !== 0) throw new Error(`${cmd} exited with status ${result.status}.`);
  }
  const run = customRun || defaultRun;

  try {
    // Extract AppImage
    console.log(`Extracting AppImage: ${appImagePath}`);
    run(appImagePath, ["--appimage-extract"], { cwd: extractDir });

    const squashfsRoot = join(extractDir, "squashfs-root");
    if (!existsSync(squashfsRoot)) {
      throw new Error(`AppImage extraction did not produce squashfs-root in ${extractDir}`);
    }

    // Remove bundled libwayland-* libraries
    const libDir = join(squashfsRoot, "usr", "lib");
    let removedCount = 0;
    const removedLibs = [];
    if (existsSync(libDir)) {
      const waylandLibs = readdirSync(libDir).filter((f) => f.startsWith("libwayland-"));
      for (const lib of waylandLibs) {
        const libPath = join(libDir, lib);
        console.log(`Removing bundled library: ${libPath}`);
        rmSync(libPath, { force: true });
        removedLibs.push(lib);
        removedCount++;
      }
    }

    if (removedCount === 0) {
      const msg = "No bundled libwayland-* libraries found; nothing to prune.";
      if (failClosed) throw new Error(msg);
      console.log(msg);
      return appImagePath;
    }

    console.log(
      `Removed ${removedCount} bundled Wayland librar${removedCount === 1 ? "y" : "ies"}: ${removedLibs.join(", ")}.`,
    );

    // Repack using appimagetool with sanitized environment (no secrets)
    const prunedPath = join(extractDir, appImageName);
    const subprocessEnv = sanitizedEnv();
    assertNoSecretsInEnv(subprocessEnv);
    console.log("Repacking AppImage with appimagetool...");
    run(toolPath, [squashfsRoot, prunedPath], { env: subprocessEnv });

    // Verify the pruned AppImage contains no libwayland-* libraries
    console.log("Verifying pruned AppImage has no bundled Wayland libraries...");
    const verifyDir = join(extractDir, "verify");
    mkdirSync(verifyDir, { recursive: true });
    run(prunedPath, ["--appimage-extract"], { cwd: verifyDir, env: subprocessEnv });
    const verifySquashfs = join(verifyDir, "squashfs-root");
    if (!existsSync(verifySquashfs)) {
      throw new Error("Verification extraction did not produce squashfs-root.");
    }
    const verifyLibDir = join(verifySquashfs, "usr", "lib");
    if (existsSync(verifyLibDir)) {
      const remainingLibs = readdirSync(verifyLibDir).filter((f) => f.startsWith("libwayland-"));
      if (remainingLibs.length > 0) {
        throw new Error(
          `Pruned AppImage still contains bundled Wayland libraries: ${remainingLibs.join(", ")}. ` +
          "Pruning was ineffective — refusing to publish.",
        );
      }
    }
    console.log("Verification passed: no libwayland-* libraries remain in the pruned AppImage.");

    // Backup original and replace with pruned
    const backupPath = `${appImagePath}.wayland-backup`;
    rmSync(backupPath, { force: true });
    console.log("Replacing original AppImage with pruned version");
    renameSync(appImagePath, backupPath);
    try {
      renameSync(prunedPath, appImagePath);
    } catch (err) {
      // Restore original on replacement failure
      renameSync(backupPath, appImagePath);
      throw err;
    }
    rmSync(backupPath, { force: true });

    console.log("AppImage Wayland library pruning complete.");
    return appImagePath;
  } finally {
    rmSync(extractDir, { recursive: true, force: true });
  }
}
