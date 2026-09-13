use regex::Regex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

static ENABLED: AtomicBool = AtomicBool::new(false);
static PATH_USERNAME: OnceLock<Regex> = OnceLock::new();
static USERNAME_PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();
static STEAM_USERDATA: OnceLock<Regex> = OnceLock::new();
static STEAM_ID64: OnceLock<Regex> = OnceLock::new();
static EMAIL_ADDRESS: OnceLock<Regex> = OnceLock::new();
static SECRET_VALUE: OnceLock<Regex> = OnceLock::new();

fn path_username() -> &'static Regex {
    PATH_USERNAME.get_or_init(|| {
        Regex::new(r"(?i)(?P<prefix>(?:[a-z]:)?[\\/](?:users|home|documents and settings)[\\/])(?P<username>[^\\/:\r\n]+)")
            .expect("the built-in username path expression is valid")
    })
}

fn username_patterns() -> &'static [Regex] {
    USERNAME_PATTERNS
        .get_or_init(|| {
            let mut names = ["USER", "USERNAME", "LOGNAME"]
                .into_iter()
                .filter_map(|name| std::env::var(name).ok())
                .map(|name| name.trim().to_string())
                .filter(|name| !name.is_empty())
                .collect::<Vec<_>>();
            if let Some(home_username) = dirs::home_dir()
                .and_then(|home| home.file_name().map(|name| name.to_owned()))
                .and_then(|name| name.to_str().map(str::to_owned))
                .filter(|name| !name.trim().is_empty())
            {
                names.push(home_username);
            }
            names.sort_unstable();
            names.dedup();
            names
                .into_iter()
                .map(|name| Regex::new(&format!(r"(?i){}", regex::escape(&name))))
                .filter_map(Result::ok)
                .collect()
        })
        .as_slice()
}

fn steam_userdata() -> &'static Regex {
    STEAM_USERDATA.get_or_init(|| {
        Regex::new(r"(?i)(?P<prefix>[\\/]userdata[\\/])\d+")
            .expect("the built-in Steam userdata expression is valid")
    })
}

fn steam_id64() -> &'static Regex {
    STEAM_ID64.get_or_init(|| {
        Regex::new(r"\b7656119\d{10}\b").expect("the built-in SteamID expression is valid")
    })
}

fn email_address() -> &'static Regex {
    EMAIL_ADDRESS.get_or_init(|| {
        Regex::new(r"(?i)\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b")
            .expect("the built-in email expression is valid")
    })
}

fn secret_value() -> &'static Regex {
    SECRET_VALUE.get_or_init(|| {
        Regex::new(
            r#"(?i)(?P<key>\b(?:authorization|api[_-]?key|access[_-]?token|refresh[_-]?token|token|password|passwd|secret|cookie)\b["']?\s*[:=]\s*)(?P<value>bearer\s+[^\s,;]+|"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'|[^\s,;]+)"#,
        )
        .expect("the built-in secret expression is valid")
    })
}

pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Release);
}

pub fn redact(message: &str) -> String {
    if !ENABLED.load(Ordering::Acquire) {
        return message.to_string();
    }
    redact_logs(message)
}

/// Remove personal and credential-shaped data from anything written to disk.
///
/// Log privacy is unconditional: a reporter should be able to attach a log
/// without first knowing that the separate UI privacy option exists.
pub fn redact_logs(message: &str) -> String {
    redact_with_patterns(message, username_patterns())
}

fn redact_with_patterns(message: &str, usernames: &[Regex]) -> String {
    let mut redacted = path_username()
        .replace_all(message, "${prefix}[user]")
        .into_owned();
    for username in usernames {
        redacted = username.replace_all(&redacted, "[user]").into_owned();
    }
    redacted = steam_userdata()
        .replace_all(&redacted, "${prefix}[steam-user]")
        .into_owned();
    redacted = steam_id64()
        .replace_all(&redacted, "[steam-user]")
        .into_owned();
    redacted = email_address()
        .replace_all(&redacted, "[email]")
        .into_owned();
    secret_value()
        .replace_all(&redacted, "${key}[redacted]")
        .into_owned()
}

#[cfg(test)]
pub(crate) fn redact_for_usernames(message: &str, usernames: &[&str]) -> String {
    let patterns = usernames
        .iter()
        .map(|username| Regex::new(&format!(r"(?i){}", regex::escape(username))).unwrap())
        .collect::<Vec<_>>();
    redact_with_patterns(message, &patterns)
}

#[cfg(test)]
mod tests {
    use super::{redact_for_usernames, redact_logs, set_enabled};

    #[test]
    fn masks_user_segments_in_macos_windows_and_linux_paths() {
        let raw = "/Users/alice/Games; C:\\Users\\bob\\AppData; /home/carol/.steam";
        assert_eq!(
            redact_for_usernames(raw, &[]),
            "/Users/[user]/Games; C:\\Users\\[user]\\AppData; /home/[user]/.steam"
        );
    }

    #[test]
    fn masks_the_current_username_even_outside_a_path() {
        assert_eq!(
            redact_for_usernames("Could not open alice's profile at /tmp", &["alice"]),
            "Could not open [user]'s profile at /tmp"
        );
    }

    #[test]
    fn masks_usernames_in_mixed_error_and_url_contexts() {
        let raw = "alice failed: owner=ALICE; mail=alice@example.test; file:///Users/alice/Game";
        assert_eq!(
            redact_for_usernames(raw, &["alice"]),
            "[user] failed: owner=[user]; mail=[user]@example.test; file:///Users/[user]/Game"
        );
    }

    #[test]
    fn disk_logs_are_redacted_even_when_ui_privacy_is_disabled() {
        set_enabled(false);
        let raw = "/Users/alice/Game token=abcd 76561198012345678";
        let redacted = redact_logs(raw);

        assert_eq!(redacted, "/Users/[user]/Game token=[redacted] [steam-user]");
    }

    #[test]
    fn masks_steam_accounts_emails_and_secret_shaped_values() {
        let raw = r"/userdata/123456789/config C:\Steam\userdata\987654321\config owner@example.test Authorization:Bearer-123 password=hunter2 access_token=abcd";
        assert_eq!(
            redact_for_usernames(raw, &[]),
            r"/userdata/[steam-user]/config C:\Steam\userdata\[steam-user]\config [email] Authorization:[redacted] password=[redacted] access_token=[redacted]"
        );
    }

    #[test]
    fn masks_quoted_json_secrets_values_with_spaces_and_bearer_tokens() {
        let raw = r#"{"token":"abc.def.ghi","safe":"visible"} password='two words' Authorization: Bearer abc.def.ghi"#;
        let redacted = redact_for_usernames(raw, &[]);

        assert!(!redacted.contains("abc.def.ghi"));
        assert!(!redacted.contains("two words"));
        assert!(redacted.contains(r#""safe":"visible""#));
        assert_eq!(redacted.matches("[redacted]").count(), 3);
    }
}
