// Extension is explicit so Node's ESM loader can resolve this from the test
// runner; Vite handles either form.
import { DEFAULT_THEME, type Theme } from './theme.ts';

/**
 * Themes that ship with the app.
 *
 * They live in code rather than as files in the themes folder: a preset cannot
 * then be half-deleted or broken by a stray edit, and "no theme" stays exactly
 * the stock look. Anyone who wants to change one duplicates it into a real
 * TOML file first, which is also how they get a shareable copy.
 *
 * Palette sources are noted per theme. Where a value was taken from a vendor's
 * own tokens it is reproduced as published; where a mode needed tuning (a dark
 * variant of a light-first brand, say) that is called out.
 */
export interface ThemePreset extends Theme {
    /** Stable identifier stored in settings; namespaced so it can never look
     *  like a theme file name. */
    id: string;
    /** One line on where the palette comes from. */
    origin: string;
}

export const BUILTIN_PREFIX = 'builtin:';

export const THEME_PRESETS: ThemePreset[] = [
    {
        id: `${BUILTIN_PREFIX}r2modmac-light`,
        name: 'r2modmac Light',
        origin: "The app's own palette, inverted",
        colors: {
            background: '#ffffff',
            surface: '#f3f4f6',
            surface_hover: '#e5e7eb',
            border: '#d1d5db',
            text: '#111827',
            text_muted: '#6b7280',
            accent: '#2563eb',
            accent_hover: '#1d4ed8',
            danger: '#dc2626',
            warning: '#d97706',
            success: '#16a34a',
        },
    },
    {
        id: `${BUILTIN_PREFIX}github-dark`,
        name: 'GitHub Dark',
        author: 'GitHub',
        origin: 'GitHub Primer primitives',
        colors: {
            background: '#0d1117', // neutral.1
            surface: '#151b23', // neutral.2
            surface_hover: '#212830', // neutral.3
            border: '#3d444d', // neutral.7
            text: '#f0f6fc',
            text_muted: '#9198a1', // neutral.9
            accent: '#1f6feb', // blue.5
            accent_hover: '#388bfd', // blue.4
            danger: '#f85149', // red.4
            warning: '#d29922', // yellow.3
            success: '#3fb950', // green.3
        },
    },
    {
        id: `${BUILTIN_PREFIX}github-light`,
        name: 'GitHub Light',
        author: 'GitHub',
        origin: 'GitHub Primer primitives',
        colors: {
            background: '#ffffff',
            surface: '#f6f8fa', // neutral.1
            surface_hover: '#eaeef2', // neutral.2
            border: '#d1d9e0', // neutral.6
            text: '#1f2328',
            text_muted: '#59636e', // neutral.9
            accent: '#0969da', // blue.5
            accent_hover: '#0550ae', // blue.6
            danger: '#cf222e', // red.5
            warning: '#9a6700', // yellow.5
            success: '#1a7f37', // green.5
        },
    },
    {
        id: `${BUILTIN_PREFIX}claudio-dark`,
        name: 'Claudio Dark',
        author: 'Anthropic palette',
        origin: "Anthropic's brand colours — coral on near-black",
        colors: {
            background: '#191919',
            surface: '#262625',
            surface_hover: '#333330',
            border: '#3d3d3a',
            text: '#f4f3ee', // Pampas
            text_muted: '#b1ada1', // Cloudy
            accent: '#d97757', // Crail light
            accent_hover: '#e28e72',
            danger: '#c9184a',
            warning: '#d4a27f',
            success: '#7fa87f',
        },
    },
    {
        id: `${BUILTIN_PREFIX}claudio-light`,
        name: 'Claudio Light',
        author: 'Anthropic palette',
        origin: "Anthropic's brand colours — coral on cream",
        colors: {
            background: '#f4f3ee', // Pampas
            surface: '#ffffff',
            surface_hover: '#f0eee6',
            border: '#dedcd1',
            text: '#191919',
            text_muted: '#6b6862',
            accent: '#c15f3c', // Crail
            accent_hover: '#a74d2d',
            danger: '#c9184a',
            warning: '#a9762f',
            success: '#4f7a52',
        },
    },
    {
        id: `${BUILTIN_PREFIX}dracula`,
        name: 'Dracula',
        author: 'Dracula Theme',
        origin: 'The official Dracula specification',
        colors: {
            background: '#282a36',
            surface: '#343746',
            surface_hover: '#44475a', // Current Line
            border: '#44475a',
            text: '#f8f8f2', // Foreground
            text_muted: '#6272a4', // Comment
            accent: '#bd93f9', // Purple
            accent_hover: '#caa9fa',
            danger: '#ff5555', // Red
            warning: '#f1fa8c', // Yellow
            success: '#50fa7b', // Green
        },
    },
    {
        id: `${BUILTIN_PREFIX}nord`,
        name: 'Nord',
        author: 'Arctic Ice Studio',
        origin: 'The official Nord palette',
        colors: {
            background: '#2e3440', // nord0
            surface: '#3b4252', // nord1
            surface_hover: '#434c5e', // nord2
            border: '#4c566a', // nord3
            text: '#eceff4', // nord6
            text_muted: '#a3adbf',
            accent: '#88c0d0', // nord8
            accent_hover: '#81a1c1', // nord9
            danger: '#bf616a', // nord11
            warning: '#ebcb8b', // nord13
            success: '#a3be8c', // nord14
        },
    },
    {
        id: `${BUILTIN_PREFIX}solarized-dark`,
        name: 'Solarized Dark',
        author: 'Ethan Schoonover',
        origin: "Solarized's published accent values",
        colors: {
            background: '#002b36', // base03
            surface: '#073642', // base02
            surface_hover: '#094453',
            border: '#586e75', // base01
            text: '#eee8d5', // base2
            text_muted: '#93a1a1', // base1
            accent: '#268bd2', // blue
            accent_hover: '#2aa198', // cyan
            danger: '#dc322f', // red
            warning: '#b58900', // yellow
            success: '#859900', // green
        },
    },
];

export function findPreset(id: string | null | undefined): ThemePreset | null {
    if (!id) return null;
    return THEME_PRESETS.find((p) => p.id === id) ?? null;
}

export function isBuiltinId(id: string | null | undefined): boolean {
    return !!id && id.startsWith(BUILTIN_PREFIX);
}

/** Presets plus the stock look, in the order the editor lists them. */
export function allBuiltinThemes(): ThemePreset[] {
    return [
        {
            ...DEFAULT_THEME,
            id: `${BUILTIN_PREFIX}default`,
            name: 'Default',
            origin: 'The stock r2modmac look',
        },
        ...THEME_PRESETS,
    ];
}
