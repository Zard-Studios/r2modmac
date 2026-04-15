pub(crate) fn build_generated_macos_bepinex_script(
    relative_exec: &str,
    relative_launch_entry: &str,
    launch_entry_uses_wrapper: bool,
    write_debug_logs_to_game: bool,
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
        );

        assert!(script.contains(".r2modmac_codesign_state"));
        assert!(script.contains("codesign_adhoc_sign_skipped_cached"));
        assert!(script.contains("codesign_state_key"));
    }
}
