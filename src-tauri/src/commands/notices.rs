//! Announcements from the maintainer, shown behind the bell on the home screen.
//!
//! They are plain GitHub issues: the maintainer opens one with the `notice`
//! label and closes it when it is no longer needed, so there is nothing to
//! commit. What counts as a notice is decided here, not by the page that shows
//! it: the issue's author, as GitHub reports it, must be the maintainer's
//! account. A display name proves nothing, because anyone can call themselves
//! "Zard Studios", but nobody else can have this login.

use serde::Serialize;
use std::time::Duration;

const NOTICES_URL: &str = "https://api.github.com/repos/Zard-Studios/r2modmac/issues?labels=notice&creator=Zard-Studios&state=open&per_page=10";
const MAINTAINER_LOGIN: &str = "Zard-Studios";
const NOTICE_LABEL: &str = "notice";
const ISSUE_URL_PREFIX: &str = "https://github.com/Zard-Studios/r2modmac/issues/";
const MAX_TITLE_CHARS: usize = 200;
const MAX_BODY_CHARS: usize = 4000;

#[derive(Debug, Serialize, PartialEq)]
pub struct Notice {
    pub id: u64,
    pub title: String,
    pub body: String,
    /// The issue's own page, only when it really is one of this repository's.
    pub url: Option<String>,
    pub created_at: String,
}

fn truncated(text: &str, max_chars: usize) -> String {
    text.trim().chars().take(max_chars).collect()
}

/// The notices in a GitHub issue listing. Anything that is not an open issue
/// from the maintainer carrying the notice label is left out, even if GitHub
/// sent it.
pub fn parse_notices(json: &str) -> Result<Vec<Notice>, String> {
    let items: Vec<serde_json::Value> =
        serde_json::from_str(json).map_err(|error| format!("Parse error: {error}"))?;

    Ok(items
        .iter()
        .filter_map(|item| {
            // The issues listing also returns pull requests.
            if item.get("pull_request").is_some() || item["state"] != "open" {
                return None;
            }
            let by_maintainer = item["user"]["login"]
                .as_str()
                .is_some_and(|login| login.eq_ignore_ascii_case(MAINTAINER_LOGIN));
            let labelled = item["labels"].as_array().is_some_and(|labels| {
                labels
                    .iter()
                    .any(|label| label["name"].as_str() == Some(NOTICE_LABEL))
            });
            if !by_maintainer || !labelled {
                return None;
            }
            let title = truncated(item["title"].as_str()?, MAX_TITLE_CHARS);
            if title.is_empty() {
                return None;
            }
            Some(Notice {
                id: item["id"].as_u64()?,
                title,
                body: truncated(item["body"].as_str().unwrap_or(""), MAX_BODY_CHARS),
                url: item["html_url"]
                    .as_str()
                    .filter(|url| url.starts_with(ISSUE_URL_PREFIX))
                    .map(str::to_string),
                created_at: item["created_at"].as_str().unwrap_or("").to_string(),
            })
        })
        .collect())
}

/// Fetch the current notices. Any failure, GitHub's rate limit included, is an
/// error so the page keeps the notices it already has instead of showing none.
#[tauri::command]
pub async fn fetch_notices() -> Result<Vec<Notice>, String> {
    let result = fetch_notices_inner().await;
    match &result {
        Ok(notices) => log::info!("[notices] Found {} announcement(s)", notices.len()),
        Err(error) => log::warn!("[notices] Could not read announcements: {error}"),
    }
    result
}

async fn fetch_notices_inner() -> Result<Vec<Notice>, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|error| format!("Request failed: {error}"))?;
    let response = client
        .get(NOTICES_URL)
        .header("User-Agent", "r2modmac-notices")
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .map_err(|error| format!("Request failed: {error}"))?;

    if !response.status().is_success() {
        return Err(format!("GitHub API error: {}", response.status()));
    }
    let text = response
        .text()
        .await
        .map_err(|error| format!("Read failed: {error}"))?;
    parse_notices(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issue(login: &str, label: &str, extra: &str) -> String {
        format!(
            r#"{{"id":7,"state":"open","title":"Reinstall 1.1.5","body":"Details","html_url":"https://github.com/Zard-Studios/r2modmac/issues/63","created_at":"2026-11-21T10:00:00Z","user":{{"login":"{login}"}},"labels":[{{"name":"{label}"}}]{extra}}}"#
        )
    }

    fn list(items: &[String]) -> String {
        format!("[{}]", items.join(","))
    }

    #[test]
    fn an_issue_from_the_maintainer_with_the_label_is_a_notice() {
        let notices = parse_notices(&list(&[issue("Zard-Studios", "notice", "")])).unwrap();

        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0].title, "Reinstall 1.1.5");
        assert_eq!(
            notices[0].url.as_deref(),
            Some("https://github.com/Zard-Studios/r2modmac/issues/63")
        );
    }

    #[test]
    fn someone_with_a_similar_name_is_not_the_maintainer() {
        for login in [
            "Zard-Studio",
            "ZardStudios",
            "Zard-Studios-official",
            "zard_studios",
        ] {
            let notices = parse_notices(&list(&[issue(login, "notice", "")])).unwrap();
            assert!(notices.is_empty(), "{login}");
        }
    }

    #[test]
    fn an_issue_without_the_label_is_not_a_notice() {
        let notices = parse_notices(&list(&[issue("Zard-Studios", "bug", "")])).unwrap();
        assert!(notices.is_empty());
    }

    #[test]
    fn a_pull_request_is_not_a_notice() {
        let item = issue("Zard-Studios", "notice", r#","pull_request":{"url":"x"}"#);
        assert!(parse_notices(&list(&[item])).unwrap().is_empty());
    }

    #[test]
    fn a_link_to_another_site_is_dropped() {
        let item = issue("Zard-Studios", "notice", "").replace(
            "https://github.com/Zard-Studios/r2modmac/issues/63",
            "https://example.com/phish",
        );
        let notices = parse_notices(&list(&[item])).unwrap();

        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0].url, None);
    }

    #[test]
    fn long_text_is_cut() {
        let item = issue("Zard-Studios", "notice", "").replace("Details", &"a".repeat(9000));
        let notices = parse_notices(&list(&[item])).unwrap();

        assert_eq!(notices[0].body.chars().count(), MAX_BODY_CHARS);
    }

    #[test]
    fn a_reply_that_is_not_a_list_is_an_error() {
        assert!(parse_notices(r#"{"message":"rate limited"}"#).is_err());
    }
}
