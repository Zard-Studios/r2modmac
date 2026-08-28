//! Custom themes.
//!
//! A theme is a small TOML file in `<app-data>/themes/`. Presets that ship with
//! the app live in the frontend rather than as files here, so the stock look
//! stays the absence of a theme and a built-in cannot be half-deleted.
//!
//! This module only reads, writes and watches those files. Turning nine colours
//! into the palette the UI paints with happens in the frontend
//! (`src/utils/theme.ts`), in one place, so the live editor preview and the
//! applied theme cannot drift apart.

use std::{
    collections::{BTreeMap, HashSet},
    path::{Component, Path, PathBuf},
    sync::mpsc,
    thread,
    time::Duration,
};

use notify::{EventKind, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use tauri::{command, AppHandle, Emitter};

/// Emitted whenever the themes directory changes on disk, so a theme edited in
/// an external editor shows up without the user restarting anything.
const THEMES_CHANGED_EVENT: &str = "themes-changed";

/// Collapses the burst of events a single save produces (write, truncate,
/// metadata) into one reload.
const EVENT_DEBOUNCE: Duration = Duration::from_millis(300);

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ThemeColors {
    #[serde(default)]
    pub background: Option<String>,
    #[serde(default)]
    pub surface: Option<String>,
    #[serde(default)]
    pub border: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub text_muted: Option<String>,
    #[serde(default)]
    pub accent: Option<String>,
    /// Hover states. Declared here because the frontend writes them: a field it
    /// saves but this struct does not name is silently dropped by serde, and
    /// the theme then reloads with the *default* theme's value in its place —
    /// which is how a coral theme came back with a blue hover.
    #[serde(default)]
    pub accent_hover: Option<String>,
    #[serde(default)]
    pub surface_hover: Option<String>,
    #[serde(default)]
    pub danger: Option<String>,
    #[serde(default)]
    pub warning: Option<String>,
    #[serde(default)]
    pub success: Option<String>,
    // Consulted only while `auto_contrast` is off.
    #[serde(default)]
    pub on_accent: Option<String>,
    #[serde(default)]
    pub on_surface: Option<String>,
    #[serde(default)]
    pub on_danger: Option<String>,
    #[serde(default)]
    pub on_warning: Option<String>,
    #[serde(default)]
    pub on_success: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    /// Chrome over cover art.
    #[serde(default)]
    pub media_scrim: Option<String>,
    #[serde(default)]
    pub media_ink: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ThemeOpacity {
    #[serde(default)]
    pub background: Option<f32>,
    #[serde(default)]
    pub surface: Option<f32>,
    #[serde(default)]
    pub surface_hover: Option<f32>,
    #[serde(default)]
    pub border: Option<f32>,
    #[serde(default)]
    pub text: Option<f32>,
    #[serde(default)]
    pub text_muted: Option<f32>,
    #[serde(default)]
    pub accent: Option<f32>,
    #[serde(default)]
    pub accent_hover: Option<f32>,
    #[serde(default)]
    pub danger: Option<f32>,
    #[serde(default)]
    pub warning: Option<f32>,
    #[serde(default)]
    pub success: Option<f32>,
    #[serde(default)]
    pub on_accent: Option<f32>,
    #[serde(default)]
    pub on_surface: Option<f32>,
    #[serde(default)]
    pub on_danger: Option<f32>,
    #[serde(default)]
    pub on_warning: Option<f32>,
    #[serde(default)]
    pub on_success: Option<f32>,
    #[serde(default)]
    pub icon: Option<f32>,
    #[serde(default)]
    pub media_scrim: Option<f32>,
    #[serde(default)]
    pub media_ink: Option<f32>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ThemeOptions {
    /// Whether the engine picks label colours per filled control. Absent means
    /// on, so themes written before the option existed keep working.
    #[serde(default)]
    pub auto_contrast: Option<bool>,
    /// Keep the stock multicolour SVG palette. Absent means enabled.
    #[serde(default)]
    pub adapt_svg: Option<bool>,
    /// Blur behind the app's translucent interface. Zero keeps the compositor
    /// filter entirely disabled.
    #[serde(default)]
    pub interface_blur: Option<f32>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ThemeBackgroundImage {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub opacity: Option<f32>,
    #[serde(default)]
    pub blur: Option<f32>,
    /// How the picture is laid out: cover, contain, fill, tile or center.
    /// Undeclared fields are dropped by serde without a word, which is why
    /// these settings appeared to save and then came back at their defaults.
    #[serde(default)]
    pub fit: Option<String>,
    #[serde(default)]
    pub offset_x: Option<f32>,
    #[serde(default)]
    pub offset_y: Option<f32>,
    /// Tile size as a percentage of the viewport, for `fit = "tile"`.
    #[serde(default)]
    pub tile_scale: Option<f32>,
}

/// The on-disk shape. Every field is optional: a half-written file should load
/// with the colours it does define rather than be rejected outright.
#[derive(Deserialize, Default)]
struct ThemeDocument {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    colors: ThemeColors,
    #[serde(default)]
    icons: BTreeMap<String, String>,
    #[serde(default)]
    opacity: Option<ThemeOpacity>,
    #[serde(default)]
    background_image: Option<ThemeBackgroundImage>,
    #[serde(default)]
    options: Option<ThemeOptions>,
}

#[derive(Serialize, Clone, Debug)]
pub struct ThemeSummary {
    /// Identifies the theme everywhere, including in settings.
    pub file_name: String,
    pub name: String,
    pub author: Option<String>,
    pub colors: ThemeColors,
    #[serde(default)]
    pub icons: BTreeMap<String, String>,
    #[serde(default)]
    pub opacity: Option<ThemeOpacity>,
    #[serde(default)]
    pub background_image: Option<ThemeBackgroundImage>,
    #[serde(default)]
    pub options: Option<ThemeOptions>,
    /// A parse failure, carried rather than swallowed: someone hand-editing a
    /// file needs to see *why* their theme stopped loading.
    pub error: Option<String>,
}

pub fn themes_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = crate::utils::paths::app_data_dir(app)
        .map_err(|e| format!("Failed to resolve the app data directory: {}", e))?
        .join("themes");
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create the themes directory: {}", e))?;
    Ok(dir)
}

/// Reduce a caller-supplied name to a single, safe `.toml` file name.
///
/// Theme names travel from the frontend and from settings, so anything with
/// path structure — separators, `..`, a drive prefix — is rejected outright
/// rather than normalised, keeping reads and writes inside the themes folder.
fn safe_theme_file_name(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("Theme file name is empty".to_string());
    }

    let mut components = Path::new(trimmed).components();
    let name = match (components.next(), components.next()) {
        (Some(Component::Normal(value)), None) => value.to_string_lossy().to_string(),
        _ => return Err(format!("Invalid theme file name: {}", raw)),
    };

    if !name.to_lowercase().ends_with(".toml") {
        return Err(format!("Theme files must end in .toml: {}", raw));
    }

    Ok(name)
}

fn theme_path(app: &AppHandle, file_name: &str) -> Result<PathBuf, String> {
    Ok(themes_dir(app)?.join(safe_theme_file_name(file_name)?))
}

/// Accept a built-in id only if it is a plain slug, so it cannot smuggle path
/// structure into settings and back out through some later lookup.
fn validate_builtin_id(raw: &str) -> Result<String, String> {
    let slug = &raw[BUILTIN_PREFIX.len()..];
    let ok = !slug.is_empty()
        && slug
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if ok {
        Ok(raw.to_string())
    } else {
        Err(format!("Invalid built-in theme id: {}", raw))
    }
}

/// Where a theme's own files (background pictures) live.
fn theme_assets_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = themes_dir(app)?.join("assets");
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create the theme assets folder: {}", e))?;
    Ok(dir)
}

/// Reduce a theme-relative asset reference to a safe path inside `themes/`.
///
/// Background pictures are referenced from hand-editable TOML, so the same
/// rule as theme file names applies: reject anything with path structure rather
/// than trying to clean it up. One `assets/` prefix is the only nesting allowed.
fn safe_asset_relative_path(raw: &str) -> Result<PathBuf, String> {
    let trimmed = raw.trim().replace('\\', "/");
    let trimmed = trimmed.trim_start_matches("./");
    if trimmed.is_empty() {
        return Err("Background image path is empty".to_string());
    }

    let mut parts = Vec::new();
    for component in Path::new(trimmed).components() {
        match component {
            Component::Normal(value) => parts.push(value.to_string_lossy().to_string()),
            Component::CurDir => {}
            _ => return Err(format!("Invalid background image path: {}", raw)),
        }
    }

    match parts.len() {
        1 => Ok(PathBuf::from("assets").join(&parts[0])),
        2 if parts[0] == "assets" => Ok(PathBuf::from("assets").join(&parts[1])),
        _ => Err(format!("Invalid background image path: {}", raw)),
    }
}

const MAX_BACKGROUND_BYTES: u64 = 24 * 1024 * 1024;

/// Copy a picture the user chose into the themes folder.
///
/// Copying rather than referencing keeps a theme self-contained: the folder can
/// be zipped and handed to someone else and the background travels with it,
/// and moving the original file later cannot break the theme.
#[command]
pub async fn import_theme_image(app: AppHandle, source_path: String) -> Result<String, String> {
    let source = PathBuf::from(&source_path);
    let metadata =
        std::fs::metadata(&source).map_err(|e| format!("Could not read the image: {}", e))?;
    if metadata.len() > MAX_BACKGROUND_BYTES {
        return Err(format!(
            "That image is {} MB. Backgrounds are limited to {} MB so themes stay quick to load.",
            metadata.len() / 1_048_576,
            MAX_BACKGROUND_BYTES / 1_048_576
        ));
    }

    let extension = source
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();
    if !matches!(
        extension.as_str(),
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp"
    ) {
        return Err("Backgrounds must be a PNG, JPEG, WebP, GIF or BMP image".to_string());
    }

    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| {
            s.chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
                .collect::<String>()
        })
        .map(|s| s.trim_matches('-').to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "background".to_string());

    let assets = theme_assets_dir(&app)?;

    let bytes = std::fs::read(&source).map_err(|e| format!("Could not read the image: {}", e))?;
    if let Some(existing) = find_identical_asset(&assets, &bytes) {
        return Ok(format!("assets/{}", existing));
    }

    let mut candidate = format!("{}.{}", stem, extension);
    let mut counter = 2;
    while assets.join(&candidate).exists() {
        candidate = format!("{}-{}.{}", stem, counter, extension);
        counter += 1;
    }

    std::fs::write(assets.join(&candidate), &bytes)
        .map_err(|e| format!("Could not copy the image: {}", e))?;

    Ok(format!("assets/{}", candidate))
}

/// Find an already-imported asset holding exactly these bytes.
///
/// Picking the same picture twice — for a second theme, or after forgetting it
/// was already imported — used to leave `wall.png` beside an identical
/// `wall-2.png`, and the folder filled up with copies of the same wallpaper.
/// Matching on content rather than on file name also catches the case where the
/// same image arrives under a different name.
///
/// Size is checked first so the byte comparison only runs on real candidates,
/// and entries are sorted so a folder with several identical copies always
/// resolves to the same one instead of whatever order the filesystem returns.
fn find_identical_asset(assets: &std::path::Path, bytes: &[u8]) -> Option<String> {
    let mut names: Vec<String> = std::fs::read_dir(assets)
        .ok()?
        .flatten()
        .filter(|entry| entry.metadata().ok().map(|m| m.len()) == Some(bytes.len() as u64))
        .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
        .collect();
    names.sort();
    names.into_iter().find(|name| {
        std::fs::read(assets.join(name))
            .map(|found| found == bytes)
            .unwrap_or(false)
    })
}

/// Read a theme's background picture as a data URL for the webview.
#[command]
pub async fn read_theme_image(
    app: AppHandle,
    relative_path: String,
) -> Result<Option<String>, String> {
    use base64::Engine;

    let path = themes_dir(&app)?.join(safe_asset_relative_path(&relative_path)?);
    if !path.exists() {
        return Ok(None);
    }

    let bytes = std::fs::read(&path).map_err(|e| format!("Could not read the image: {}", e))?;
    let mime = match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        _ => "image/png",
    };

    Ok(Some(format!(
        "data:{};base64,{}",
        mime,
        base64::engine::general_purpose::STANDARD.encode(&bytes)
    )))
}

fn parse_theme(file_name: String, source: &str) -> ThemeSummary {
    match toml::from_str::<ThemeDocument>(source) {
        Ok(doc) => {
            let fallback = file_name
                .trim_end_matches(".toml")
                .trim_end_matches(".TOML");
            let name = doc
                .name
                .map(|n| n.trim().to_string())
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| fallback.to_string());
            ThemeSummary {
                file_name,
                name,
                author: doc
                    .author
                    .map(|a| a.trim().to_string())
                    .filter(|a| !a.is_empty()),
                colors: doc.colors,
                icons: doc.icons,
                opacity: doc.opacity,
                background_image: doc.background_image,
                options: doc.options,
                error: None,
            }
        }
        Err(error) => ThemeSummary {
            name: file_name.trim_end_matches(".toml").to_string(),
            file_name,
            author: None,
            colors: ThemeColors::default(),
            icons: BTreeMap::new(),
            opacity: None,
            background_image: None,
            options: None,
            error: Some(error.message().to_string()),
        },
    }
}

#[command]
pub async fn list_themes(app: AppHandle) -> Result<Vec<ThemeSummary>, String> {
    let dir = themes_dir(&app)?;
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        // A missing directory just means no themes yet.
        Err(_) => return Ok(Vec::new()),
    };

    let mut themes = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let is_toml = path
            .extension()
            .map(|ext| ext.eq_ignore_ascii_case("toml"))
            .unwrap_or(false);
        if !is_toml {
            continue;
        }
        let file_name = match path.file_name() {
            Some(name) => name.to_string_lossy().to_string(),
            None => continue,
        };
        match std::fs::read_to_string(&path) {
            Ok(source) => themes.push(parse_theme(file_name, &source)),
            Err(error) => themes.push(ThemeSummary {
                name: file_name.trim_end_matches(".toml").to_string(),
                file_name,
                author: None,
                colors: ThemeColors::default(),
                icons: BTreeMap::new(),
                opacity: None,
                background_image: None,
                options: None,
                error: Some(format!("Could not read the file: {}", error)),
            }),
        }
    }

    themes.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(themes)
}

/// The raw file, for the editor's TOML view and for anyone who wants to see
/// exactly what is on disk.
#[command]
pub async fn read_theme_source(app: AppHandle, file_name: String) -> Result<String, String> {
    let path = theme_path(&app, &file_name)?;
    std::fs::read_to_string(&path).map_err(|e| format!("Could not read {}: {}", file_name, e))
}

#[command]
pub async fn write_theme(app: AppHandle, file_name: String, content: String) -> Result<(), String> {
    let path = theme_path(&app, &file_name)?;
    std::fs::write(&path, content).map_err(|e| format!("Could not save {}: {}", file_name, e))
}

#[command]
pub async fn delete_theme(app: AppHandle, file_name: String) -> Result<(), String> {
    let path = theme_path(&app, &file_name)?;
    if !path.exists() {
        return Ok(());
    }
    std::fs::remove_file(&path).map_err(|e| format!("Could not delete {}: {}", file_name, e))
}

#[command]
pub async fn open_themes_folder(app: AppHandle) -> Result<(), String> {
    let dir = themes_dir(&app)?;
    open::that(&dir).map_err(|e| format!("Could not open the themes folder: {}", e))
}

/// Watch the themes folder so files edited outside the app are picked up.
///
/// This is the half of the feature that serves people who would rather use
/// their own editor than the in-app one: save the file, the app repaints.
pub fn start_theme_watcher(app: AppHandle) {
    thread::Builder::new()
        .name("theme-watcher".to_string())
        .spawn(move || {
            if let Err(error) = run_theme_watcher(app) {
                log::warn!("[theme_watcher] stopped: {}", error);
            }
        })
        .ok();
}

fn run_theme_watcher(app: AppHandle) -> Result<(), String> {
    let dir = themes_dir(&app)?;

    let (tx, rx) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |event| {
        let _ = tx.send(event);
    })
    .map_err(|e| format!("Failed to start the theme watcher: {}", e))?;

    watcher
        .watch(&dir, RecursiveMode::NonRecursive)
        .map_err(|e| format!("Failed to watch the themes folder: {}", e))?;

    log::debug!("[theme_watcher] watching {}", dir.display());

    loop {
        match rx.recv() {
            Ok(Ok(event)) => {
                if !touches_a_theme(&event) {
                    continue;
                }
                // A single save arrives as several events; wait for the burst to
                // settle so the UI reloads once rather than flickering.
                while rx.recv_timeout(EVENT_DEBOUNCE).is_ok() {}
                let _ = app.emit(THEMES_CHANGED_EVENT, ());
            }
            Ok(Err(error)) => log::warn!("[theme_watcher] watch error: {}", error),
            Err(_) => return Err("Theme watcher channel disconnected".to_string()),
        }
    }
}

fn touches_a_theme(event: &notify::Event) -> bool {
    if matches!(event.kind, EventKind::Access(_)) {
        return false;
    }
    // Editors write temp/backup files alongside the real one; only .toml counts.
    event.paths.iter().any(|path| {
        path.extension()
            .map(|ext| ext.eq_ignore_ascii_case("toml"))
            .unwrap_or(false)
    })
}

/// Themes that ship with the app are identified by this prefix rather than a
/// file name, since they have no file. Kept distinct so a preset id can never
/// be mistaken for something on disk.
const BUILTIN_PREFIX: &str = "builtin:";

/// Select a theme, or pass `None` for the stock palette.
///
/// Deliberately its own command rather than a field on a whole-settings save:
/// the theme changes from the appearance panel while other preferences may be
/// mid-edit elsewhere, and a read-modify-write of the entire settings file
/// would let one of the two silently overwrite the other.
#[command]
pub async fn set_active_theme(app: AppHandle, file_name: Option<String>) -> Result<(), String> {
    let normalized = match file_name {
        Some(name) if name.starts_with(BUILTIN_PREFIX) => Some(validate_builtin_id(&name)?),
        Some(name) => Some(safe_theme_file_name(&name)?),
        None => None,
    };
    let mut settings = crate::models::shared::load_settings_impl(&app);
    settings.active_theme = normalized;
    crate::models::shared::save_settings_impl(&app, &settings)
}

/// Turn a theme name into a file name that will not collide with an existing one.
#[command]
pub async fn suggest_theme_file_name(app: AppHandle, name: String) -> Result<String, String> {
    let slug: String = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let slug = slug.trim_matches('-').replace("--", "-");
    let stem = if slug.is_empty() {
        "theme".to_string()
    } else {
        slug
    };

    let existing: HashSet<String> = std::fs::read_dir(themes_dir(&app)?)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|e| e.file_name().to_str().map(|s| s.to_lowercase()))
                .collect()
        })
        .unwrap_or_default();

    let mut candidate = format!("{}.toml", stem);
    let mut counter = 2;
    while existing.contains(&candidate.to_lowercase()) {
        candidate = format!("{}-{}.toml", stem, counter);
        counter += 1;
    }
    Ok(candidate)
}

#[cfg(test)]
mod theme_file_name_tests {
    use super::*;

    #[test]
    fn plain_names_are_accepted() {
        assert_eq!(safe_theme_file_name("nord.toml").unwrap(), "nord.toml");
        assert_eq!(
            safe_theme_file_name("  My Theme.TOML  ").unwrap(),
            "My Theme.TOML"
        );
    }

    #[test]
    fn path_traversal_is_refused_rather_than_normalised() {
        // Theme names reach this function from settings and from the frontend,
        // so escaping the themes folder must fail loudly, not get cleaned up.
        for hostile in [
            "../settings.json",
            "../../etc/passwd.toml",
            "sub/dir.toml",
            "/etc/hosts.toml",
            "..",
        ] {
            assert!(
                safe_theme_file_name(hostile).is_err(),
                "expected {} to be rejected",
                hostile
            );
        }
    }

    #[test]
    fn builtin_ids_are_accepted_but_only_as_plain_slugs() {
        assert_eq!(
            validate_builtin_id("builtin:github-dark").unwrap(),
            "builtin:github-dark"
        );
        // A preset id reaches this from settings, so it must not be able to
        // carry path structure through to any later lookup.
        for hostile in [
            "builtin:../../etc/passwd",
            "builtin:a/b",
            "builtin:",
            "builtin:has space",
        ] {
            assert!(
                validate_builtin_id(hostile).is_err(),
                "expected {} to be rejected",
                hostile
            );
        }
    }

    #[test]
    fn background_paths_stay_inside_the_themes_folder() {
        assert_eq!(
            safe_asset_relative_path("wall.jpg").unwrap(),
            PathBuf::from("assets/wall.jpg")
        );
        assert_eq!(
            safe_asset_relative_path("assets/wall.jpg").unwrap(),
            PathBuf::from("assets/wall.jpg")
        );
        for hostile in [
            "../settings.json",
            "assets/../../secrets.txt",
            "/etc/hosts",
            "a/b/c.png",
            "",
        ] {
            assert!(
                safe_asset_relative_path(hostile).is_err(),
                "expected {} to be rejected",
                hostile
            );
        }
    }

    #[test]
    fn a_background_image_section_is_parsed() {
        let summary = parse_theme(
            "wall.toml".to_string(),
            r##"
name = "Wall"
[colors]
accent = "#88c0d0"
[background_image]
path = "assets/wall.jpg"
opacity = 0.4
blur = 6
fit = "tile"
offset_x = 25
offset_y = 75
tile_scale = 40
"##,
        );
        let image = summary.background_image.expect("expected a background");
        assert_eq!(image.path.as_deref(), Some("assets/wall.jpg"));
        assert_eq!(image.opacity, Some(0.4));
        assert_eq!(image.blur, Some(6.0));
        assert_eq!(image.fit.as_deref(), Some("tile"));
        assert_eq!(image.offset_x, Some(25.0));
        assert_eq!(image.offset_y, Some(75.0));
        assert_eq!(image.tile_scale, Some(40.0));
    }

    #[test]
    fn interface_blur_is_parsed_from_theme_options() {
        let summary = parse_theme(
            "glass.toml".to_string(),
            r##"
name = "Glass"
[colors]
accent = "#88c0d0"
[options]
auto_contrast = true
interface_blur = 24
"##,
        );
        let options = summary.options.expect("expected theme options");
        assert_eq!(options.auto_contrast, Some(true));
        assert_eq!(options.interface_blur, Some(24.0));
    }

    #[test]
    fn svg_icon_overrides_are_parsed_without_enumerating_them() {
        let summary = parse_theme(
            "icons.toml".to_string(),
            r##"
[options]
adapt_svg = false

[icons]
version = "#8b5cf6"
future_svg = "#22c55e"
"##,
        );
        let options = summary.options.expect("expected theme options");
        assert_eq!(options.adapt_svg, Some(false));
        assert_eq!(summary.icons.get("version").map(String::as_str), Some("#8b5cf6"));
        assert_eq!(summary.icons.get("future_svg").map(String::as_str), Some("#22c55e"));
    }

    #[test]
    fn the_new_status_colours_round_trip() {
        let summary = parse_theme(
            "status.toml".to_string(),
            r##"
[colors]
danger  = "#ff5555"
warning = "#f1fa8c"
success = "#50fa7b"
"##,
        );
        assert_eq!(summary.colors.danger.as_deref(), Some("#ff5555"));
        assert_eq!(summary.colors.warning.as_deref(), Some("#f1fa8c"));
        assert_eq!(summary.colors.success.as_deref(), Some("#50fa7b"));
    }

    #[test]
    fn per_colour_opacity_is_parsed() {
        let summary = parse_theme(
            "alpha.toml".to_string(),
            r##"
[colors]
background = "#111827"
[opacity]
background = 0.42
accent = 0.75
media_scrim = 0.25
"##,
        );
        let opacity = summary.opacity.expect("expected opacity values");
        assert_eq!(opacity.background, Some(0.42));
        assert_eq!(opacity.accent, Some(0.75));
        assert_eq!(opacity.media_scrim, Some(0.25));
    }

    #[test]
    fn non_toml_files_are_refused() {
        assert!(safe_theme_file_name("theme.json").is_err());
        assert!(safe_theme_file_name("theme").is_err());
        assert!(safe_theme_file_name("").is_err());
    }

    #[test]
    fn a_valid_file_is_parsed_into_its_colours() {
        let summary = parse_theme(
            "nord.toml".to_string(),
            r##"
name = "Nord"
author = "arcticicestudio"

[colors]
background = "#2e3440"
surface = "#3b4252"
accent = "#88c0d0"
"##,
        );
        assert_eq!(summary.name, "Nord");
        assert_eq!(summary.author.as_deref(), Some("arcticicestudio"));
        assert_eq!(summary.colors.background.as_deref(), Some("#2e3440"));
        assert_eq!(summary.colors.accent.as_deref(), Some("#88c0d0"));
        // Unspecified colours stay absent; the frontend fills them from defaults.
        assert!(summary.colors.text.is_none());
        assert!(summary.error.is_none());
    }

    #[test]
    fn every_colour_the_frontend_writes_survives_a_reload() {
        // A field this struct forgets to declare is dropped without a word, and
        // the frontend then backfills it from the default theme — so a missing
        // `accent_hover` turned a coral theme's hover blue. Reading back every
        // key the writer emits is what stops that from recurring silently.
        let summary = parse_theme(
            "round-trip.toml".to_string(),
            r##"
[colors]
background    = "#191919"
surface       = "#262625"
surface_hover = "#333330"
border        = "#3d3d3a"
text          = "#f4f3ee"
text_muted    = "#b1ada1"
accent        = "#d97757"
accent_hover  = "#e28e72"
danger        = "#c9184a"
warning       = "#d4a27f"
success       = "#7fa87f"
on_accent     = "#191919"
icon          = "#d97757"
media_scrim   = "#09090b"
media_ink     = "#ffffff"
"##,
        );
        assert!(summary.error.is_none(), "{:?}", summary.error);
        assert_eq!(summary.colors.accent_hover.as_deref(), Some("#e28e72"));
        assert_eq!(summary.colors.surface_hover.as_deref(), Some("#333330"));
        assert_eq!(summary.colors.on_accent.as_deref(), Some("#191919"));
        assert_eq!(summary.colors.icon.as_deref(), Some("#d97757"));
        assert_eq!(summary.colors.media_scrim.as_deref(), Some("#09090b"));
        assert_eq!(summary.colors.media_ink.as_deref(), Some("#ffffff"));
    }

    #[test]
    fn a_broken_file_reports_why_instead_of_vanishing() {
        let summary = parse_theme("broken.toml".to_string(), "name = \"unterminated");
        assert!(summary.error.is_some(), "expected a parse error to surface");
        // It still appears in the list, under a usable name, so the user can
        // find and fix it from the UI.
        assert_eq!(summary.name, "broken");
    }

    #[test]
    fn a_nameless_file_falls_back_to_its_file_name() {
        let summary = parse_theme("my-theme.toml".to_string(), "[colors]\naccent = \"#fff\"");
        assert_eq!(summary.name, "my-theme");
    }
}

#[cfg(test)]
mod theme_asset_reuse_tests {
    use super::find_identical_asset;

    fn temp_root(label: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "r2modmac-{}-{}-{}",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn importing_the_same_picture_twice_reuses_the_first_copy() {
        let assets = temp_root("theme-assets");
        std::fs::write(assets.join("wall.png"), b"the same pixels").unwrap();

        assert_eq!(
            find_identical_asset(&assets, b"the same pixels").as_deref(),
            Some("wall.png")
        );

        std::fs::remove_dir_all(&assets).ok();
    }

    #[test]
    fn a_folder_already_holding_duplicates_resolves_to_one_of_them() {
        // Folders that filled up before this existed still hold copies. Whichever
        // is picked must be the same every time, or re-importing would rewrite
        // the theme's path on each attempt.
        let assets = temp_root("theme-assets-duplicates");
        for name in ["wall-3.png", "wall.png", "wall-2.png"] {
            std::fs::write(assets.join(name), b"the same pixels").unwrap();
        }

        assert_eq!(
            find_identical_asset(&assets, b"the same pixels").as_deref(),
            Some("wall-2.png"),
            "sorted order decides, not the order the filesystem lists"
        );

        std::fs::remove_dir_all(&assets).ok();
    }

    #[test]
    fn a_different_picture_of_the_same_size_is_not_reused() {
        // Equal length is the cheap pre-filter; it must never stand in for a
        // match on its own, or one wallpaper would silently replace another.
        let assets = temp_root("theme-assets-collision");
        std::fs::write(assets.join("wall.png"), b"first  wallpaper").unwrap();

        assert_eq!(find_identical_asset(&assets, b"second wallpaper"), None);

        std::fs::remove_dir_all(&assets).ok();
    }

    #[test]
    fn an_empty_folder_yields_nothing_to_reuse() {
        let assets = temp_root("theme-assets-empty");
        assert_eq!(find_identical_asset(&assets, b"pixels"), None);
        std::fs::remove_dir_all(&assets).ok();
    }
}
