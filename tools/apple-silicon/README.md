# Native Apple Silicon for BepInEx 5 games (experimental)

Many Unity games ship a universal (arm64 + x86_64) Mac build, but with BepInEx 5 mods r2modmac falls back to Rosetta.
This tool lets those games run natively with mods.

## Why it fell back to Rosetta

BepInEx 5's Harmony patches game methods through MonoMod (legacy, 22.x). MonoMod makes a method's compiled code
writable with `mprotect(RWX)`. On Apple Silicon that code lives in `MAP_JIT` memory: a thread may only write to it after
`pthread_jit_write_protect_np(0)`, and while that write mode is on, the thread can't execute any `MAP_JIT` code.
MonoMod's first self-test fails, `DetourHelper.Runtime` stays null, and the preloader dies in
`HarmonyInteropFix.Apply` → `DetourHelper.GetIdentifiable` (`NullReferenceException`). The launcher's preloader-crash
check then pins the game to x64.

## What this adds

- `jitwrite.c` → `libjitwrite.dylib` (arm64): does each write to code entirely in native code: unlock, `memcpy`, lock,
  `sys_icache_invalidate`. It has to be native: managed code is itself `MAP_JIT` code and can't run while writes are on.
- `AppleSiliconMods.dll`: Doorstop's `Doorstop.Entrypoint.Start`. It sets MonoMod's `DetourHelper.Native` to a platform
  that writes MonoMod's AArch64 jump through `libjitwrite`, with x17 instead of x15 (`ldr x17, #8; br x17; .quad
  target`): x16/x17 are the ABI's scratch registers at a call's entry. It then restores
  `DOORSTOP_INVOKE_DLL_PATH` to BepInEx's preloader (Doorstop sets it to the assembly it started) and calls BepInEx's
  entry point. Under Rosetta, or if anything in the setup fails, BepInEx starts exactly as before.
- `AppleSiliconMods.Straftat.dll`: STRAFTAT counts every assembly loaded from outside its Managed folder as a mod that
  is incompatible with vanilla matchmaking, unless it carries the game's `[StraftatMod(true)]` or a compatible assembly
  references it. The shim changes no gameplay, so this marker carries the attribute and references the shim. The shim
  loads it only in games that ship `ComputerysModdingUtilities.dll`; it is compiled against a stub of that attribute
  (`StraftatMarker/ComputerysModdingUtilities`), and the game's own DLL is never copied.
- The launcher (`script_template_prelude.sh`) points Doorstop at `<game>/apple_silicon/AppleSiliconMods.dll` when the
  runtime arch is arm64 and that file exists. Nothing changes for games without it, and the x64 crash fallback still
  applies.
- UnityDoorstop 4.5.0's universal `libdoorstop.dylib` goes beside the game (only if the loader there has no arm64):
  packs often ship only `doorstop_libs/libdoorstop_x64.dylib`. `build.sh` downloads it pinned by SHA-256, with its
  LGPL-2.1 license.

## How it gets to the game

**r2modmac's macOS release builds do it by themselves.** The release build runs `build.sh` and bundles `dist/` into
the app (`src-tauri/tauri.apple-silicon.conf.json`). Before each modded launch, r2modmac puts the files beside the game
(`apple_silicon.rs`), but only when all of these hold:
- the Mac is Apple Silicon;
- the game has an arm64 build;
- BepInEx/core has a `BepInEx.Preloader.dll` with Doorstop 4's `Doorstop` entry point (BepInEx 5.4.22 or later;
  5.4.21 already reads `DOORSTOP_INVOKE_DLL_PATH`, so that string alone doesn't tell them apart);
- BepInEx/core has `MonoMod.RuntimeDetour.dll`.

It removes an earlier shim (and the Doorstop 4 loader it put there) if the game's BepInEx changes to one it can't
start; Repair does the same, so a runtime it left unhealthy can be repaired. If the game crashes natively with the
shim in place (SIGABRT, SIGSEGV, SIGBUS, SIGILL, SIGTRAP), even after BepInEx started, the launcher pins it to Rosetta.
The pin is a file per profile beside the game (`.r2modmac_force_x64_state.<profile id>`), so one profile's mods don't
move the others. r2modmac clears every pin when it puts the shim there for the first time, or a newer shim (a pin may
have come from what it fixes); otherwise the pin stays. Dev builds (`tauri dev`, plain `tauri build`) don't carry the
files, so nothing changes there. Anything that goes wrong is logged, and the launch goes on under Rosetta.

## Architecture per profile (`macos/architecture.rs`)

Each profile has an **Architecture** setting, under *Advanced* in the profile's Settings (macOS, Apple Silicon, BepInEx
games with a native Mac build only):

- **Automatic** (the default; the profile has no `architecture` field): native when the game and its loader allow it,
  Rosetta after a native crash (the pin above).
- **Apple Silicon**: always native. No retry, no pin; a crash is a crash, so it can be tried on mods that crash under
  Automatic.
- **Intel (Rosetta)**: always Rosetta.

The section stays closed and says nothing until a profile is off the default: it opens by itself when a mode is chosen
or a crash pinned the profile, with the reason and a "Try Apple Silicon again" button. Options a game can't do are
disabled with the reason (no Apple Silicon or Intel build, Rosetta missing, no loader for that architecture).

`run_bepinex.sh` is shared by every profile of a game, so before each launch r2modmac writes
`<game>/.r2modmac_launch_arch` (`mode`, `profile`, `rosetta`); the launcher reads it before picking the architecture.
Without the file (a launch started from Steam alone, an older launcher) everything is automatic, as before.

**Without Rosetta** (Apple is removing it) Automatic never picks or retries Intel and ignores a pin, and a game with no
Apple Silicon build stops with "Rosetta Is Required" instead of crashing. `R2MODMAC_ASSUME_NO_ROSETTA=1` makes r2modmac
behave as if Rosetta weren't installed, to try that on a Mac that has it.

## By hand (builds without it)

Needs Xcode's command line tools and the .NET SDK.

```sh
tools/apple-silicon/build.sh
tools/apple-silicon/install.sh "<game folder>"        # e.g. ~/Library/Application Support/Steam/steamapps/common/Valheim
```

Then launch the game with mods from r2modmac. `<game>/apple_silicon/apple_silicon.log` should say "Native Apple
Silicon: MonoMod writes code through libjitwrite", and `BepInEx/LogOutput.log` should reach "Chainloader startup
complete". `install.sh --remove "<game folder>"` undoes it. To compare with Rosetta for one launch, copy
`<game>/.r2modmac_codesign_state` to `<game>/.r2modmac_force_x64_state`; delete that file to go back to native.

## Tested

Valheim 1.0.20 (BepInExPack Valheim 5.4.2351 = BepInEx 5.4.23.5, MonoMod 22.1.29.1), 5 mods, M4 Pro, macOS 26:
- native arm64 process, all 5 plugins loaded, played online;
- world load 12–13 s native vs 31 s under Rosetta;
- in-world FPS the same (GPU-bound scene).

r2modmac's checks, MacBook Pro M1 Pro, macOS 27, launched from r2modmac:
- Hollow Knight Silksong (BepInEx 5.4.23.4, 10 plugins): native arm64, all plugins loaded.
- STRAFTAT (BepInEx 5.4.23.5, Mod_Menu and Minimal_Viewmodels_Reborn): native arm64, and with the marker the game logs
  "No incompatible assemblies found!" (without it, the shim alone put the player in the modded matchmaking pool).
- Cult of the Lamb (BepInEx 5.4.21): not used (older BepInEx); runs under Rosetta as before.
- Inscryption (BepInEx 5.4.23.5, InscryptionAPI and 10 content mods): crashes natively in 5 of 8 launches, in Mono's
  GC while InscryptionAPI clones cards (the same `ConditionalWeakTable` code throws `InvalidCastException` under
  Rosetta, where it doesn't crash: 0 of 8). With InscryptionAPI alone: 0 of 6. The launcher's crash pin sends it to
  Rosetta from the next launch on.

Only BepInEx 5 with legacy MonoMod is covered. Don't install it for BepInEx 6 games: their preloader has another
name, and their MonoMod (MonoMod.Core) works differently.
