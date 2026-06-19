# Agent Notes

## Project

r2modmac is a Tauri 2 desktop app with a React/TypeScript frontend and a Rust backend. It is a native macOS-focused mod manager inspired by r2modman for Thunderstore-supported games. The app manages game profiles, installs/syncs Thunderstore mods, and launches games either natively on macOS or through Windows compatibility tools such as Wine, CrossOver, Whisky, and Wineskin/Sikarugir wrappers.

Key areas:

- `src/`: React UI, Zustand stores, Tauri adapter.
- `src-tauri/src/commands/mod_commands.rs`: mod install/sync logic.
- `src-tauri/src/commands/game_commands/`: game path discovery, profile sync, process checks, and launch flows.
- `src-tauri/src/commands/game_commands/macos/`: native macOS runtime/Steam launch option logic.
- `src-tauri/src/commands/game_commands/windows/`: Windows game launch through host compatibility runners.

## GitHub Issue 14

Issue: https://github.com/Zard-Studios/r2modmac/issues/14

Title: "Not working when using Sikarugir"

Reporter says Lethal Company works normally from Steam inside Sikarugir, but does not open through r2modmac. Screenshot shows a Windows profile and paths like:

`~/Applications/Sikarugir/SteamWin.app/Contents/SharedSupport/prefix/drive_c/Program Files (x86)/Steam/steamapps/common/Lethal Company`

Sikarugir is the Kegworks/Wineskin successor. 

## Sikarugir Bundle Layout (Verified on Local Install)

Real structure of `~/Applications/Sikarugir/Steam.app`:

```
Steam.app/
  Contents/
    Info.plist              # Holds launch config: "Program Name and Path" + "Program Flags"
    MacOS/
      Sikarugir             # Main Mach-O binary (universal arm64+x86_64)
      launcher  -> Sikarugir   # Symlink
      wineskinlauncher -> launcher  # Symlink (Wineskin compat alias)
    drive_c -> SharedSupport/prefix/drive_c  # Symlink convenience alias
    SharedSupport/
      prefix/               # WINEPREFIX root
        drive_c/            # The real Windows C: drive tree
    Logs/
      LastRunWine.log
```

**Key Facts:**
- `Info.plist` key `Program Name and Path`: Windows path WITHOUT `C:`, with forward slashes and leading `/` (e.g. `/Program Files (x86)/Steam/steam.exe`)
- `Info.plist` key `Program Flags`: space-separated CLI args (e.g. `-applaunch 1966720`)
- The launcher binary **cannot** be invoked directly from CLI — `NSBundle.main()` fails → `WineAppInitializationError error 0`
- **Correct launch method**: modify `Info.plist` via `/usr/libexec/PlistBuddy`, then call `open -n <wrapper.app>`, then restore `Info.plist` after ~5 seconds in a background thread

## Fix Implemented (wine.rs + windows/mod.rs)

- `find_macos_wineskin_launcher_binary()` now returns the **bundle `.app` path** (not the launcher binary path). Accepts any of: `Sikarugir`, `launcher`, `wineskinlauncher`, `WineskinLauncher` in `Contents/MacOS/`.
- `launch_macos_wineskin_program()` now uses PlistBuddy + `open -n` instead of `.bat` + `WSS-installer`.
- `windows_rel_path_from_drive_c()` converts native macOS paths to Sikarugir's plist format (forward slashes, leading `/`, no `C:`).

## Useful Verification

Run focused Rust tests from the Tauri crate:

`cd src-tauri && cargo test game_commands::macos::wine::tests`

If a real Sikarugir install is available, use a Windows profile whose Steam Directory points at:

`SteamWin.app/Contents/SharedSupport/prefix/drive_c/Program Files (x86)/Steam`

Then apply/sync a game mod profile and press the launch button. Check `LastRunWine.log` inside the wrapper if the game process doesn't start. The log should contain `steam.exe` with `-applaunch <id>` as arguments.

## PlistBuddy Note

`/usr/libexec/PlistBuddy` is a standard macOS utility (available on all macOS versions since 10.5). It is used to read/write `Info.plist` keys with proper plist encoding. `defaults write` fails with paths containing spaces.

## Clear Profile Cache

- Command: `clear_profile_cache` in `src-tauri/src/commands/profile_commands.rs`.
- Scans `profiles_dir` (retrieved via `app_data_dir()`, ensuring cross-platform support across macOS, Linux, and Windows) and removes legacy cache folders:
  - `BepInEx`
  - `Balatro`
  - `MelonLoader`
- Cleans up download cache in `app_cache_dir()` (e.g. `chunks/` directory and package index cache files ending in `_packages.json.gz`/`_packages_v2.json.gz`).

## Profile Import Logic

- Handled in `src/hooks/useProfileActions.ts`.
- **New Mode (`legacyInstallMode = false`):**
  - Instantaneous import (no file downloads).
  - Queries Thunderstore for packages to verify exist (using case-insensitive lookup to prevent mismatches), adds them to the profile store with `pending_sync: true` and updates profile state with `needs_sync: true`.
  - To prevent immediate downloads, the `autoApplyProfileRef` ref is set to the new profile ID, skipping the frontend `useEffect` auto-apply sync cycle during the initial active profile change.
  - The actual downloading and extracting happens on-demand when syncing/launching the game (Apply to Game).
- **Legacy Mode (`legacyInstallMode = true`):**
  - Directly downloads and installs all mods in the profile into the game folder.
  - Obeys parallel download configurations (`concurrency = MAX_PARALLEL_OPS = 10` if `installInParallel` is active) and displays progress utilizing the Tauri `mod-download-progress` event (showing download speed, bytes, and count).

## Sync & Reinstallation Logic

- Handled in `src-tauri/src/commands/game_commands/sync.rs`.
- Reads `manifest.json` as a fallback when directory names lack version suffix (e.g. `Author-ModName`) to determine the active version, preventing redundant reinstallations.
- Checks ownership manifest files (`.r2modmac/manifests`) to avoid deleting dependency directories owned by "kept" manifests.
- Verifies files listed in kept manifests exist in the game directory using `manifest_files_exist`. If a mod's manifest is kept and its files are found, it is skipped from `to_install`, preventing a reinstall loop for core and patcher mods (like `UniTask` or `AutoHookGenPatcher`) which are not deployed in `BepInEx/plugins`.
- Properly distinguishes between vanilla/modded runtimes, preventing duplicate copies of requirements (like BepInEx/Lovely) if they are already fully populated.

