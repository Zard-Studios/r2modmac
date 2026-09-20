/// Where the script tells Doorstop to find BepInEx.
///
/// BepInEx roots itself two directories above the preloader it is handed, so
/// pointing this at a profile moves plugins, config and patchers there with it.
pub(crate) enum BepInExRoot<'a> {
    /// `$BASEDIR/BepInEx`, the tree beside the game executable.
    Game,
    /// An absolute path, for a profile that keeps its own tree.
    Profile(&'a str),
}

impl BepInExRoot<'_> {
    fn as_shell_path(&self) -> &str {
        match self {
            BepInExRoot::Game => "$BASEDIR/BepInEx",
            BepInExRoot::Profile(path) => path,
        }
    }
}

pub(crate) fn build_generated_macos_bepinex_script(
    relative_exec: &str,
    relative_launch_entry: &str,
    launch_entry_uses_wrapper: bool,
    write_debug_logs_to_game: bool,
    bepinex_root: BepInExRoot<'_>,
) -> String {
    let write_debug_logs = if write_debug_logs_to_game { "1" } else { "0" };
    let launch_entry_uses_wrapper = if launch_entry_uses_wrapper { "1" } else { "0" };
    // Shell templates use doubled braces (`{{` / `}}`) to keep raw files readable
    // without `format!`. Convert them back to valid shell syntax here.
    [
        include_str!("script_template_prelude.sh"),
        include_str!("script_template_runtime_disabled.sh"),
        include_str!("script_template_modded.sh"),
    ]
    .join("")
    .replace("__RELATIVE_EXEC__", relative_exec)
    .replace("__RELATIVE_LAUNCH_ENTRY__", relative_launch_entry)
    .replace("__LAUNCH_ENTRY_USES_WRAPPER__", launch_entry_uses_wrapper)
    .replace("__WRITE_DEBUG_LOGS__", write_debug_logs)
    .replace("__R2MODMAC_VERSION__", env!("CARGO_PKG_VERSION"))
    .replace("__BEPINEX_ROOT__", bepinex_root.as_shell_path())
    .replace("${{", "${")
    .replace("{{", "{")
    .replace("}}", "}")
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_script_unescapes_template_braces() {
        let script = build_generated_macos_bepinex_script(
            "Example.app/Contents/MacOS/Example",
            "Example.app/Contents/MacOS/Example",
            false,
            true,
            BepInExRoot::Game,
        );

        assert!(
            !script.contains("${{"),
            "generated script still contains escaped variable braces"
        );
        assert!(
            !script.contains("() {{"),
            "generated script still contains escaped function braces"
        );
    }

    #[test]
    fn generated_script_uses_cross_generation_doorstop_flags() {
        let script = build_generated_macos_bepinex_script(
            "Example.app/Contents/MacOS/Example",
            "Example.app/Contents/MacOS/Example",
            false,
            true,
            BepInExRoot::Game,
        );

        assert!(script.contains("DOORSTOP_ENABLE=TRUE"));
        assert!(script.contains("DOORSTOP_ENABLED=1"));
        assert!(!script.contains("DOORSTOP_ENABLE=1"));
        assert!(!script.contains("DOORSTOP_ENABLED=TRUE"));
    }

    #[test]
    fn generated_script_skips_x64_retry_after_clean_arm64_exit() {
        let script = build_generated_macos_bepinex_script(
            "Example.app/Contents/MacOS/Example",
            "Example.app/Contents/MacOS/Example",
            false,
            true,
            BepInExRoot::Game,
        );

        assert!(script.contains("if [ \"$failed_status\" = \"0\" ]"));
        assert!(script.contains("retry_skipped_clean_exit"));
    }

    #[test]
    fn generated_script_includes_codesign_cache_guard() {
        let script = build_generated_macos_bepinex_script(
            "Example.app/Contents/MacOS/Example",
            "Example.app/Contents/MacOS/Example",
            false,
            true,
            BepInExRoot::Game,
        );

        assert!(script.contains(".r2modmac_codesign_state"));
        assert!(script.contains("codesign_adhoc_sign_skipped_cached"));
        assert!(script.contains("codesign_state_key"));
    }

    #[test]
    fn generated_script_does_not_label_clean_game_exit_as_failure() {
        let script = build_generated_macos_bepinex_script(
            "Example.app/Contents/MacOS/Example",
            "Example.app/Contents/MacOS/Example",
            false,
            true,
            BepInExRoot::Game,
        );

        assert!(script.contains("if [ \"$exec_result_status\" = \"0\" ]"));
        assert!(script.contains("${exec_mode}_completed status=0"));
        assert!(script.contains("${exec_mode}_failed status=${exec_result_status}"));
        assert!(!script.contains("exec_modded_arch_env_failed status=$exec_status"));
    }

    #[test]
    fn generated_debug_log_has_a_schema_version_context_and_no_raw_argv() {
        let script = build_generated_macos_bepinex_script(
            "Example.app/Contents/MacOS/Example",
            "Example.app/Contents/MacOS/Example",
            false,
            true,
            BepInExRoot::Game,
        );

        assert!(script.contains("log_schema=3"));
        assert!(script.contains(&format!(
            "r2modmac_version=\"{}\"",
            env!("CARGO_PKG_VERSION")
        )));
        assert!(script.contains("timestamp\\tlevel\\tlaunch_id\\tcomponent\\tmessage"));
        assert!(script.contains("session_start schema=$log_schema"));
        assert!(script.contains("sanitize_bootstrap_message()"));
        assert!(script.contains("redact_log_literal \"$HOME\" \"[home]\""));
        assert!(script.contains("redact_log_literal \"$USER\" \"[user]\""));
        assert!(!script.contains("argv=$*"));
    }

    #[test]
    fn generated_debug_launcher_keeps_the_packs_doorstop_loader() {
        let script = build_generated_macos_bepinex_script(
            "Example.app/Contents/MacOS/Example",
            "Example.app/Contents/MacOS/Example",
            false,
            true,
            BepInExRoot::Game,
        );

        assert!(script.contains("root_doorstop_dylib=\"$BASEDIR/libdoorstop.dylib\""));
        assert!(!script.contains("libdoorstop_r2modmac_verbose.dylib"));
        assert!(script.contains("export DYLD_INSERT_LIBRARIES=\"${doorstop_dylib}\""));
    }

    #[test]
    #[cfg(unix)]
    fn generated_launcher_remains_valid_shell_after_log_redaction_is_inserted() {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let script = build_generated_macos_bepinex_script(
            "Example.app/Contents/MacOS/Example",
            "Example.app/Contents/MacOS/Example",
            false,
            true,
            BepInExRoot::Game,
        );
        let mut child = Command::new("/bin/sh")
            .arg("-n")
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(script.as_bytes())
            .unwrap();

        assert!(child.wait().unwrap().success());
    }
}

#[cfg(test)]
mod bepinex_root_tests {
    use super::*;

    fn script(root: BepInExRoot<'_>) -> String {
        build_generated_macos_bepinex_script(
            "Example.app/Contents/MacOS/Example",
            "Example.app/Contents/MacOS/Example",
            false,
            false,
            root,
        )
    }

    #[test]
    fn the_game_root_is_what_the_script_has_always_written() {
        let generated = script(BepInExRoot::Game);
        assert!(generated.contains(
            r#"export DOORSTOP_INVOKE_DLL_PATH="$BASEDIR/BepInEx/core/BepInEx.Preloader.dll""#
        ));
        assert!(generated
            .contains(r#"export DOORSTOP_MONO_DLL_SEARCH_PATH_OVERRIDE="$BASEDIR/BepInEx/core""#));
        assert!(generated.contains(r#"BEPINEX_LOG_PATH="$BASEDIR/BepInEx/LogOutput.log""#));
    }

    /// Measured on a real game: BepInEx wrote its log and loaded its plugins
    /// from the directory two levels above the preloader it was handed, leaving
    /// the tree in the game folder untouched.
    #[test]
    fn a_profile_root_moves_every_doorstop_path_with_it() {
        let profile = "/Users/x/Library/Application Support/com.r2modmac/profiles/abc/BepInEx";
        let generated = script(BepInExRoot::Profile(profile));

        assert!(generated.contains(&format!(
            r#"export DOORSTOP_INVOKE_DLL_PATH="{profile}/core/BepInEx.Preloader.dll""#
        )));
        assert!(generated.contains(&format!(
            r#"export DOORSTOP_MONO_DLL_SEARCH_PATH_OVERRIDE="{profile}/core""#
        )));
        assert!(generated.contains(&format!(r#"BEPINEX_LOG_PATH="{profile}/LogOutput.log""#)));

        // The loader itself stays beside the game: the game is what loads it.
        assert!(generated.contains("$BASEDIR/libdoorstop.dylib"));
        assert!(generated.contains("$BASEDIR/doorstop_libs"));
    }

    #[test]
    fn no_placeholder_survives_into_the_written_script() {
        for generated in [
            script(BepInExRoot::Game),
            script(BepInExRoot::Profile("/tmp/p")),
        ] {
            assert!(!generated.contains("__BEPINEX_ROOT__"));
            assert!(!generated.contains("__RELATIVE_EXEC__"));
        }
    }
}
