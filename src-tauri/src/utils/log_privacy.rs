use regex::Regex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

static ENABLED: AtomicBool = AtomicBool::new(false);
static PATH_USERNAME: OnceLock<Regex> = OnceLock::new();
static USERNAME_PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();

fn path_username() -> &'static Regex {
    PATH_USERNAME.get_or_init(|| {
        Regex::new(r"(?i)(?P<prefix>(?:[a-z]:)?[\\/](?:users|home|documents and settings)[\\/])(?P<username>[^\\/:\r\n]+)")
            .expect("the built-in username path expression is valid")
    })
}

fn username_patterns() -> &'static [Regex] {
    USERNAME_PATTERNS
        .get_or_init(|| {
            ["USER", "USERNAME"]
                .into_iter()
                .filter_map(|name| std::env::var(name).ok())
                .map(|name| name.trim().to_string())
                .filter(|name| !name.is_empty())
                .map(|name| Regex::new(&format!(r"(?i){}", regex::escape(&name))))
                .filter_map(Result::ok)
                .collect()
        })
        .as_slice()
}

pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Release);
}

pub fn redact(message: &str) -> String {
    if !ENABLED.load(Ordering::Acquire) {
        return message.to_string();
    }
    redact_with_patterns(message, username_patterns())
}

fn redact_with_patterns(message: &str, usernames: &[Regex]) -> String {
    let mut redacted = path_username()
        .replace_all(message, "${prefix}[user]")
        .into_owned();
    for username in usernames {
        redacted = username.replace_all(&redacted, "[user]").into_owned();
    }
    redacted
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
    use super::redact_for_usernames;

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
}
