/**
 * Theme engine.
 *
 * A theme is six semantic colours. The app, however, paints with ~34 palette
 * shades (gray/slate/blue ramps). This module expands the former into the
 * latter and writes the result to the CSS custom properties that
 * tailwind.config.js maps every colour utility onto.
 *
 * Expansion happens here, in one place, so the live editor preview and the
 * applied theme can never drift apart.
 */

export interface ThemeColors {
    /** App background — the largest surface. */
    background: string;
    /** Panels, cards, modals: the layer above the background. */
    surface: string;
    /** Hover state for panels, cards, list items and secondary buttons. */
    surface_hover?: string;
    /** Separators and outlines. */
    border: string;
    /** Primary text and icons. */
    text: string;
    /** Secondary text: descriptions, timestamps, hints. */
    text_muted: string;
    /** Buttons, links, selection, focus rings. */
    accent: string;
    /** Buttons and interactive elements when hovered. */
    accent_hover?: string;
    /** Destructive actions and error messages. */
    danger: string;
    /** Warnings and cautions. */
    warning: string;
    /** Confirmations and healthy states. */
    success: string;

    // ── Manual overrides ────────────────────────────────────────────────────
    // Only consulted when `autoContrast` is off. Optional so a theme that lets
    // the engine decide stays at nine colours rather than fifteen.

    /** Label on a primary button. */
    on_accent?: string;
    /** Label on a secondary (grey) button. */
    on_surface?: string;
    /** Label on a destructive button. */
    on_danger?: string;
    /** Label on a warning fill. */
    on_warning?: string;
    /** Label on a success fill. */
    on_success?: string;
    /** One colour for every decorative icon, replacing the adaptive hues. */
    icon?: string;

    // ── Cover chrome ────────────────────────────────────────────────────────
    // Badges that float on game artwork. They default to a fixed near-black
    // scrim with white icons, because the picture underneath is arbitrary and
    // following the palette turns them white on a light theme — but that is a
    // default, not a rule, so both are settable.

    /** Background of badges sitting on cover art. */
    media_scrim?: string;
    /** Icons and text on that scrim. */
    media_ink?: string;
}

/** Per-role transparency. Missing values are fully opaque for old themes. */
export type ThemeOpacity = Partial<Record<keyof ThemeColors, number>>;

/** Chrome that floats on artwork rather than on an app surface. */
export const COVER_COLOR_KEYS = ['media_scrim', 'media_ink'] as const;

export const DEFAULT_SCRIM = '#09090b';
export const DEFAULT_MEDIA_INK = '#ffffff';

/** Shown only when automatic colours are switched off. */
export const MANUAL_COLOR_KEYS = [
    'on_accent',
    'on_surface',
    'on_danger',
    'on_warning',
    'on_success',
    'icon',
] as const;

/** A picture behind the app, with the controls needed to keep text readable. */
export interface ThemeBackgroundImage {
    /** Path relative to the themes folder, so a theme stays shareable. */
    path: string;
    /** How strongly the picture shows through, 0–1. */
    opacity: number;
    /** Blur in pixels; softening busy artwork buys back legibility. */
    blur: number;
    /** How the image scales across the viewport: cover, contain, fill, tile, center. */
    fit?: 'cover' | 'contain' | 'fill' | 'tile' | 'center';
    /** Horizontal position/offset in percent (0–100, default 50). */
    offset_x?: number;
    /** Vertical position/offset in percent (0–100, default 50). */
    offset_y?: number;
    /** Tile size as a percentage of the viewport, for `fit: 'tile'`. */
    tile_scale?: number;
}

/** Behaviour switches, separate from the colours themselves. */
export interface ThemeOptions {
    /**
     * Let the engine pick label colours per filled control instead of using the
     * single text colour everywhere.
     *
     * On by default, because one text colour cannot read on both a pale confirm
     * button and a dark cancel button next to it. Turning it off hands complete
     * control back to whoever wants to place every colour by hand.
     */
    autoContrast: boolean;
}

export const DEFAULT_THEME_OPTIONS: ThemeOptions = { autoContrast: true };

export interface Theme {
    name: string;
    author?: string;
    colors: ThemeColors;
    opacity?: ThemeOpacity;
    backgroundImage?: ThemeBackgroundImage | null;
    options?: ThemeOptions;
}

export const THEME_COLOR_KEYS = [
    'background',
    'surface',
    'surface_hover',
    'border',
    'text',
    'text_muted',
    'accent',
    'accent_hover',
    'danger',
    'warning',
    'success',
] as const;

/** How the editor groups the colours, so related decisions sit together. */
export const THEME_COLOR_GROUPS: Array<{
    id: string;
    label: string;
    hint: string;
    keys: Array<keyof ThemeColors>;
}> = [
    {
        id: 'surfaces',
        label: 'Surfaces',
        hint: 'The layers the app is built from',
        keys: ['background', 'surface', 'surface_hover', 'border'],
    },
    {
        id: 'text',
        label: 'Text',
        hint: 'Everything you read',
        keys: ['text', 'text_muted'],
    },
    {
        id: 'accent',
        label: 'Accent',
        hint: 'What the app highlights & interactive states',
        keys: ['accent', 'accent_hover'],
    },
    {
        id: 'status',
        label: 'Status',
        hint: 'Errors, warnings and confirmations',
        keys: ['danger', 'warning', 'success'],
    },
];

/** Human-facing labels and help text for the editor. */
export const THEME_COLOR_META: Record<
    keyof ThemeColors,
    { label: string; description: string }
> = {
    background: { label: 'Background', description: 'The main app background' },
    surface: { label: 'Surface', description: 'Panels, cards and modals' },
    surface_hover: { label: 'Surface hover', description: 'Hover state for cards, rows & secondary buttons' },
    border: { label: 'Border', description: 'Separators and outlines' },
    text: { label: 'Text', description: 'Primary text and icons' },
    text_muted: { label: 'Muted text', description: 'Descriptions and hints' },
    accent: { label: 'Accent', description: 'Buttons, links and highlights' },
    accent_hover: { label: 'Accent hover', description: 'Primary button and link hover colour' },
    danger: { label: 'Danger', description: 'Errors and destructive actions' },
    warning: { label: 'Warning', description: 'Cautions and deprecation notices' },
    success: { label: 'Success', description: 'Confirmations and healthy states' },
    on_accent: { label: 'On primary button', description: 'Label sitting on the accent' },
    on_surface: { label: 'On secondary button', description: 'Label sitting on a grey fill' },
    on_danger: { label: 'On destructive button', description: 'Label sitting on the danger fill' },
    on_warning: { label: 'On warning', description: 'Label sitting on the warning fill' },
    on_success: { label: 'On success', description: 'Label sitting on the success fill' },
    icon: { label: 'Icons', description: 'One colour for every decorative icon' },
    media_scrim: { label: 'Badge background', description: 'Behind badges floating on cover art' },
    media_ink: { label: 'Badge icons', description: 'Icons and text on those badges' },
};

// ── Default ramps ────────────────────────────────────────────────────────────
// Tailwind's own values, mirrored from :root in index.css. They serve two
// purposes: they define the shape of a ramp (how lightness and chroma move from
// shade to shade), and they are the fallback when no theme is active.

const SHADES = [50, 100, 200, 300, 400, 500, 600, 700, 800, 900, 950] as const;
type Shade = (typeof SHADES)[number];
type Ramp = Record<Shade, string>;
type AlphaRamp = Record<Shade, number>;

const DEFAULT_GRAY: Ramp = {
    50: '#f9fafb', 100: '#f3f4f6', 200: '#e5e7eb', 300: '#d1d5db',
    400: '#9ca3af', 500: '#6b7280', 600: '#4b5563', 700: '#374151',
    800: '#1f2937', 900: '#111827', 950: '#030712',
};

const DEFAULT_SLATE: Ramp = {
    50: '#f8fafc', 100: '#f1f5f9', 200: '#e2e8f0', 300: '#cbd5e1',
    400: '#94a3b8', 500: '#64748b', 600: '#475569', 700: '#334155',
    800: '#1e293b', 900: '#0f172a', 950: '#020617',
};

const DEFAULT_BLUE: Ramp = {
    50: '#eff6ff', 100: '#dbeafe', 200: '#bfdbfe', 300: '#93c5fd',
    400: '#60a5fa', 500: '#3b82f6', 600: '#2563eb', 700: '#1d4ed8',
    800: '#1e40af', 900: '#1e3a8a', 950: '#172554',
};

const DEFAULT_RED: Ramp = {
    50: '#fef2f2', 100: '#fee2e2', 200: '#fecaca', 300: '#fca5a5',
    400: '#f87171', 500: '#ef4444', 600: '#dc2626', 700: '#b91c1c',
    800: '#991b1b', 900: '#7f1d1d', 950: '#450a0a',
};

const DEFAULT_AMBER: Ramp = {
    50: '#fffbeb', 100: '#fef3c7', 200: '#fde68a', 300: '#fcd34d',
    400: '#fbbf24', 500: '#f59e0b', 600: '#d97706', 700: '#b45309',
    800: '#92400e', 900: '#78350f', 950: '#451a03',
};

const DEFAULT_YELLOW: Ramp = {
    50: '#fefce8', 100: '#fef9c3', 200: '#fef08a', 300: '#fde047',
    400: '#facc15', 500: '#eab308', 600: '#ca8a04', 700: '#a16207',
    800: '#854d0e', 900: '#713f12', 950: '#422006',
};

const DEFAULT_GREEN: Ramp = {
    50: '#f0fdf4', 100: '#dcfce7', 200: '#bbf7d0', 300: '#86efac',
    400: '#4ade80', 500: '#22c55e', 600: '#16a34a', 700: '#15803d',
    800: '#166534', 900: '#14532d', 950: '#052e16',
};

const DEFAULT_EMERALD: Ramp = {
    50: '#ecfdf5', 100: '#d1fae5', 200: '#a7f3d0', 300: '#6ee7b7',
    400: '#34d399', 500: '#10b981', 600: '#059669', 700: '#047857',
    800: '#065f46', 900: '#064e3b', 950: '#022c22',
};

/**
 * Families used purely to tell one icon from another — the coloured glyphs in
 * Preferences, for instance. No theme colour drives them: their whole job is to
 * stay distinguishable from each other, so their hues are fixed.
 *
 * What does adapt is their lightness. A palette picked to read on a dark panel
 * is washed out on a light one, so under a light theme each ramp is mirrored:
 * the shade a component asks for keeps its hue and gains the lightness that
 * suits the surface. Same idea as a status-bar glyph flipping between light and
 * dark depending on what is behind it.
 */
const DECORATIVE_RAMPS: Record<string, Ramp> = {
    purple: {
        50: '#faf5ff', 100: '#f3e8ff', 200: '#e9d5ff', 300: '#d8b4fe',
        400: '#c084fc', 500: '#a855f7', 600: '#9333ea', 700: '#7e22ce',
        800: '#6b21a8', 900: '#581c87', 950: '#3b0764',
    },
    cyan: {
        50: '#ecfeff', 100: '#cffafe', 200: '#a5f3fc', 300: '#67e8f9',
        400: '#22d3ee', 500: '#06b6d4', 600: '#0891b2', 700: '#0e7490',
        800: '#155e75', 900: '#164e63', 950: '#083344',
    },
    violet: {
        50: '#f5f3ff', 100: '#ede9fe', 200: '#ddd6fe', 300: '#c4b5fd',
        400: '#a78bfa', 500: '#8b5cf6', 600: '#7c3aed', 700: '#6d28d9',
        800: '#5b21b6', 900: '#4c1d95', 950: '#2e1065',
    },
    sky: {
        50: '#f0f9ff', 100: '#e0f2fe', 200: '#bae6fd', 300: '#7dd3fc',
        400: '#38bdf8', 500: '#0ea5e9', 600: '#0284c7', 700: '#0369a1',
        800: '#075985', 900: '#0c4a6e', 950: '#082f49',
    },
    indigo: {
        50: '#eef2ff', 100: '#e0e7ff', 200: '#c7d2fe', 300: '#a5b4fc',
        400: '#818cf8', 500: '#6366f1', 600: '#4f46e5', 700: '#4338ca',
        800: '#3730a3', 900: '#312e81', 950: '#1e1b4b',
    },
    fuchsia: {
        50: '#fdf4ff', 100: '#fae8ff', 200: '#f5d0fe', 300: '#f0abfc',
        400: '#e879f9', 500: '#d946ef', 600: '#c026d3', 700: '#a21caf',
        800: '#86198f', 900: '#701a75', 950: '#4a044e',
    },
    rose: {
        50: '#fff1f2', 100: '#ffe4e6', 200: '#fecdd3', 300: '#fda4af',
        400: '#fb7185', 500: '#f43f5e', 600: '#e11d48', 700: '#be123c',
        800: '#9f1239', 900: '#881337', 950: '#4c0519',
    },
    orange: {
        50: '#fff7ed', 100: '#ffedd5', 200: '#fed7aa', 300: '#fdba74',
        400: '#fb923c', 500: '#f97316', 600: '#ea580c', 700: '#c2410c',
        800: '#9a3412', 900: '#7c2d12', 950: '#431407',
    },
    teal: {
        50: '#f0fdfa', 100: '#ccfbf1', 200: '#99f6e4', 300: '#5eead4',
        400: '#2dd4bf', 500: '#14b8a6', 600: '#0d9488', 700: '#0f766e',
        800: '#115e59', 900: '#134e4a', 950: '#042f2e',
    },
    pink: {
        50: '#fdf2f8', 100: '#fce7f3', 200: '#fbcfe8', 300: '#f9a8d4',
        400: '#f472b6', 500: '#ec4899', 600: '#db2777', 700: '#be185d',
        800: '#9d174d', 900: '#831843', 950: '#500724',
    },
    lime: {
        50: '#f7fee7', 100: '#ecfccb', 200: '#d9f99d', 300: '#bef264',
        400: '#a3e635', 500: '#84cc16', 600: '#65a30d', 700: '#4d7c0f',
        800: '#3f6212', 900: '#365314', 950: '#1a2e05',
    },
};

export const DECORATIVE_FAMILIES = Object.keys(DECORATIVE_RAMPS);

/** The shade icons are drawn at; the ramp is aligned so this one reads. */
const ICON_SHADE: Shade = 400;
const ICON_MIN_CONTRAST = 3.5;

/**
 * Slide a ramp along itself until its icon shade stands out from `backdrop`.
 *
 * Mirroring the ramp for light themes was the obvious move and is not enough:
 * some families are simply weak at that lightness whichever way you flip them —
 * lime against a pale box, indigo against a dark one both land near 2:1. So
 * rather than assume an offset, each family is walked outward from its usual
 * shade until one actually clears the bar, and the whole ramp moves with it.
 * The hue never changes, so an icon keeps its identity; only its lightness
 * adapts, the way a status-bar glyph flips to suit what is behind it.
 */
function alignRampToBackdrop(reference: Ramp, backdrop: string): Ramp {
    const home = SHADES.indexOf(ICON_SHADE);

    let offset = 0;
    let bestOffset = 0;
    let bestRatio = 0;
    // Alternate outward — 0, +1, -1, +2, -2 … — so the nearest shade that works
    // wins and the ramp shifts as little as possible.
    for (let step = 0; step < SHADES.length * 2; step++) {
        const delta = step === 0 ? 0 : (step % 2 === 1 ? (step + 1) / 2 : -step / 2);
        const index = home + delta;
        if (index < 0 || index >= SHADES.length) continue;
        const ratio = contrastRatio(reference[SHADES[index]], backdrop);
        if (ratio >= ICON_MIN_CONTRAST) { offset = delta; bestRatio = ratio; break; }
        if (ratio > bestRatio) { bestRatio = ratio; bestOffset = delta; }
    }
    if (bestRatio < ICON_MIN_CONTRAST) offset = bestOffset;

    if (offset === 0) return { ...reference };

    const out = {} as Ramp;
    for (const shade of SHADES) {
        const index = clamp(SHADES.indexOf(shade) + offset, 0, SHADES.length - 1);
        out[shade] = reference[SHADES[index]];
    }
    return out;
}

/** The stock look, expressed as a theme. Also the starting point for a new one. */
export const DEFAULT_THEME: Theme = {
    name: 'Default',
    colors: {
        background: DEFAULT_GRAY[900],
        surface: DEFAULT_GRAY[800],
        surface_hover: DEFAULT_GRAY[700],
        border: DEFAULT_GRAY[700],
        text: '#ffffff',
        text_muted: DEFAULT_GRAY[400],
        accent: DEFAULT_BLUE[500],
        accent_hover: DEFAULT_BLUE[600],
        danger: DEFAULT_RED[500],
        warning: DEFAULT_AMBER[500],
        success: DEFAULT_GREEN[500],
    },
};

// ── Colour space ─────────────────────────────────────────────────────────────
// Interpolating in sRGB muddies dark ramps; OKLab is perceptually uniform, so
// midpoints between two theme anchors land where the eye expects them.

interface Rgb { r: number; g: number; b: number }
interface Lab { L: number; a: number; b: number }

export function parseHex(hex: string): Rgb | null {
    const m = /^#?([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(hex.trim());
    if (!m) return null;
    let h = m[1];
    if (h.length === 3) h = h[0] + h[0] + h[1] + h[1] + h[2] + h[2];
    const n = parseInt(h, 16);
    return { r: (n >> 16) & 255, g: (n >> 8) & 255, b: n & 255 };
}

export function isValidHex(hex: string): boolean {
    return parseHex(hex) !== null;
}

function toHex({ r, g, b }: Rgb): string {
    const c = (v: number) => Math.round(clamp(v, 0, 255)).toString(16).padStart(2, '0');
    return `#${c(r)}${c(g)}${c(b)}`;
}

function clamp(v: number, lo: number, hi: number): number {
    return v < lo ? lo : v > hi ? hi : v;
}

const srgbToLinear = (c: number) =>
    c <= 0.04045 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);

const linearToSrgb = (c: number) =>
    c <= 0.0031308 ? 12.92 * c : 1.055 * Math.pow(c, 1 / 2.4) - 0.055;

function rgbToLab({ r, g, b }: Rgb): Lab {
    const lr = srgbToLinear(r / 255);
    const lg = srgbToLinear(g / 255);
    const lb = srgbToLinear(b / 255);

    const l = Math.cbrt(0.4122214708 * lr + 0.5363325363 * lg + 0.0514459929 * lb);
    const m = Math.cbrt(0.2119034982 * lr + 0.6806995451 * lg + 0.1073969566 * lb);
    const s = Math.cbrt(0.0883024619 * lr + 0.2817188376 * lg + 0.6299787005 * lb);

    return {
        L: 0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
        a: 1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
        b: 0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
    };
}

/** Unclamped conversion — channels may fall outside 0..255 when out of gamut. */
function labToRgbRaw({ L, a, b }: Lab): Rgb {
    const l_ = L + 0.3963377774 * a + 0.2158037573 * b;
    const m_ = L - 0.1055613458 * a - 0.0638541728 * b;
    const s_ = L - 0.0894841775 * a - 1.2914855480 * b;

    const l = l_ * l_ * l_;
    const m = m_ * m_ * m_;
    const s = s_ * s_ * s_;

    const lr = 4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s;
    const lg = -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s;
    const lb = -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s;

    return {
        r: linearToSrgb(lr) * 255,
        g: linearToSrgb(lg) * 255,
        b: linearToSrgb(lb) * 255,
    };
}

function labToRgb(lab: Lab): Rgb {
    const { r, g, b } = labToRgbRaw(lab);
    return { r: clamp(r, 0, 255), g: clamp(g, 0, 255), b: clamp(b, 0, 255) };
}

// Cylindrical form of OKLab. Hue as an angle is what lets an accent ramp keep
// one colour identity while lightness and saturation vary across shades.
interface Lch { L: number; C: number; h: number }

function labToLch({ L, a, b }: Lab): Lch {
    return { L, C: Math.hypot(a, b), h: Math.atan2(b, a) };
}

function lchToLab({ L, C, h }: Lch): Lab {
    return { L, a: C * Math.cos(h), b: C * Math.sin(h) };
}

/** Wrap a hue difference into (-π, π] so it reads as a drift, not a long way round. */
function signedAngle(delta: number): number {
    const TAU = Math.PI * 2;
    return ((((delta + Math.PI) % TAU) + TAU) % TAU) - Math.PI;
}

function inGamut({ r, g, b }: Rgb): boolean {
    const ok = (v: number) => v >= -0.5 && v <= 255.5;
    return ok(r) && ok(g) && ok(b);
}

/**
 * Pull a colour into sRGB by desaturating rather than clamping.
 *
 * Clamping channels independently drags the hue (a too-saturated blue clips to
 * purple); reducing chroma keeps the hue and lightness the user chose.
 */
function fitToGamut(lch: Lch): Lch {
    if (inGamut(labToRgbRaw(lchToLab(lch)))) return lch;
    let lo = 0;
    let hi = lch.C;
    for (let i = 0; i < 20; i++) {
        const mid = (lo + hi) / 2;
        if (inGamut(labToRgbRaw(lchToLab({ ...lch, C: mid })))) lo = mid;
        else hi = mid;
    }
    return { ...lch, C: lo };
}

function mixLab(x: Lab, y: Lab, t: number): Lab {
    return {
        L: x.L + (y.L - x.L) * t,
        a: x.a + (y.a - x.a) * t,
        b: x.b + (y.b - x.b) * t,
    };
}

// ── Ramp expansion ───────────────────────────────────────────────────────────

/**
 * Build a full ramp from a few anchored shades.
 *
 * Anchors pin specific shades to the theme's colours; the rest are found by
 * interpolating between them. The interpolation parameter is each shade's
 * lightness in the *default* ramp, which keeps the spacing of the original
 * palette — shades stay as far apart as they always were, just in new colours.
 *
 * Shades outside the anchored span (only 950, darker than the darkest anchor)
 * extrapolate along the nearest segment, then clamp into sRGB.
 */
function expandRamp(reference: Ramp, anchors: Partial<Record<Shade, string>>): Ramp {
    const anchored = (Object.keys(anchors) as unknown as string[])
        .map((k) => Number(k) as Shade)
        .filter((s) => isValidHex(anchors[s]!))
        .map((shade) => ({
            shade,
            refL: rgbToLab(parseHex(reference[shade])!).L,
            lab: rgbToLab(parseHex(anchors[shade]!)!),
        }))
        .sort((x, y) => x.refL - y.refL);

    if (anchored.length < 2) return { ...reference };

    const out = {} as Ramp;
    for (const shade of SHADES) {
        const refL = rgbToLab(parseHex(reference[shade])!).L;

        // Locate the segment covering this shade (or the nearest one to extend).
        let lo = 0;
        while (lo < anchored.length - 2 && refL > anchored[lo + 1].refL) lo++;
        const a = anchored[lo];
        const b = anchored[lo + 1];

        const span = b.refL - a.refL;
        const t = span === 0 ? 0 : (refL - a.refL) / span;
        out[shade] = toHex(labToRgb(mixLab(a.lab, b.lab, t)));
    }
    return out;
}

/** Match opacity to the same perceptual shade spacing used by a colour ramp. */
function expandAlphaRamp(reference: Ramp, anchors: Partial<Record<Shade, number>>): AlphaRamp {
    const anchored = (Object.keys(anchors) as unknown as string[])
        .map((key) => Number(key) as Shade)
        .filter((shade) => typeof anchors[shade] === 'number')
        .map((shade) => ({
            shade,
            refL: rgbToLab(parseHex(reference[shade])!).L,
            alpha: clamp(anchors[shade]!, 0, 1),
        }))
        .sort((a, b) => a.refL - b.refL);
    if (anchored.length < 2) {
        const alpha = anchored[0]?.alpha ?? 1;
        return Object.fromEntries(SHADES.map((shade) => [shade, alpha])) as AlphaRamp;
    }
    const out = {} as AlphaRamp;
    for (const shade of SHADES) {
        const refL = rgbToLab(parseHex(reference[shade])!).L;
        let lo = 0;
        while (lo < anchored.length - 2 && refL > anchored[lo + 1].refL) lo++;
        const a = anchored[lo];
        const b = anchored[lo + 1];
        const span = b.refL - a.refL;
        const t = span === 0 ? 0 : (refL - a.refL) / span;
        out[shade] = clamp(a.alpha + (b.alpha - a.alpha) * t, 0, 1);
    }
    return out;
}

const flatAlphaRamp = (alpha: number): AlphaRamp =>
    Object.fromEntries(SHADES.map((shade) => [shade, clamp(alpha, 0, 1)])) as AlphaRamp;

/**
 * Rebuild the accent ramp around a single colour.
 *
 * The accent is pinned at shade 500 — where the app's `blue-500` lives — and
 * every other shade borrows the reference ramp's *relative* lightness step,
 * chroma ratio and hue drift, re-centred on the accent. Scaling chroma rather
 * than holding it fixed is what keeps pale shades from drifting toward cyan;
 * carrying the hue drift reproduces the slight rotation a good ramp has between
 * its light and dark ends instead of flattening it to one hue.
 */
function expandAccentRamp(reference: Ramp, accent: string, pivot: Ramp = reference): Ramp {
    const parsed = parseHex(accent);
    if (!parsed) return { ...reference };

    const acc = labToLch(rgbToLab(parsed));
    // `pivot` is the ramp the anchor colour actually names. It differs from
    // `reference` for families that shadow another one — yellow trailing amber,
    // emerald trailing green — so they keep their own distance from it instead
    // of collapsing onto the same colour.
    const ref500 = labToLch(rgbToLab(parseHex(pivot[500])!));
    const chromaScale = ref500.C === 0 ? 0 : acc.C / ref500.C;

    const out = {} as Ramp;
    for (const shade of SHADES) {
        const ref = labToLch(rgbToLab(parseHex(reference[shade])!));
        out[shade] = toHex(
            labToRgb(
                lchToLab(
                    fitToGamut({
                        L: clamp(acc.L + (ref.L - ref500.L), 0, 1),
                        C: ref.C * chromaScale,
                        h: acc.h + signedAngle(ref.h - ref500.h),
                    })
                )
            )
        );
    }
    return out;
}

/**
 * Keep every decorative hue, but give it the lightness the surface calls for.
 *
 * On a dark theme the stock ramps already read well, so they pass through
 * untouched and nothing moves. On a light theme each is mirrored, so a glyph
 * asking for `text-purple-400` gets a purple dark enough to see instead of the
 * pale one meant for a dark panel.
 */
function resolveDecorative(backdrop: string): Record<string, Ramp> {
    const out: Record<string, Ramp> = {};
    for (const [family, reference] of Object.entries(DECORATIVE_RAMPS)) {
        out[family] = alignRampToBackdrop(reference, backdrop);
    }
    return out;
}

export interface ResolvedPalette {
    gray: Ramp;
    slate: Ramp;
    blue: Ramp;
    red: Ramp;
    amber: Ramp;
    yellow: Ramp;
    green: Ramp;
    emerald: Ramp;
    white: string;
    /** Label colour for anything sitting on an accent fill. */
    onAccent: string;
    /**
     * Label colours for the other filled controls. A theme has one text colour,
     * but it has to survive on a grey secondary button and a red destructive
     * one too, so each fill gets the readable end of the palette.
     */
    on: {
        accent: string;
        surface: string;
        danger: string;
        warning: string;
        success: string;
    };
    /** Fixed-hue icon families, lightness-matched to the surface. */
    decorative: Record<string, Ramp>;
    /** Chrome over artwork: scrim fill and the ink on it. */
    media: { scrim: string; ink: string };
    /** Explicit button/interactive hover accent. */
    accentHover: string;
    /** Explicit card/panel/row hover surface. */
    surfaceHover: string;
    /**
     * Status colours picked to be *readable as text on a panel*, rather than a
     * fixed shade.
     *
     * A shade number cannot do this job on its own: `text-amber-200` is chosen
     * assuming a dark panel, and under a light theme the ramp puts it at pale
     * yellow on near-white — around 1.6:1. These pick whichever end of the
     * family actually contrasts with the theme's surface.
     */
    fg: {
        accent: string;
        danger: string;
        warning: string;
        success: string;
    };
    /** Alpha counterparts for every runtime colour token. */
    alpha: {
        gray: AlphaRamp;
        slate: AlphaRamp;
        blue: AlphaRamp;
        red: AlphaRamp;
        amber: AlphaRamp;
        yellow: AlphaRamp;
        green: AlphaRamp;
        emerald: AlphaRamp;
        white: number;
        on: Record<'accent' | 'surface' | 'danger' | 'warning' | 'success', number>;
        media: { scrim: number; ink: number };
        accentHover: number;
        surfaceHover: number;
        fg: Record<'accent' | 'danger' | 'warning' | 'success', number>;
        decorative: number;
    };
}

/**
 * The shade of a family that reads as text on `surface`.
 *
 * Candidates are tried from the shades a dark UI would reach for outward to
 * their darker counterparts, so a dark theme keeps the look it has today and a
 * light theme gets the mirror-image choice. If nothing clears the bar — a very
 * pale surface with a very pale accent — the best available is used, because a
 * dim label still beats an invisible one.
 */
function readableOnSurface(ramp: Ramp, surface: string): string {
    const candidates: Shade[] = [400, 300, 500, 600, 700, 200, 800, 100, 900];
    let best = ramp[400];
    let bestRatio = 0;
    for (const shade of candidates) {
        const ratio = contrastRatio(ramp[shade], surface);
        if (ratio >= 4.5) return ramp[shade];
        if (ratio > bestRatio) {
            bestRatio = ratio;
            best = ramp[shade];
        }
    }
    return best;
}

/**
 * Pick the colour for a label painted directly on a filled control.
 *
 * A theme carries one text colour, but text lands on several different fills —
 * the page, a grey secondary button, a red destructive one, the accent. One
 * colour cannot read on all of them: choose something dark enough for a pale
 * confirm button and it disappears on a dark cancel button beside it.
 *
 * So the label is chosen per fill, from the theme's own two extremes. The
 * lighter of `text`/`background` is preferred, which is what keeps the stock
 * white-on-blue-500 button (3.7:1) exactly as it was; only when that drops
 * below the 3:1 bar for bold UI text does it flip to the darker one. Both
 * candidates come from the theme, so this never invents a colour the user did
 * not pick — and when `autoContrast` is off it does nothing at all, leaving the
 * single text colour in charge everywhere.
 */
export function resolveOnFill(
    colors: ThemeColors,
    fill: string,
    autoContrast = true
): string {
    if (!autoContrast) return colors.text;
    const textIsLighter = luminance(colors.text) > luminance(colors.background);
    const lightColor = textIsLighter ? colors.text : colors.background;
    const darkColor = textIsLighter ? colors.background : colors.text;

    // The light label is preferred: a filled, saturated button carries light
    // text in light and dark themes alike, and a set of buttons that each
    // solved its own contrast problem in isolation would not look like a set.
    //
    // The floor sits at 2.6 rather than the 3:1 of the bold-text guideline. At
    // 3 the rule flipped on hairline differences — a coral accent measuring
    // 2.81 fell to near-black lettering, which reads as a mistake however it
    // scores. Below 2.6 the light label really is lost, and it gives way.
    return contrastRatio(lightColor, fill) >= 2.6 ? lightColor : darkColor;
}

/**
 * The label for one fill: derived while automatic colours are on, taken from
 * the theme's own override once they are off.
 *
 * The override falls back to the plain text colour rather than to a derived
 * value, so switching automatic off gives exactly the old single-colour
 * behaviour until the user actually sets something.
 */
function pickLabel(
    colors: ThemeColors,
    key: 'on_accent' | 'on_surface' | 'on_danger' | 'on_warning' | 'on_success',
    fill: string,
    auto: boolean
): string {
    if (auto) return resolveOnFill(colors, fill, true);
    const manual = colors[key];
    return manual && isValidHex(manual) ? normalizeHex(manual) : colors.text;
}

/**
 * Paint every decorative family in one colour.
 *
 * Manual mode trades the per-icon hues for a single choice — which is the whole
 * point of turning the automatic behaviour off. The ramp is still generated
 * around it so shade modifiers keep working.
 */
function tintDecorative(icon: string): Record<string, Ramp> {
    const out: Record<string, Ramp> = {};
    for (const family of Object.keys(DECORATIVE_RAMPS)) {
        out[family] = expandAccentRamp(DECORATIVE_RAMPS[family], icon);
    }
    return out;
}

/** Back-compat shorthand for the accent, the most common fill. */
export function resolveOnAccent(colors: ThemeColors, autoContrast = true): string {
    return resolveOnFill(colors, colors.accent, autoContrast);
}

/**
 * Expanding a theme costs ~90 OKLab conversions plus a gamut search per shade.
 * That is nothing once, but the editor re-resolves on every pointer move while
 * a colour is being dragged, so the last few results are kept. Keyed by the
 * colours alone — nothing else in a theme affects the palette.
 */
const paletteCache = new Map<string, ResolvedPalette>();
const PALETTE_CACHE_LIMIT = 8;

/** Expand a theme's colours into every shade the app paints with. */
export function resolveTheme(theme: Theme): ResolvedPalette {
    const key = [
        ...THEME_COLOR_KEYS.map((k) => theme.colors[k] || ''),
        ...MANUAL_COLOR_KEYS.map((k) => theme.colors[k] || ''),
        ...COVER_COLOR_KEYS.map((k) => theme.colors[k] || ''),
        ...[...THEME_COLOR_KEYS, ...MANUAL_COLOR_KEYS, ...COVER_COLOR_KEYS].map(
            (k) => theme.opacity?.[k] ?? 1
        ),
        theme.options?.autoContrast ?? DEFAULT_THEME_OPTIONS.autoContrast,
    ].join('|');
    const cached = paletteCache.get(key);
    if (cached) return cached;

    const resolved = computePalette(theme);
    if (paletteCache.size >= PALETTE_CACHE_LIMIT) {
        // Oldest first: a drag walks through colours and never revisits them,
        // so the recent entries are the ones worth keeping.
        paletteCache.delete(paletteCache.keys().next().value!);
    }
    paletteCache.set(key, resolved);
    return resolved;
}

function deriveHoverColor(baseHex: string, isAccent = true): string {
    const parsed = parseHex(baseHex);
    if (!parsed) return baseHex;
    const lab = rgbToLab(parsed);
    // If base is light (L > 0.65), make hover slightly darker.
    // If base is dark (L <= 0.65), make hover slightly lighter.
    const deltaL = lab.L > 0.65 ? (isAccent ? -0.10 : -0.06) : (isAccent ? 0.08 : 0.05);
    const newL = clamp(lab.L + deltaL, 0.05, 0.95);
    return toHex(labToRgb({ L: newL, a: lab.a, b: lab.b }));
}

function computePalette(theme: Theme): ResolvedPalette {
    const c = theme.colors;
    const opacity = (key: keyof ThemeColors) => clamp(theme.opacity?.[key] ?? 1, 0, 1);

    // The gray ramp runs from the background (darkest surface) up through the
    // muted text to the primary text at its lightest end.
    const grayAnchors: Partial<Record<Shade, string>> = {
        900: c.background,
        800: c.surface,
        700: c.border,
        400: c.text_muted,
        50: c.text,
    };

    const auto = theme.options?.autoContrast ?? DEFAULT_THEME_OPTIONS.autoContrast;
    const gray = expandRamp(DEFAULT_GRAY, grayAnchors);
    const grayAlpha = expandAlphaRamp(DEFAULT_GRAY, {
        900: opacity('background'),
        800: opacity('surface'),
        700: opacity('border'),
        400: opacity('text_muted'),
        50: opacity('text'),
    });
    const blue = expandAccentRamp(DEFAULT_BLUE, c.accent);
    const red = expandAccentRamp(DEFAULT_RED, c.danger);
    const amber = expandAccentRamp(DEFAULT_AMBER, c.warning);
    const green = expandAccentRamp(DEFAULT_GREEN, c.success);

    const accentHover = c.accent_hover && isValidHex(c.accent_hover)
        ? normalizeHex(c.accent_hover)
        : deriveHoverColor(c.accent, true);

    const surfaceHover = c.surface_hover && isValidHex(c.surface_hover)
        ? normalizeHex(c.surface_hover)
        : deriveHoverColor(c.surface, false);

    return {
        gray,
        // Slate mirrors gray so the few slate-built screens follow the theme.
        slate: expandRamp(DEFAULT_SLATE, grayAnchors),
        blue,
        red,
        amber,
        // Yellow and emerald shadow warning and success. Passing the pivot ramp
        // keeps their hue offset from the family they follow, so a themed
        // yellow stays recognisably not-amber.
        yellow: expandAccentRamp(DEFAULT_YELLOW, c.warning, DEFAULT_AMBER),
        green,
        emerald: expandAccentRamp(DEFAULT_EMERALD, c.success, DEFAULT_GREEN),
        white: c.text,
        media: {
            scrim: c.media_scrim && isValidHex(c.media_scrim) ? normalizeHex(c.media_scrim) : DEFAULT_SCRIM,
            ink: c.media_ink && isValidHex(c.media_ink) ? normalizeHex(c.media_ink) : DEFAULT_MEDIA_INK,
        },
        onAccent: pickLabel(c, 'on_accent', c.accent, auto),
        on: {
            accent: pickLabel(c, 'on_accent', c.accent, auto),
            // Secondary buttons use the surface as their fill. Border is only
            // an outline token and must never decide a button's label colour.
            surface: pickLabel(c, 'on_surface', c.surface, auto),
            danger: pickLabel(c, 'on_danger', red[600], auto),
            warning: pickLabel(c, 'on_warning', amber[600], auto),
            success: pickLabel(c, 'on_success', green[600], auto),
        },
        decorative:
            !auto && c.icon && isValidHex(c.icon)
                ? tintDecorative(c.icon)
                : resolveDecorative(gray[700]),
        accentHover,
        surfaceHover,
        fg: {
            accent: readableOnSurface(blue, c.surface),
            danger: readableOnSurface(red, c.surface),
            warning: readableOnSurface(amber, c.surface),
            success: readableOnSurface(green, c.surface),
        },
        alpha: {
            gray: grayAlpha,
            slate: expandAlphaRamp(DEFAULT_SLATE, {
                900: opacity('background'),
                800: opacity('surface'),
                700: opacity('border'),
                400: opacity('text_muted'),
                50: opacity('text'),
            }),
            blue: flatAlphaRamp(opacity('accent')),
            red: flatAlphaRamp(opacity('danger')),
            amber: flatAlphaRamp(opacity('warning')),
            yellow: flatAlphaRamp(opacity('warning')),
            green: flatAlphaRamp(opacity('success')),
            emerald: flatAlphaRamp(opacity('success')),
            white: opacity('text'),
            on: {
                accent: auto ? 1 : opacity('on_accent'),
                surface: auto ? 1 : opacity('on_surface'),
                danger: auto ? 1 : opacity('on_danger'),
                warning: auto ? 1 : opacity('on_warning'),
                success: auto ? 1 : opacity('on_success'),
            },
            media: { scrim: opacity('media_scrim'), ink: opacity('media_ink') },
            accentHover: opacity(c.accent_hover ? 'accent_hover' : 'accent'),
            surfaceHover: opacity(c.surface_hover ? 'surface_hover' : 'surface'),
            fg: {
                accent: opacity('accent'),
                danger: opacity('danger'),
                warning: opacity('warning'),
                success: opacity('success'),
            },
            decorative: !auto && c.icon ? opacity('icon') : 1,
        },
    };
}

// ── Readability ──────────────────────────────────────────────────────────────

/** WCAG relative luminance. */
function luminance(hex: string): number {
    const { r, g, b } = parseHex(hex)!;
    const ch = (v: number) => srgbToLinear(v / 255);
    return 0.2126 * ch(r) + 0.7152 * ch(g) + 0.0722 * ch(b);
}

/** WCAG contrast ratio, 1 (identical) to 21 (black on white). */
export function contrastRatio(a: string, b: string): number {
    if (!isValidHex(a) || !isValidHex(b)) return 1;
    const la = luminance(a);
    const lb = luminance(b);
    const [hi, lo] = la > lb ? [la, lb] : [lb, la];
    return (hi + 0.05) / (lo + 0.05);
}

export interface ContrastWarning {
    pair: string;
    ratio: number;
}

/**
 * Text/background pairings that would be hard to read.
 *
 * Advisory, never blocking — it is the user's app, and someone may want a
 * low-contrast look on purpose. The threshold is WCAG AA for body text (4.5),
 * relaxed to 3 for muted text, which is secondary by design.
 */
export function findContrastWarnings(colors: ThemeColors): ContrastWarning[] {
    const checks: Array<[string, string, string, number]> = [
        ['Text on background', colors.text, colors.background, 4.5],
        ['Text on surface', colors.text, colors.surface, 4.5],
        ['Muted text on background', colors.text_muted, colors.background, 3],
        ['Accent on background', colors.accent, colors.background, 3],
        // The root cause behind most of the pairs above: if the page and the
        // panels sit at opposite ends of the scale, no single text colour can
        // serve both, and no amount of automatic label picking can rescue it.
        // Reported as its own line so the fix is obvious — move one of them.
        ['Background vs surface', colors.background, colors.surface, 0],
    ];
    const warnings = checks
        .filter(([, , , min]) => min > 0)
        .map(([pair, fg, bg, min]) => ({ pair, ratio: contrastRatio(fg, bg), min }))
        .filter(({ ratio, min }) => ratio < min)
        .map(({ pair, ratio }) => ({ pair, ratio }));

    // Panels and page more than a few steps apart cannot share a text colour.
    // 4.5 between them means one of the two is guaranteed to fight the text.
    const spread = contrastRatio(colors.background, colors.surface);
    if (spread >= 4.5) {
        warnings.unshift({ pair: 'Background and surface are too far apart', ratio: spread });
    }

    return warnings;
}

// ── Application ──────────────────────────────────────────────────────────────

const VAR_PREFIX = '--r2-';

/** Palette families the theme engine drives. Mirrors tailwind.config.js. */
export const THEMED_FAMILIES = [
    'gray', 'slate', 'blue', 'red', 'amber', 'yellow', 'green', 'emerald',
] as const;

function channels(hex: string): string {
    const rgb = parseHex(hex)!;
    return `${Math.round(rgb.r)} ${Math.round(rgb.g)} ${Math.round(rgb.b)}`;
}

/** Every custom property the theme engine controls. */
export function paletteVars(p: ResolvedPalette): Record<string, string> {
    const vars: Record<string, string> = {};
    for (const family of THEMED_FAMILIES) {
        for (const shade of SHADES) {
            vars[`${VAR_PREFIX}${family}-${shade}`] = channels(p[family][shade]);
            vars[`${VAR_PREFIX}${family}-${shade}-alpha`] = String(p.alpha[family][shade]);
        }
    }
    vars[`${VAR_PREFIX}white`] = channels(p.white);
    vars[`${VAR_PREFIX}white-alpha`] = String(p.alpha.white);
    vars[`${VAR_PREFIX}scrim`] = channels(p.media.scrim);
    vars[`${VAR_PREFIX}scrim-alpha`] = String(p.alpha.media.scrim);
    vars[`${VAR_PREFIX}on-media`] = channels(p.media.ink);
    vars[`${VAR_PREFIX}on-media-alpha`] = String(p.alpha.media.ink);
    for (const [fill, value] of Object.entries(p.on)) {
        vars[`${VAR_PREFIX}on-${fill}`] = channels(value);
        vars[`${VAR_PREFIX}on-${fill}-alpha`] = String(p.alpha.on[fill as keyof typeof p.alpha.on]);
    }
    vars[`${VAR_PREFIX}accent-hover`] = channels(p.accentHover);
    vars[`${VAR_PREFIX}accent-hover-alpha`] = String(p.alpha.accentHover);
    vars[`${VAR_PREFIX}surface-hover`] = channels(p.surfaceHover);
    vars[`${VAR_PREFIX}surface-hover-alpha`] = String(p.alpha.surfaceHover);
    for (const [role, value] of Object.entries(p.fg)) {
        vars[`${VAR_PREFIX}fg-${role}`] = channels(value);
        vars[`${VAR_PREFIX}fg-${role}-alpha`] = String(p.alpha.fg[role as keyof typeof p.alpha.fg]);
    }
    for (const [family, ramp] of Object.entries(p.decorative)) {
        for (const shade of SHADES) {
            vars[`${VAR_PREFIX}${family}-${shade}`] = channels(ramp[shade]);
            vars[`${VAR_PREFIX}${family}-${shade}-alpha`] = String(p.alpha.decorative);
        }
    }
    return vars;
}

/**
 * Paint a theme onto the document, or clear back to the stock palette.
 *
 * Writing to the root element's inline style overrides the `:root` defaults in
 * index.css; clearing them lets those defaults take over again, which is why
 * "no theme" costs nothing and can never be half-applied.
 */
export interface StyleTarget {
    style: {
        setProperty(name: string, value: string): void;
        removeProperty(name: string): void;
    };
    /** Present on real elements; optional so tests can pass a plain object. */
    setAttribute?(name: string, value: string): void;
    removeAttribute?(name: string): void;
}

/**
 * A theme's variables as a plain style object.
 *
 * Applying these to any element scopes the theme to that subtree, which is how
 * the editor shows an unsaved draft: every specimen inside resolves against the
 * draft through the same tokens the real app uses, so nothing can be live in one
 * corner and stale in another.
 */
export function themeStyleVariables(theme: Theme): Record<string, string> {
    return paletteVars(resolveTheme(theme));
}


/**
 * How the background picture is laid out, for a given image setting.
 *
 * The one description of it. The editor's full-screen preview used to work this
 * out again on its own, and the two drifted: the preview kept the blur overscan
 * switched on for every mode, so a "contain" picture was quietly enlarged past
 * the point where it fits, and it never clamped the pattern scale. A preview
 * that lays the picture out differently from the window it is previewing is
 * worse than no preview.
 */
export function backgroundLayerStyle(image: ThemeBackgroundImage): {
    backgroundSize: string;
    backgroundRepeat: string;
    backgroundPosition: string;
    filter: string;
    /** Multiplier for the layer's transform, as a bare number. */
    scale: string;
} {
    const fit = image.fit || 'cover';

    let backgroundSize = 'cover';
    let backgroundRepeat = 'no-repeat';
    if (fit === 'contain') backgroundSize = 'contain';
    else if (fit === 'fill') backgroundSize = '100% 100%';
    else if (fit === 'tile') {
        // A percentage size is what makes the pattern scalable; `auto` would
        // pin it to the file's own pixel size and ignore the setting.
        backgroundSize = `${clamp(image.tile_scale ?? 25, 2, 100)}% auto`;
        backgroundRepeat = 'repeat';
    } else if (fit === 'center') backgroundSize = 'auto';

    const posX = typeof image.offset_x === 'number' ? clamp(image.offset_x, 0, 100) : 50;
    const posY = typeof image.offset_y === 'number' ? clamp(image.offset_y, 0, 100) : 50;

    // Overscan exists only so a blurred edge cannot show. Without blur there is
    // nothing to hide, and enlarging would crop the modes that promise to fit
    // the picture whole — which is exactly what `contain` and `center` promise.
    const blur = Math.max(0, image.blur ?? 0);

    return {
        backgroundSize,
        backgroundRepeat,
        backgroundPosition: `${posX}% ${posY}%`,
        filter: `blur(${blur}px)`,
        scale: blur > 0 ? '1.06' : '1',
    };
}

export function applyTheme(
    theme: Theme | null,
    root: StyleTarget = document.documentElement,
    /** Background picture as a data URL; resolved by the caller from disk. */
    backgroundImageUrl?: string | null
): void {
    if (!theme) {
        for (const name of Object.keys(paletteVars(resolveTheme(DEFAULT_THEME)))) {
            root.style.removeProperty(name);
        }
        clearBackgroundImage(root);
        return;
    }
    const vars = paletteVars(resolveTheme(theme));
    for (const [name, value] of Object.entries(vars)) {
        root.style.setProperty(name, value);
    }
    applyBackgroundImage(theme, root, backgroundImageUrl);
}

/** Marks the document as carrying a picture, which index.css keys off. */
const BACKGROUND_ATTRIBUTE = 'data-r2-background-image';

function clearBackgroundImage(root: StyleTarget): void {
    lastImageUrl.delete(root);
    root.style.removeProperty(`${VAR_PREFIX}background-image`);
    root.style.removeProperty(`${VAR_PREFIX}background-blur`);
    root.style.removeProperty(`${VAR_PREFIX}background-veil`);
    root.style.removeProperty(`${VAR_PREFIX}background-size`);
    root.style.removeProperty(`${VAR_PREFIX}background-position`);
    root.style.removeProperty(`${VAR_PREFIX}background-scale`);
    root.style.removeProperty(`${VAR_PREFIX}background-repeat`);
    if ('removeAttribute' in root && typeof root.removeAttribute === 'function') {
        root.removeAttribute(BACKGROUND_ATTRIBUTE);
    }
}

/**
 * The data URL last written to a given root.
 *
 * A background picture is megabytes of base64. Dragging the blur or visibility
 * slider re-applies the theme on every step, and re-assigning that string each
 * time is what makes the editor crawl — so it is written only when it actually
 * changes.
 */
const lastImageUrl = new WeakMap<object, string>();

function applyBackgroundImage(
    theme: Theme,
    root: StyleTarget,
    url?: string | null
): void {
    const image = theme.backgroundImage;
    if (!image || !url) {
        clearBackgroundImage(root);
        return;
    }
    if (lastImageUrl.get(root) !== url) {
        root.style.setProperty(`${VAR_PREFIX}background-image`, `url("${url}")`);
        lastImageUrl.set(root, url);
    }
    root.style.setProperty(`${VAR_PREFIX}background-blur`, `${image.blur}px`);
    // The veil is the app background painted back over the picture. Inverting
    // the requested opacity means the slider reads as "how much picture", which
    // is what the user is actually choosing.
    root.style.setProperty(
        `${VAR_PREFIX}background-veil`,
        String(clamp(1 - image.opacity, 0, 1))
    );

    const layer = backgroundLayerStyle(image);
    root.style.setProperty(`${VAR_PREFIX}background-size`, layer.backgroundSize);
    root.style.setProperty(`${VAR_PREFIX}background-repeat`, layer.backgroundRepeat);
    root.style.setProperty(`${VAR_PREFIX}background-position`, layer.backgroundPosition);
    root.style.setProperty(`${VAR_PREFIX}background-scale`, layer.scale);

    if ('setAttribute' in root && typeof root.setAttribute === 'function') {
        root.setAttribute(BACKGROUND_ATTRIBUTE, '');
    }
}

// ── TOML ─────────────────────────────────────────────────────────────────────

/**
 * Serialise a theme to the on-disk format.
 *
 * Written by hand rather than through a library so the file keeps its comments
 * and key order: someone opening it in a text editor should find a document
 * that explains itself, not a machine dump.
 */
export function themeToToml(theme: Theme): string {
    const esc = (s: string) => s.replace(/\\/g, '\\\\').replace(/"/g, '\\"');
    const lines = [
        '# r2modmac theme',
        '# Colours are hex, e.g. "#1f2937". Edit and save — the app reloads it live.',
        '',
        `name = "${esc(theme.name)}"`,
    ];
    if (theme.author) lines.push(`author = "${esc(theme.author)}"`);
    lines.push('', '[colors]');
    for (const key of THEME_COLOR_KEYS) {
        const val = theme.colors[key];
        if (val) {
            const pad = ' '.repeat(Math.max(0, 14 - key.length));
            lines.push(`${key}${pad} = "${val}"  # ${THEME_COLOR_META[key].description}`);
        }
    }

    const manual = [...MANUAL_COLOR_KEYS, ...COVER_COLOR_KEYS].filter((key) => theme.colors[key]);
    if (manual.length > 0) {
        lines.push('', '# Used only while auto_contrast is off.');
        for (const key of manual) {
            const pad = ' '.repeat(Math.max(0, 10 - key.length));
            lines.push(`${key}${pad} = "${theme.colors[key]}"  # ${THEME_COLOR_META[key].description}`);
        }
    }

    const transparent = [...THEME_COLOR_KEYS, ...MANUAL_COLOR_KEYS, ...COVER_COLOR_KEYS]
        .filter((key) => typeof theme.opacity?.[key] === 'number');
    if (transparent.length > 0) {
        lines.push('', '[opacity]', '# Per-colour opacity: 0 = transparent, 1 = opaque.');
        for (const key of transparent) {
            const value = clamp(theme.opacity?.[key] ?? 1, 0, 1);
            lines.push(`${key} = ${round2(value)}`);
        }
    }

    const options = theme.options ?? DEFAULT_THEME_OPTIONS;
    lines.push(
        '',
        '[options]',
        '# Pick label colours per button so text stays readable on every fill.',
        '# Turn off to use the single text colour everywhere and place each',
        '# colour by hand.',
        `auto_contrast = ${options.autoContrast}`
    );

    if (theme.backgroundImage) {
        const { path, opacity, blur, fit, offset_x, offset_y, tile_scale } = theme.backgroundImage;
        lines.push(
            '',
            '[background_image]',
            `path     = "${esc(path)}"  # relative to the themes folder`,
            `opacity = ${round2(opacity)}  # 0 = hidden, 1 = fully visible`,
            `blur    = ${Math.round(blur)}  # pixels; softens busy artwork`,
            `fit     = "${fit || 'cover'}"  # cover, contain, fill, tile, center`,
            `offset_x = ${typeof offset_x === 'number' ? Math.round(offset_x) : 50}  # horizontal position in % (0..100)`,
            `offset_y = ${typeof offset_y === 'number' ? Math.round(offset_y) : 50}  # vertical position in % (0..100)`,
            `tile_scale = ${typeof tile_scale === 'number' ? Math.round(tile_scale) : 25}  # pattern size in % of the window, for fit = "tile"`
        );
    }

    lines.push('');
    return lines.join('\n');
}

/**
 * Fill in anything a hand-edited file left out.
 *
 * A theme missing keys still applies: absent colours fall back to the default
 * theme rather than rejecting the file, so a typo costs one colour, not all six.
 */
export function normalizeTheme(
    input: Omit<Partial<Theme>, 'colors' | 'opacity' | 'backgroundImage' | 'options'> & {
        colors?: Partial<ThemeColors>;
        opacity?: Partial<Record<keyof ThemeColors, number | null>> | null;
        // Nullable per field: this is what arrives over IPC, where an unset
        // value is `null` rather than absent. Accepting it here means callers
        // no longer hand-convert — which is what kept losing fields.
        backgroundImage?: { [K in keyof ThemeBackgroundImage]?: ThemeBackgroundImage[K] | null } | null;
        options?: Partial<ThemeOptions> | null;
    }
): Theme {
    const colors = {} as ThemeColors;
    for (const key of THEME_COLOR_KEYS) {
        const value = input.colors?.[key];
        colors[key] = value && isValidHex(value) ? normalizeHex(value) : (DEFAULT_THEME.colors[key] || '#1f2937');
    }

    for (const key of [...MANUAL_COLOR_KEYS, ...COVER_COLOR_KEYS]) {
        const value = input.colors?.[key];
        if (value && isValidHex(value)) colors[key] = normalizeHex(value);
    }


    const opacity: ThemeOpacity = {};
    for (const key of [...THEME_COLOR_KEYS, ...MANUAL_COLOR_KEYS, ...COVER_COLOR_KEYS]) {
        const value = input.opacity?.[key];
        if (typeof value === 'number' && Number.isFinite(value)) {
            opacity[key] = clamp(value, 0, 1);
        }
    }

    const rawOptions = input.options;
    const rawImage = input.backgroundImage;
    const path = rawImage?.path?.trim();
    const rawFit = rawImage?.fit;
    const validFit = rawFit && ['cover', 'contain', 'fill', 'tile', 'center'].includes(rawFit) ? rawFit : 'cover';

    return {
        name: input.name?.trim() || 'Untitled',
        author: input.author?.trim() || undefined,
        colors,
        opacity: Object.keys(opacity).length > 0 ? opacity : undefined,
        options: {
            autoContrast:
                typeof rawOptions?.autoContrast === 'boolean'
                    ? rawOptions.autoContrast
                    : DEFAULT_THEME_OPTIONS.autoContrast,
        },
        // A picture entry without a path is meaningless, so it is dropped
        // rather than kept as a half-configured background.
        backgroundImage: path
            ? {
                  path,
                  opacity: clamp(numberOr(rawImage?.opacity, 0.35), 0, 1),
                  blur: clamp(numberOr(rawImage?.blur, 0), 0, 40),
                  tile_scale: clamp(numberOr(rawImage?.tile_scale, 25), 2, 100),
                  fit: validFit,
                  offset_x: clamp(numberOr(rawImage?.offset_x, 50), 0, 100),
                  offset_y: clamp(numberOr(rawImage?.offset_y, 50), 0, 100),
              }
            : null,
    };
}

function numberOr(value: unknown, fallback: number): number {
    const n = typeof value === 'string' ? Number(value) : value;
    return typeof n === 'number' && Number.isFinite(n) ? n : fallback;
}

function round2(value: number): number {
    return Math.round(value * 100) / 100;
}

/** Canonical `#rrggbb`, so comparisons and swatches agree on form. */
export function normalizeHex(hex: string): string {
    const rgb = parseHex(hex);
    return rgb ? toHex(rgb) : hex;
}
