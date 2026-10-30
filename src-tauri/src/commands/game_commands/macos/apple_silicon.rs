//! Native Apple Silicon for BepInEx 5 games (see tools/apple-silicon/README.md).
//!
//! The macOS release build carries tools/apple-silicon/dist in its resources as
//! `apple-silicon/`. Before a modded launch on an Apple Silicon Mac, those files
//! go beside a game that can use them: the shim in `apple_silicon/` and a
//! universal Doorstop loader as `libdoorstop.dylib`. The launcher's
//! `apple_silicon_shim` hook then starts the game natively. If that still
//! crashes, the launcher's preloader-crash check falls back to Rosetta as before.

use super::*;

pub(crate) const APPLE_SILICON_RESOURCE_DIR: &str = "apple-silicon";
pub(crate) const APPLE_SILICON_SHIM_DIR: &str = "apple_silicon";
// The shim (first), its native helper, and the marker it loads in STRAFTAT only.
const SHIM_FILES: [&str; 3] = [
    "AppleSiliconMods.dll",
    "libjitwrite.dylib",
    "AppleSiliconMods.Straftat.dll",
];
const LOADER: &str = "libdoorstop.dylib";

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum NativeAppleSilicon {
    /// Files were put beside the game (or updated).
    Installed,
    /// Everything was already in place.
    Unchanged,
    /// A shim from earlier was taken away: this game's BepInEx can't be started by it.
    Removed,
    /// Nothing to do, and why.
    NotApplicable(&'static str),
}

pub(crate) struct NativeAppleSiliconTarget<'a> {
    /// `<r2modmac.app>/Contents/Resources/apple-silicon`, if this build has it.
    pub resources: Option<&'a std::path::Path>,
    /// The folder with the game's app bundle: the loader and the shim go here.
    pub runtime_root: &'a std::path::Path,
    /// The BepInEx/core the game will load (beside the game, or a profile's).
    pub core_dir: &'a std::path::Path,
    pub host_is_apple_silicon: bool,
    pub game_has_arm64: bool,
}

fn file_contains(path: &std::path::Path, needle: &[u8]) -> bool {
    fs::read(path)
        .map(|bytes| bytes.windows(needle.len()).any(|window| window == needle))
        .unwrap_or(false)
}

pub(crate) const CPU_TYPE_ARM64: u32 = 0x0100_000c;
pub(crate) const CPU_TYPE_X86_64: u32 = 0x0100_0007;

/// Whether a Mach-O file (thin or universal) has an arm64 slice.
pub(crate) fn mach_o_bytes_have_arm64(bytes: &[u8]) -> bool {
    mach_o_bytes_have_arch(bytes, CPU_TYPE_ARM64)
}

/// Whether a Mach-O file (thin or universal) has a slice for this CPU type.
/// Only the header is read, so a file's first KiB is enough.
pub(crate) fn mach_o_bytes_have_arch(bytes: &[u8], cpu_type: u32) -> bool {
    let be = |at: usize| {
        bytes
            .get(at..at + 4)
            .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    };
    let le = |at: usize| {
        bytes
            .get(at..at + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    match be(0) {
        // Universal: big-endian header, then one entry per slice (20 or 32 bytes).
        Some(magic @ (0xcafe_babe | 0xcafe_babf)) => {
            let entry = if magic == 0xcafe_babf { 32 } else { 20 };
            let count = be(4).unwrap_or(0).min(16) as usize;
            (0..count).any(|i| be(8 + i * entry) == Some(cpu_type))
        }
        // Thin 64-bit, little-endian.
        _ => le(0) == Some(0xfeed_facf) && le(4) == Some(cpu_type),
    }
}

/// Can the shim start this game's BepInEx? It needs BepInEx 5 with legacy
/// MonoMod, new enough to be started by Doorstop 4: BepInEx 5.4.22 and later
/// have Doorstop 4's `Doorstop.Entrypoint.Start`. 5.4.21 already reads
/// `DOORSTOP_INVOKE_DLL_PATH`, so that alone doesn't tell them apart; the
/// `Doorstop` namespace does, as in `macos_preloader_matches_loader`.
pub(crate) fn bepinex_can_use_shim(core_dir: &std::path::Path) -> Result<(), &'static str> {
    let preloader = core_dir.join("BepInEx.Preloader.dll");
    if !preloader.is_file() {
        return Err("the game doesn't use BepInEx 5");
    }
    if !core_dir.join("MonoMod.RuntimeDetour.dll").is_file() {
        return Err("BepInEx has no MonoMod.RuntimeDetour");
    }
    if !file_contains(&preloader, b"Doorstop") {
        return Err("its BepInEx is older than 5.4.22");
    }
    Ok(())
}

/// Whether this build of r2modmac carries every file the shim needs.
pub(crate) fn resources_carry_shim(resources: Option<&std::path::Path>) -> bool {
    resources.is_some_and(|dir| {
        SHIM_FILES
            .iter()
            .chain([&LOADER])
            .all(|name| dir.join(name).is_file())
    })
}

fn copy_if_different(from: &std::path::Path, to: &std::path::Path) -> Result<bool, String> {
    let wanted = fs::read(from).map_err(|e| format!("Couldn't read {}: {}", from.display(), e))?;
    if fs::read(to).ok().as_deref() == Some(wanted.as_slice()) {
        return Ok(false);
    }
    fs::write(to, &wanted).map_err(|e| format!("Couldn't write {}: {}", to.display(), e))?;
    Ok(true)
}

/// Takes away a shim left beside a game whose BepInEx it can't start: Doorstop
/// would run the shim instead of BepInEx, and our Doorstop 4 loader can't
/// start an older BepInEx either (Repair then never gets the runtime healthy).
/// The pack's own loaders in doorstop_libs take over again. Returns whether
/// a shim was there.
pub(crate) fn take_away_unusable_native_apple_silicon(
    runtime_root: &std::path::Path,
    core_dir: &std::path::Path,
    resources: Option<&std::path::Path>,
) -> Result<bool, String> {
    let shim_dir = runtime_root.join(APPLE_SILICON_SHIM_DIR);
    if bepinex_can_use_shim(core_dir).is_ok() || !shim_dir.join(SHIM_FILES[0]).is_file() {
        return Ok(false);
    }
    fs::remove_dir_all(&shim_dir)
        .map_err(|e| format!("Couldn't remove {}: {}", shim_dir.display(), e))?;
    let loader = runtime_root.join(LOADER);
    let ours = resources.and_then(|dir| fs::read(dir.join(LOADER)).ok());
    if ours.is_some() && fs::read(&loader).ok() == ours {
        fs::remove_file(&loader)
            .map_err(|e| format!("Couldn't remove {}: {}", loader.display(), e))?;
    }
    Ok(true)
}

pub(crate) fn prepare_native_apple_silicon(
    target: &NativeAppleSiliconTarget<'_>,
) -> Result<NativeAppleSilicon, String> {
    let shim_dir = target.runtime_root.join(APPLE_SILICON_SHIM_DIR);

    if !target.host_is_apple_silicon {
        return Ok(NativeAppleSilicon::NotApplicable(
            "this Mac isn't Apple Silicon",
        ));
    }
    if !target.game_has_arm64 {
        return Ok(NativeAppleSilicon::NotApplicable(
            "the game has no arm64 build",
        ));
    }
    if let Err(reason) = bepinex_can_use_shim(target.core_dir) {
        if take_away_unusable_native_apple_silicon(
            target.runtime_root,
            target.core_dir,
            target.resources,
        )? {
            return Ok(NativeAppleSilicon::Removed);
        }
        return Ok(NativeAppleSilicon::NotApplicable(reason));
    }
    let Some(resources) = target.resources.filter(|dir| {
        SHIM_FILES
            .iter()
            .chain([&LOADER])
            .all(|name| dir.join(name).is_file())
    }) else {
        return Ok(NativeAppleSilicon::NotApplicable(
            "this build of r2modmac doesn't carry the Apple Silicon files",
        ));
    };

    let first_install = !shim_dir.exists();
    fs::create_dir_all(&shim_dir)
        .map_err(|e| format!("Couldn't create {}: {}", shim_dir.display(), e))?;
    let mut shim_updated = false;
    for name in SHIM_FILES {
        shim_updated |= copy_if_different(&resources.join(name), &shim_dir.join(name))?;
    }
    let mut changed = shim_updated;
    set_executable_if_present(&shim_dir.join("libjitwrite.dylib"))?;

    // Packs often ship only an x86_64 loader. Ours is universal; one that
    // already has arm64 is left as it is.
    let loader = target.runtime_root.join(LOADER);
    let loader_has_arm64 = fs::read(&loader)
        .map(|bytes| mach_o_bytes_have_arm64(&bytes))
        .unwrap_or(false);
    if !loader_has_arm64 {
        changed |= copy_if_different(&resources.join(LOADER), &loader)?;
    }

    // A Rosetta pin written before the shim existed came from the crash it
    // fixes, and one written with an older shim may be fixed by this one, so
    // both get a native try. Pins written with this shim are kept: it didn't
    // help there (the launcher pins a game whose native launch crashed).
    if first_install || shim_updated {
        remove_all_architecture_pins(target.runtime_root)?;
    }

    Ok(if changed {
        NativeAppleSilicon::Installed
    } else {
        NativeAppleSilicon::Unchanged
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const UNIVERSAL: [u8; 48] = {
        let mut b = [0u8; 48];
        // FAT_MAGIC, 2 slices: x86_64 (0x01000007) and arm64 (0x0100000c)
        b[0] = 0xca;
        b[1] = 0xfe;
        b[2] = 0xba;
        b[3] = 0xbe;
        b[7] = 2;
        b[8] = 0x01;
        b[11] = 0x07;
        b[28] = 0x01;
        b[31] = 0x0c;
        b
    };
    const X86_64_ONLY: [u8; 8] = [0xcf, 0xfa, 0xed, 0xfe, 0x07, 0x00, 0x00, 0x01];
    const ARM64_ONLY: [u8; 8] = [0xcf, 0xfa, 0xed, 0xfe, 0x0c, 0x00, 0x00, 0x01];

    struct World {
        root: std::path::PathBuf,
    }

    impl World {
        fn new(label: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "r2modmac-apple-silicon-{}-{}-{}",
                label,
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let resources = root.join("Resources/apple-silicon");
            fs::create_dir_all(&resources).unwrap();
            fs::write(resources.join("AppleSiliconMods.dll"), b"shim").unwrap();
            fs::write(resources.join("libjitwrite.dylib"), ARM64_ONLY).unwrap();
            fs::write(resources.join("AppleSiliconMods.Straftat.dll"), b"marker").unwrap();
            fs::write(resources.join("libdoorstop.dylib"), UNIVERSAL).unwrap();
            let core = root.join("Game/BepInEx/core");
            fs::create_dir_all(&core).unwrap();
            fs::write(
                core.join("BepInEx.Preloader.dll"),
                b"..get_DOORSTOP_INVOKE_DLL_PATH..Doorstop..Entrypoint..Start..",
            )
            .unwrap();
            fs::write(core.join("MonoMod.RuntimeDetour.dll"), b"monomod").unwrap();
            fs::create_dir_all(root.join("Game/doorstop_libs")).unwrap();
            fs::write(
                root.join("Game/doorstop_libs/libdoorstop_x64.dylib"),
                X86_64_ONLY,
            )
            .unwrap();
            World { root }
        }
        fn game(&self) -> std::path::PathBuf {
            self.root.join("Game")
        }
        fn core(&self) -> std::path::PathBuf {
            self.game().join("BepInEx/core")
        }
        fn resources(&self) -> std::path::PathBuf {
            self.root.join("Resources/apple-silicon")
        }
        fn prepare(&self, host: bool, arm64_game: bool, resources: bool) -> NativeAppleSilicon {
            let (game, core, res) = (self.game(), self.core(), self.resources());
            prepare_native_apple_silicon(&NativeAppleSiliconTarget {
                resources: resources.then_some(res.as_path()),
                runtime_root: &game,
                core_dir: &core,
                host_is_apple_silicon: host,
                game_has_arm64: arm64_game,
            })
            .unwrap()
        }
    }

    impl Drop for World {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn reads_arm64_slices_from_thin_and_universal_mach_o() {
        assert!(mach_o_bytes_have_arm64(&UNIVERSAL));
        assert!(mach_o_bytes_have_arm64(&ARM64_ONLY));
        assert!(!mach_o_bytes_have_arm64(&X86_64_ONLY));
        assert!(!mach_o_bytes_have_arm64(b"MZ not a mach-o"));
        assert!(!mach_o_bytes_have_arm64(&[]));
    }

    #[test]
    fn reads_x86_64_slices_the_same_way() {
        assert!(mach_o_bytes_have_arch(&UNIVERSAL, CPU_TYPE_X86_64));
        assert!(mach_o_bytes_have_arch(&X86_64_ONLY, CPU_TYPE_X86_64));
        assert!(!mach_o_bytes_have_arch(&ARM64_ONLY, CPU_TYPE_X86_64));
        assert!(!mach_o_bytes_have_arch(b"MZ not a mach-o", CPU_TYPE_X86_64));
        assert!(!mach_o_bytes_have_arch(&[], CPU_TYPE_X86_64));
    }

    #[test]
    fn puts_the_shim_and_a_universal_loader_beside_a_bepinex_5_game() {
        let world = World::new("install");
        assert_eq!(
            world.prepare(true, true, true),
            NativeAppleSilicon::Installed
        );
        assert_eq!(
            fs::read(world.game().join("apple_silicon/AppleSiliconMods.dll")).unwrap(),
            b"shim"
        );
        assert!(world
            .game()
            .join("apple_silicon/libjitwrite.dylib")
            .is_file());
        assert_eq!(
            fs::read(world.game().join("libdoorstop.dylib")).unwrap(),
            UNIVERSAL
        );
        // The pack's own x86_64 loader is left alone.
        assert_eq!(
            fs::read(world.game().join("doorstop_libs/libdoorstop_x64.dylib")).unwrap(),
            X86_64_ONLY
        );

        assert_eq!(
            world.prepare(true, true, true),
            NativeAppleSilicon::Unchanged
        );
    }

    #[test]
    fn keeps_a_loader_that_already_has_arm64_and_replaces_an_x86_64_one() {
        let world = World::new("loader");
        fs::write(world.game().join("libdoorstop.dylib"), ARM64_ONLY).unwrap();
        world.prepare(true, true, true);
        assert_eq!(
            fs::read(world.game().join("libdoorstop.dylib")).unwrap(),
            ARM64_ONLY
        );

        let world = World::new("loader-x64");
        fs::write(world.game().join("libdoorstop.dylib"), X86_64_ONLY).unwrap();
        world.prepare(true, true, true);
        assert_eq!(
            fs::read(world.game().join("libdoorstop.dylib")).unwrap(),
            UNIVERSAL
        );
    }

    #[test]
    fn clears_a_rosetta_pin_only_when_the_shim_is_new_or_updated() {
        let world = World::new("pin");
        let pin = world.game().join(".r2modmac_force_x64_state");
        fs::write(&pin, "Game.app|1").unwrap();
        world.prepare(true, true, true);
        assert!(
            !pin.exists(),
            "a pin from before the shim came from the crash it fixes"
        );

        fs::write(&pin, "Game.app|1").unwrap();
        world.prepare(true, true, true);
        assert!(
            pin.exists(),
            "a pin written with the shim in place means native didn't work"
        );

        fs::write(
            world.resources().join("AppleSiliconMods.dll"),
            b"newer shim",
        )
        .unwrap();
        assert_eq!(
            world.prepare(true, true, true),
            NativeAppleSilicon::Installed
        );
        assert!(!pin.exists(), "a newer shim gets a native try again");
    }

    #[test]
    fn a_new_shim_clears_every_profiles_pin_too() {
        let world = World::new("profile-pins");
        let whole_game = world.game().join(".r2modmac_force_x64_state");
        let one = world.game().join(".r2modmac_force_x64_state.7e774110-c505");
        let two = world.game().join(".r2modmac_force_x64_state.03b85b8d-9cbd");
        let unrelated = world.game().join(".r2modmac_force_x64_state_notes.txt");
        for file in [&whole_game, &one, &two, &unrelated] {
            fs::write(file, "Game.app|1").unwrap();
        }

        world.prepare(true, true, true);

        for pin in [&whole_game, &one, &two] {
            assert!(!pin.exists(), "{} is a pin and goes", pin.display());
        }
        assert!(unrelated.exists(), "only pins go: {}", unrelated.display());

        // With the same shim in place, pins written since are kept.
        fs::write(&one, "Game.app|1").unwrap();
        world.prepare(true, true, true);
        assert!(one.exists());
    }

    #[test]
    fn changes_nothing_where_it_cant_help() {
        let world = World::new("skip");
        assert_eq!(
            world.prepare(false, true, true),
            NativeAppleSilicon::NotApplicable("this Mac isn't Apple Silicon")
        );
        assert_eq!(
            world.prepare(true, false, true),
            NativeAppleSilicon::NotApplicable("the game has no arm64 build")
        );
        assert_eq!(
            world.prepare(true, true, false),
            NativeAppleSilicon::NotApplicable(
                "this build of r2modmac doesn't carry the Apple Silicon files"
            )
        );
        assert!(!world.game().join("apple_silicon").exists());
        assert!(!world.game().join("libdoorstop.dylib").exists());
    }

    #[test]
    fn leaves_other_mod_loaders_alone() {
        let world = World::new("bepinex6");
        fs::remove_file(world.core().join("BepInEx.Preloader.dll")).unwrap();
        fs::write(
            world.core().join("BepInEx.Unity.Mono.Preloader.dll"),
            b"bepinex 6",
        )
        .unwrap();
        assert_eq!(
            world.prepare(true, true, true),
            NativeAppleSilicon::NotApplicable("the game doesn't use BepInEx 5")
        );

        let world = World::new("old-bepinex");
        fs::write(
            world.core().join("BepInEx.Preloader.dll"),
            b"5.4.21..get_DOORSTOP_INVOKE_DLL_PATH..Main..",
        )
        .unwrap();
        assert_eq!(
            world.prepare(true, true, true),
            NativeAppleSilicon::NotApplicable("its BepInEx is older than 5.4.22")
        );
    }

    #[test]
    fn takes_a_shim_away_when_the_game_moves_to_a_bepinex_it_cant_start() {
        let world = World::new("remove");
        world.prepare(true, true, true);
        fs::write(
            world.core().join("BepInEx.Preloader.dll"),
            b"5.4.21..get_DOORSTOP_INVOKE_DLL_PATH..Main..",
        )
        .unwrap();
        assert_eq!(world.prepare(true, true, true), NativeAppleSilicon::Removed);
        assert!(!world.game().join("apple_silicon").exists());
        // Our Doorstop 4 loader goes too: the older BepInEx needs the pack's own.
        assert!(!world.game().join("libdoorstop.dylib").exists());
        assert_eq!(
            fs::read(world.game().join("doorstop_libs/libdoorstop_x64.dylib")).unwrap(),
            X86_64_ONLY
        );
    }

    #[test]
    fn keeps_a_loader_that_isnt_ours_when_taking_the_shim_away() {
        let world = World::new("remove-foreign-loader");
        world.prepare(true, true, true);
        fs::write(world.game().join("libdoorstop.dylib"), ARM64_ONLY).unwrap();
        fs::write(
            world.core().join("BepInEx.Preloader.dll"),
            b"5.4.21..get_DOORSTOP_INVOKE_DLL_PATH..Main..",
        )
        .unwrap();
        assert_eq!(world.prepare(true, true, true), NativeAppleSilicon::Removed);
        assert_eq!(
            fs::read(world.game().join("libdoorstop.dylib")).unwrap(),
            ARM64_ONLY
        );
    }
}
