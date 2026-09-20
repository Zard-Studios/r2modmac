pub(crate) fn find_next_non_whitespace(text: &str, mut index: usize, end: usize) -> Option<usize> {
    while index < end {
        match text.as_bytes()[index] {
            b' ' | b'\t' | b'\r' | b'\n' => index += 1,
            _ => return Some(index),
        }
    }
    None
}

fn find_ascii_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    let needle = needle.as_bytes();
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .as_bytes()
        .windows(needle.len())
        .position(|candidate| candidate.eq_ignore_ascii_case(needle))
}

pub(crate) fn find_matching_brace(text: &str, open_index: usize, end: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut index = open_index;
    let mut in_string = false;
    let mut escaped = false;

    while index < end {
        let byte = bytes[index];
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            index += 1;
            continue;
        }

        match byte {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
        index += 1;
    }

    None
}

pub(crate) fn find_block_by_key(
    text: &str,
    key: &str,
    start: usize,
    end: usize,
) -> Option<(usize, usize, usize, String)> {
    let pattern = format!("\"{}\"", key);
    let mut search_start = start;

    while search_start < end {
        let relative_match = find_ascii_case_insensitive(&text[search_start..end], &pattern)?;
        let key_index = search_start + relative_match;
        let line_start = text[..key_index]
            .rfind('\n')
            .map(|idx| idx + 1)
            .unwrap_or(0);
        let indentation = text[line_start..key_index].to_string();

        let block_search_start = key_index + pattern.len();
        let brace_index = find_next_non_whitespace(text, block_search_start, end)?;
        if text.as_bytes()[brace_index] == b'{' {
            let close_index = find_matching_brace(text, brace_index, end)?;
            return Some((key_index, brace_index, close_index, indentation));
        }

        search_start = key_index + pattern.len();
    }

    None
}

pub(crate) fn find_all_blocks_by_key(
    text: &str,
    key: &str,
    start: usize,
    end: usize,
) -> Vec<(usize, usize, usize, String)> {
    let pattern = format!("\"{}\"", key);
    let mut search_start = start;
    let mut matches = Vec::new();

    while search_start < end {
        let Some(relative_match) = find_ascii_case_insensitive(&text[search_start..end], &pattern)
        else {
            break;
        };
        let key_index = search_start + relative_match;
        let line_start = text[..key_index]
            .rfind('\n')
            .map(|idx| idx + 1)
            .unwrap_or(0);
        let indentation = text[line_start..key_index].to_string();

        let block_search_start = key_index + pattern.len();
        if let Some(brace_index) = find_next_non_whitespace(text, block_search_start, end) {
            if text.as_bytes()[brace_index] == b'{' {
                if let Some(close_index) = find_matching_brace(text, brace_index, end) {
                    matches.push((key_index, brace_index, close_index, indentation));
                    search_start = close_index + 1;
                    continue;
                }
            }
        }

        search_start = key_index + pattern.len();
    }

    matches
}

pub(crate) fn escape_vdf_value(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

pub(crate) fn unescape_vdf_value(value: &str) -> String {
    value.replace("\\\"", "\"").replace("\\\\", "\\")
}

pub(crate) fn extract_launch_options_from_app_block(block: &str) -> Option<String> {
    let re =
        regex::Regex::new(r#"(?mi)^[ \t]*"LaunchOptions"[ \t]*"((?:\\.|[^"])*)"[ \t]*$"#).ok()?;
    re.captures(block).and_then(|captures| {
        captures
            .get(1)
            .map(|value| unescape_vdf_value(value.as_str()))
    })
}

pub(crate) fn find_steam_apps_block(
    text: &str,
    app_id: Option<&str>,
) -> Option<(usize, usize, usize, String)> {
    // LaunchOptions belong to the historical
    // UserLocalConfigStore/Software/Valve/Steam/Apps branch. Newer files can
    // also contain root-level or controller `apps` blocks; those are not
    // interchangeable and Steam may ignore LaunchOptions written there.
    // Prefer the Steam branch whenever it exists, and use the global search
    // only for files that genuinely omit that hierarchy.
    let steam_candidates = find_block_by_key(text, "Software", 0, text.len())
        .and_then(|(_, software_open, software_close, _)| {
            find_block_by_key(text, "Valve", software_open + 1, software_close)
        })
        .and_then(|(_, valve_open, valve_close, _)| {
            find_block_by_key(text, "Steam", valve_open + 1, valve_close)
        })
        .map(|(_, steam_open, steam_close, _)| {
            find_all_blocks_by_key(text, "apps", steam_open + 1, steam_close)
        })
        .unwrap_or_default();
    let candidates = if steam_candidates.is_empty() {
        find_all_blocks_by_key(text, "apps", 0, text.len())
    } else {
        steam_candidates
    };
    if candidates.is_empty() {
        return None;
    }

    let mut scored = candidates
        .into_iter()
        .map(|candidate| {
            let (_, apps_open, apps_close, _) = &candidate;
            let content = &text[*apps_open + 1..*apps_close];
            let mut score = 0i32;

            if let Some(app_id) = app_id {
                if let Some((_, app_open, app_close, _)) =
                    find_block_by_key(text, app_id, *apps_open + 1, *apps_close)
                {
                    score += 2000;
                    let app_content = &text[app_open + 1..app_close];
                    if app_content.contains("\"LastPlayed\"")
                        || app_content.contains("\"Playtime\"")
                        || app_content.contains("\"BadgeData\"")
                        || app_content.contains("\"cloud\"")
                        || app_content.contains("\"autocloud\"")
                        || app_content.contains("\"LaunchOptions\"")
                    {
                        score += 500;
                    }
                    if app_content.contains("\"UseSteamControllerConfig\"")
                        || app_content.contains("\"SteamControllerRumble\"")
                    {
                        score -= 1000;
                    }
                } else {
                    score -= 1000;
                }
            }

            if content.contains("\"LastPlayed\"") || content.contains("\"Playtime\"") {
                score += 100;
            }
            if content.contains("\"LaunchOptions\"") {
                score += 50;
            }
            if content.contains("\"UseSteamControllerConfig\"") {
                score -= 250;
            }
            if content.contains("\"SteamControllerRumble\"")
                || content.contains("\"SteamControllerRumbleIntensity\"")
            {
                score -= 250;
            }

            (score, candidate)
        })
        .collect::<Vec<_>>();

    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.into_iter().map(|(_, candidate)| candidate).next()
}

pub(crate) fn get_launch_options_for_app(text: &str, app_id: &str) -> Option<String> {
    let (_, apps_open, apps_close, _) = find_steam_apps_block(text, Some(app_id))?;
    let (_, app_open, app_close, _) = find_block_by_key(text, app_id, apps_open + 1, apps_close)?;
    extract_launch_options_from_app_block(&text[app_open + 1..app_close])
}

pub(crate) fn update_launch_options_in_localconfig(
    text: &str,
    app_id: &str,
    desired: Option<&str>,
) -> Result<(String, Option<String>), String> {
    let (_, apps_open, apps_close, apps_indent) = find_steam_apps_block(text, Some(app_id))
        .ok_or_else(|| "Steam localconfig.vdf does not contain an apps block".to_string())?;

    let app_block = find_block_by_key(text, app_id, apps_open + 1, apps_close);
    let launch_options_re =
        regex::Regex::new(r#"(?mi)^[ \t]*"LaunchOptions"[ \t]*"((?:\\.|[^"])*)"[ \t]*\r?\n?"#)
            .map_err(|e| format!("Invalid launch options regex: {}", e))?;

    if let Some((_, app_open, app_close, app_indent)) = app_block {
        let block_content = &text[app_open + 1..app_close];
        let current = extract_launch_options_from_app_block(block_content);

        let property_indent = format!("{}\t", app_indent);
        let mut updated_block = if let Some(value) = desired {
            let replacement_line = format!(
                "{}\"LaunchOptions\"\t\t\"{}\"\n",
                property_indent,
                escape_vdf_value(value)
            );
            if launch_options_re.is_match(block_content) {
                launch_options_re
                    .replace(block_content, replacement_line.as_str())
                    .to_string()
            } else {
                let mut block = block_content.to_string();
                if !block.ends_with('\n') {
                    block.push('\n');
                }
                block.push_str(&replacement_line);
                block
            }
        } else {
            launch_options_re.replace(block_content, "").to_string()
        };

        if !updated_block.ends_with('\n') {
            updated_block.push('\n');
        }

        let updated_text = format!(
            "{}{}{}",
            &text[..app_open + 1],
            updated_block,
            &text[app_close..]
        );

        return Ok((updated_text, current));
    }

    if let Some(value) = desired {
        let app_indent = format!("{}\t", apps_indent);
        let property_indent = format!("{}\t", app_indent);
        let insertion = format!(
            "\n{}\"{}\"\n{}{{\n{}\"LaunchOptions\"\t\t\"{}\"\n{}}}\n",
            app_indent,
            app_id,
            app_indent,
            property_indent,
            escape_vdf_value(value),
            app_indent
        );
        let updated_text = format!(
            "{}{}{}",
            &text[..apps_close],
            insertion,
            &text[apps_close..]
        );
        return Ok((updated_text, None));
    }

    Ok((text.to_string(), None))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DESIRED: &str = "/bin/bash \"/tmp/My Game/run_bepinex.sh\" %command%";

    fn count_launch_options(text: &str) -> usize {
        text.to_ascii_lowercase()
            .matches("\"launchoptions\"")
            .count()
    }

    #[test]
    fn launch_options_accept_mixed_case_steam_vdf_keys() {
        let localconfig = r#""UserLocalConfigStore"
{
    "software"
    {
        "VALVE"
        {
            "steam"
            {
                "Apps"
                {
                    "42"
                    {
                        "LastPlayed" "1"
                    }
                }
            }
        }
    }
}
"#;

        let (updated, previous) =
            update_launch_options_in_localconfig(localconfig, "42", Some(DESIRED)).unwrap();

        assert_eq!(previous, None);
        assert_eq!(
            get_launch_options_for_app(&updated, "42").as_deref(),
            Some(DESIRED)
        );
    }

    #[test]
    fn launch_options_accept_root_level_apps_block() {
        let localconfig = r#""UserLocalConfigStore"
{
    "apps"
    {
        "42"
        {
            "LastPlayed" "1"
        }
    }
}
"#;

        let (updated, previous) =
            update_launch_options_in_localconfig(localconfig, "42", Some(DESIRED)).unwrap();

        assert_eq!(previous, None);
        assert_eq!(
            get_launch_options_for_app(&updated, "42").as_deref(),
            Some(DESIRED)
        );
    }

    #[test]
    fn launch_options_prefer_steam_branch_over_root_apps_block() {
        let localconfig = r#""UserLocalConfigStore"
{
    "apps"
    {
        "42"
        {
            "LastPlayed" "1"
        }
    }
    "Software"
    {
        "valve"
        {
            "Steam"
            {
                "Apps"
                {
                    "42"
                    {
                        "cloud" "1"
                    }
                }
            }
        }
    }
}
"#;

        let (updated, _) =
            update_launch_options_in_localconfig(localconfig, "42", Some(DESIRED)).unwrap();
        let all_apps = find_all_blocks_by_key(&updated, "apps", 0, updated.len());
        let (_, root_open, root_close, _) = &all_apps[0];
        let (_, steam_open, steam_close, _) = &all_apps[1];

        assert!(!updated[*root_open + 1..*root_close].contains("\"LaunchOptions\""));
        assert!(updated[*steam_open + 1..*steam_close].contains("\"LaunchOptions\""));
        assert_eq!(
            get_launch_options_for_app(&updated, "42").as_deref(),
            Some(DESIRED)
        );
    }

    #[test]
    fn launch_options_replace_differently_cased_existing_property_without_duplication() {
        let localconfig = r#""UserLocalConfigStore"
{
    "Apps"
    {
        "42"
        {
            "launchoptions" "--old"
        }
    }
}
"#;

        let (updated, previous) =
            update_launch_options_in_localconfig(localconfig, "42", Some(DESIRED)).unwrap();

        assert_eq!(previous.as_deref(), Some("--old"));
        assert_eq!(
            get_launch_options_for_app(&updated, "42").as_deref(),
            Some(DESIRED)
        );
        assert_eq!(count_launch_options(&updated), 1);
    }

    #[test]
    fn launch_options_choose_game_apps_block_over_controller_block() {
        let localconfig = r#""UserLocalConfigStore"
{
    "controller_config"
    {
        "apps"
        {
            "42"
            {
                "UseSteamControllerConfig" "1"
            }
        }
    }
    "apps"
    {
        "42"
        {
            "LastPlayed" "1"
        }
    }
}
"#;

        let (updated, _) =
            update_launch_options_in_localconfig(localconfig, "42", Some(DESIRED)).unwrap();
        let apps_blocks = find_all_blocks_by_key(&updated, "apps", 0, updated.len());
        let (_, controller_open, controller_close, _) = &apps_blocks[0];
        let (_, game_open, game_close, _) = &apps_blocks[1];

        assert!(!updated[*controller_open + 1..*controller_close].contains("\"LaunchOptions\""));
        assert!(updated[*game_open + 1..*game_close].contains("\"LaunchOptions\""));
        assert_eq!(
            get_launch_options_for_app(&updated, "42").as_deref(),
            Some(DESIRED)
        );
    }

    #[test]
    fn launch_options_insert_missing_app_into_game_apps_block() {
        let localconfig = r#""UserLocalConfigStore"
{
    "controller_config"
    {
        "apps"
        {
            "7"
            {
                "SteamControllerRumble" "1"
            }
        }
    }
    "apps"
    {
        "8"
        {
            "Playtime" "10"
        }
    }
}
"#;

        let (updated, previous) =
            update_launch_options_in_localconfig(localconfig, "42", Some(DESIRED)).unwrap();

        assert_eq!(previous, None);
        assert_eq!(
            get_launch_options_for_app(&updated, "42").as_deref(),
            Some(DESIRED)
        );
        let apps_blocks = find_all_blocks_by_key(&updated, "apps", 0, updated.len());
        let (_, controller_open, controller_close, _) = &apps_blocks[0];
        let (_, game_open, game_close, _) = &apps_blocks[1];
        assert!(!updated[*controller_open + 1..*controller_close].contains("\"42\""));
        assert!(updated[*game_open + 1..*game_close].contains("\"42\""));
    }

    #[test]
    fn launch_options_clear_only_the_target_property() {
        let localconfig = r#""UserLocalConfigStore"
{
    "apps"
    {
        "42"
        {
            "LaunchOptions" "--old"
            "LastPlayed" "1"
        }
        "43"
        {
            "LaunchOptions" "--keep"
        }
    }
}
"#;

        let (updated, previous) =
            update_launch_options_in_localconfig(localconfig, "42", None).unwrap();

        assert_eq!(previous.as_deref(), Some("--old"));
        assert_eq!(get_launch_options_for_app(&updated, "42"), None);
        assert_eq!(
            get_launch_options_for_app(&updated, "43").as_deref(),
            Some("--keep")
        );
        assert!(updated.contains("\"LastPlayed\" \"1\""));
    }

    #[test]
    fn launch_options_round_trip_escaped_path() {
        let localconfig = "\"UserLocalConfigStore\"\n{\n\t\"apps\"\n\t{\n\t}\n}\n";
        let desired = r#"/bin/bash "/tmp/A \\ B/run_bepinex.sh" %command%"#;

        let (updated, _) =
            update_launch_options_in_localconfig(localconfig, "42", Some(desired)).unwrap();

        assert_eq!(
            get_launch_options_for_app(&updated, "42").as_deref(),
            Some(desired)
        );
    }

    #[test]
    fn launch_options_reject_config_without_any_apps_block() {
        let localconfig = r#""UserLocalConfigStore"
{
    "system"
    {
        "EnableGameOverlay" "1"
    }
}
"#;

        let error =
            update_launch_options_in_localconfig(localconfig, "42", Some(DESIRED)).unwrap_err();

        assert_eq!(
            error,
            "Steam localconfig.vdf does not contain an apps block"
        );
    }
}
