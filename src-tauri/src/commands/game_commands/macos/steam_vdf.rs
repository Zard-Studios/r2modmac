pub(crate) fn find_next_non_whitespace(text: &str, mut index: usize, end: usize) -> Option<usize> {
    while index < end {
        match text.as_bytes()[index] {
            b' ' | b'\t' | b'\r' | b'\n' => index += 1,
            _ => return Some(index),
        }
    }
    None
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
        let relative_match = text[search_start..end].find(&pattern)?;
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
        let Some(relative_match) = text[search_start..end].find(&pattern) else {
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
        regex::Regex::new(r#"(?m)^[ \t]*"LaunchOptions"[ \t]*"((?:\\.|[^"])*)"[ \t]*$"#).ok()?;
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
    let (_, software_open, software_close, _) = find_block_by_key(text, "Software", 0, text.len())?;
    let (_, valve_open, valve_close, _) =
        find_block_by_key(text, "Valve", software_open + 1, software_close)?;
    let (_, steam_open, steam_close, _) =
        find_block_by_key(text, "Steam", valve_open + 1, valve_close)?;

    let candidates = find_all_blocks_by_key(text, "apps", steam_open + 1, steam_close);
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
        regex::Regex::new(r#"(?m)^[ \t]*"LaunchOptions"[ \t]*"((?:\\.|[^"])*)"[ \t]*\r?\n?"#)
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
