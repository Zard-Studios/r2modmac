export const PREFERENCE_ICON_COLORS = {
    install: 'text-fg-icon',
    version: 'text-fg-icon',
    parallel: 'text-fg-icon',
    apply: 'text-fg-success',
    logs: 'text-fg-icon',
    layout: 'text-fg-icon',
    warning: 'text-fg-warning',
    cache: 'text-fg-danger',
    stream: 'text-fg-icon',
    update: 'text-fg-success',
    support: 'text-fg-icon',
    folder: 'text-fg-icon',
    game: 'text-fg-icon',
    profile: 'text-fg-icon',
    theme: 'text-fg-icon',
    keyboard: 'text-fg-icon',
} as const;

export type PreferencesIconName = keyof typeof PREFERENCE_ICON_COLORS;
