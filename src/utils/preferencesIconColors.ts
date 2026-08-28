/**
 * The single catalogue for every SVG glyph used by Preferences.
 *
 * RowIcon and the theme editor both iterate this object. Adding a new
 * Preferences icon here therefore makes it available to custom themes without
 * maintaining a second list or a Rust struct field.
 */
export const PREFERENCE_ICON_CATALOG = {
    install: { label: 'Install', tone: 'accent', className: 'text-fg-accent' },
    version: { label: 'Version', tone: 'cyan', className: 'text-cyan-400' },
    parallel: { label: 'Parallel downloads', tone: 'violet', className: 'text-violet-400' },
    apply: { label: 'Apply', tone: 'success', className: 'text-fg-success' },
    logs: { label: 'Logs', tone: 'sky', className: 'text-sky-400' },
    layout: { label: 'Layout', tone: 'indigo', className: 'text-indigo-400' },
    warning: { label: 'Warnings', tone: 'warning', className: 'text-fg-warning' },
    cache: { label: 'Cache', tone: 'danger', className: 'text-fg-danger' },
    stream: { label: 'Stream mode', tone: 'fuchsia', className: 'text-fuchsia-400' },
    update: { label: 'Updates', tone: 'success', className: 'text-fg-success' },
    support: { label: 'Support', tone: 'rose', className: 'text-rose-400' },
    folder: { label: 'Folders', tone: 'orange', className: 'text-orange-400' },
    game: { label: 'Games', tone: 'teal', className: 'text-teal-400' },
    profile: { label: 'Profiles', tone: 'purple', className: 'text-purple-400' },
    theme: { label: 'Themes', tone: 'accent', className: 'text-fg-accent' },
    keyboard: { label: 'Keyboard', tone: 'warning', className: 'text-amber-400' },
} as const;

export type PreferencesIconName = keyof typeof PREFERENCE_ICON_CATALOG;
export type PreferencesIconTone = (typeof PREFERENCE_ICON_CATALOG)[PreferencesIconName]['tone'];

export const PREFERENCE_ICON_NAMES = Object.freeze(
    Object.keys(PREFERENCE_ICON_CATALOG) as PreferencesIconName[]
);

/** Kept as a derived export for callers that only need the stock CSS class. */
export const PREFERENCE_ICON_COLORS = Object.fromEntries(
    PREFERENCE_ICON_NAMES.map((name) => [name, PREFERENCE_ICON_CATALOG[name].className])
) as Record<PreferencesIconName, string>;
