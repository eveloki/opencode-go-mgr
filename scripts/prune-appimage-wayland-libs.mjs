import { existsSync, mkdirSync, readdirSync, rmSync, renameSync } from "node:fs";
import { basename, join } from "node:path";
import { spawnSync } from "node:child_process";

/**
 * Remove bundled libwayland-* libraries from a Linux AppImage.
 *
 * The AppImage produced by Tauri bundles libwayland-client, libwayland-cursor,
 * and libwayland-egl from the build host (typically Ubuntu). On Arch Linux and
 * other rolling-release distributions these bundled libraries conflict with the
 * system Mesa/EGL stack, causing a dynamic-loader symbol resolution error:
 *   /usr/lib/libEGL_mesa.so.0: undefined symbol: wl_fixes_interface
 *
 * This function extracts the AppImage, deletes the conflicting libraries, and
 * repacks the AppImage using appimagetool.
 *
 * @param {string} appImagePath - Path to the .AppImage file to prune.
 * @param {string} [appimagetoolPath] - Path to appimagetool executable.
 *   Defaults to the APPIMAGETOOL environment variable.
 * @returns {string} Path to the (possibly pruned) AppImage.
 */
export function pruneAppImageWaylandLibs(appImagePath, appimagetoolPath) {
  const toolPath = appimagetoolPath || process.env.APPIMAGETOOL;
  if (!toolPath) {
    console.warn(
      "appimagetool not found (set APPIMAGETOOL env var or pass --appimagetool). "
      + "Skipping AppImage Wayland library pruning.",
    );
    return appImagePath;
  }

  const cwd = process.cwd();
  const appImageName = basename(appImagePath);
  const extractDir = join(cwd, `.appimage-prune-${process.pid}`);

  function run(cmd, args, opts = {}) {
    console.log(`> ${basename(cmd)} ${args.join(" ")}`);
    const result = spawnSync(cmd, args, { stdio: "inherit", ...opts });
    if (result.error) throw new Error(`${cmd} failed: ${result.error.message}`);
    if (result.status !== 0) throw new Error(`${cmd} exited with status ${result.status}.`);
  }

  try {
    // Create temp extraction directory
    rmSync(extractDir, { recursive: true, force: true });
    mkdirSync(extractDir, { recursive: true });

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
    if (existsSync(libDir)) {
      const waylandLibs = readdirSync(libDir).filter((f) => f.startsWith("libwayland-"));
      for (const lib of waylandLibs) {
        const libPath = join(libDir, lib);
        console.log(`Removing bundled library: ${libPath}`);
        rmSync(libPath, { force: true });
        removedCount++;
      }
    }

    if (removedCount === 0) {
      console.log("No bundled libwayland-* libraries found; nothing to prune.");
      rmSync(extractDir, { recursive: true, force: true });
      return appImagePath;
    }

    console.log(`Removed ${removedCount} bundled Wayland librar${removedCount === 1 ? "y" : "ies"}.`);

    // Repack using appimagetool
    const prunedPath = join(extractDir, appImageName);
    console.log("Repacking AppImage with appimagetool...");
    run(toolPath, [squashfsRoot, prunedPath], {
      env: { ...process.env, ARCH: "x86_64", APPIMAGE_EXTRACT_AND_RUN: "1" },
    });

    // Backup original and replace with pruned
    const backupPath = `${appImagePath}.wayland-backup`;
    rmSync(backupPath, { force: true });
    console.log(`Replacing original AppImage with pruned version`);
    renameSync(appImagePath, backupPath);
    try {
      renameSync(prunedPath, appImagePath);
    } catch (err) {
      // Restore original on failure
      renameSync(backupPath, appImagePath);
      throw err;
    }
    rmSync(backupPath, { force: true });

    console.log("AppImage Wayland library pruning complete.");
    return appImagePath;
  } catch (error) {
    console.warn(`AppImage Wayland library pruning failed: ${error.message}`);
    console.warn("Falling back to the unpruned AppImage.");
    return appImagePath;
  } finally {
    rmSync(extractDir, { recursive: true, force: true });
  }
}
