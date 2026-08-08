import { memo, useCallback, useEffect, useMemo, useState } from 'react';

import { Button } from '../ui';
import { ColorField } from '../ui/ColorPicker';
import { Toggle } from '../ui/Toggle';
import { ThemeTomlEditor } from './ThemeTomlEditor';
import { useThemeStore, summaryToTheme, forgetBackgroundImage } from '../../store/useThemeStore';
import { useAppStore } from '../../store/useAppStore';
import {
    DEFAULT_THEME,
    DEFAULT_THEME_OPTIONS,
    THEME_COLOR_GROUPS,
    THEME_COLOR_KEYS,
    MANUAL_COLOR_KEYS,
    COVER_COLOR_KEYS,
    DEFAULT_SCRIM,
    DEFAULT_MEDIA_INK,
    THEME_COLOR_META,
    findContrastWarnings,
    isValidHex,
    normalizeHex,
    normalizeTheme,
    resolveOnAccent,
    resolveTheme,
    themeToToml,
    type Theme,
    type ThemeColors,
} from '../../utils/theme';
import { allBuiltinThemes, isBuiltinId, type ThemePreset } from '../../utils/themePresets';
import type { ThemeSummary } from '../../types/electron';

interface ThemeEditorModalProps {
    isOpen: boolean;
    onClose: () => void;
}

/** The colours that identify a theme at a glance, in the order they read best. */
function SwatchStrip({ colors, className = '' }: { colors: ThemeColors; className?: string }) {
    return (
        <div className={`flex overflow-hidden rounded-md border border-gray-600/60 ${className}`}>
            {[colors.background, colors.surface, colors.accent, colors.text].map((color, i) => (
                <span key={i} className="h-full flex-1" style={{ backgroundColor: color }} />
            ))}
        </div>
    );
}

/**
 * A miniature of the app painted in the theme being edited.
 *
 * The whole window previews the theme already, but the parts that reveal a bad
 * choice — a status pill, a muted caption, a button label — are not necessarily
 * on screen while you are editing. This puts them all in one place.
 */
const ICON_SAMPLE_FAMILIES = ['purple', 'cyan', 'sky', 'rose', 'orange'];

const PreviewCard = memo(function PreviewCard({ colors }: { colors: ThemeColors }) {
    const c = colors;
    const iconSamples = useMemo(() => {
        const palette = resolveTheme({ name: 'preview', colors });
        return ICON_SAMPLE_FAMILIES.map((family) => ({
            family,
            color: palette.decorative[family][400],
            backdrop: palette.gray[700],
        }));
    }, [colors]);
    const onAccent = resolveOnAccent(c);
    const accentHover = c.accent_hover && isValidHex(c.accent_hover) ? normalizeHex(c.accent_hover) : c.accent;
    const surfaceHover = c.surface_hover && isValidHex(c.surface_hover) ? normalizeHex(c.surface_hover) : c.surface;

    return (
        <div
            className="overflow-hidden rounded-2xl border transition-colors"
            style={{ backgroundColor: c.background, borderColor: c.border }}
        >
            <div
                className="flex items-center justify-between border-b px-4 py-3"
                style={{ backgroundColor: c.surface, borderColor: c.border }}
            >
                <div className="min-w-0">
                    <p className="truncate text-[13px] font-semibold" style={{ color: c.text }}>
                        Live Preview
                    </p>
                    <p className="truncate text-[11px]" style={{ color: c.text_muted }}>
                        Real-time interface styling & hover states
                    </p>
                </div>
                <div className="flex items-center gap-2">
                    <span
                        className="rounded-lg px-3 py-1.5 text-[12px] font-semibold transition-transform shadow-sm"
                        style={{ backgroundColor: c.accent, color: onAccent }}
                        title="Normal Button State"
                    >
                        Primary
                    </span>
                    <span
                        className="rounded-lg px-3 py-1.5 text-[12px] font-semibold transition-transform shadow-sm"
                        style={{ backgroundColor: accentHover, color: onAccent }}
                        title="Hover Button State"
                    >
                        Hover
                    </span>
                </div>
            </div>

            <div className="space-y-3 p-4">
                <div className="flex flex-wrap items-center gap-2">
                    {([
                        ['Error', c.danger],
                        ['Warning', c.warning],
                        ['Success', c.success],
                    ] as const).map(([label, color]) => (
                        <span
                            key={label}
                            className="rounded-full border px-2.5 py-0.5 text-[11px] font-semibold"
                            style={{ color, borderColor: `${color}66`, backgroundColor: `${color}1f` }}
                        >
                            {label}
                        </span>
                    ))}
                    <span
                        className="rounded-md border px-2.5 py-0.5 text-[11px] font-medium"
                        style={{ backgroundColor: surfaceHover, borderColor: c.border, color: c.text }}
                        title="Surface Hover Preview"
                    >
                        Surface Item
                    </span>
                </div>
                <p className="text-[12px] leading-snug" style={{ color: c.text }}>
                    Primary text sits cleanly on surfaces and background.
                </p>
                <p className="text-[11px] leading-snug" style={{ color: c.text_muted }}>
                    Muted text carries descriptions, secondary metrics and timestamps.
                </p>

                {/* Icon hues are not a theme colour — they stay fixed so one
                    icon remains tellable from another — but their lightness is
                    matched to the panel they sit on. Showing them here is what
                    makes that adaptation visible while you edit. */}
                <div className="flex items-center gap-2 pt-1">
                    {iconSamples.map(({ family, color, backdrop }) => (
                        <span
                            key={family}
                            title={`${family} icon`}
                            className="flex h-6 w-6 items-center justify-center rounded-lg border"
                            style={{ backgroundColor: backdrop, borderColor: c.border, color }}
                        >
                            <svg className="h-3.5 w-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                <circle cx="12" cy="12" r="7" strokeWidth={2} />
                            </svg>
                        </span>
                    ))}
                    <span className="text-[11px]" style={{ color: c.text_muted }}>
                        Icons adapt to the panel
                    </span>
                </div>

            </div>
        </div>
    );
});

/**
 * Memoised, and given a stable `onChange` that carries its own key.
 *
 * Dragging a background slider changes the draft on every frame. Without this
 * every colour row — and the picker inside it — re-rendered each time even
 * though not one of their colours had moved.
 */
/**
 * A titled card whose contents are always the same two bands: a recessed
 * specimen showing what these colours paint, then the controls that change
 * them.
 *
 * The recess matters — it is what separates "this is what it looks like" from
 * "this is what you change". Before it, the editor was one preview at the top
 * and a long list of rows underneath, so by the time you reached Status the
 * thing you were judging had scrolled away.
 */
function Section({
    label,
    hint,
    specimen,
    children,
}: {
    label: string;
    hint: string;
    specimen?: React.ReactNode;
    children: React.ReactNode;
}) {
    return (
        <div className="space-y-2">
            <div className="flex items-baseline gap-2 px-1">
                <h3 className="text-xs font-semibold uppercase tracking-widest text-gray-400">{label}</h3>
                <span className="text-[11px] text-gray-400">{hint}</span>
            </div>
            <div className="overflow-hidden rounded-2xl border border-gray-700 bg-gray-800">
                {specimen && (
                    <div className="border-b border-gray-700/50 bg-gray-900/40 p-4">{specimen}</div>
                )}
                <div className="divide-y divide-gray-700/50">{children}</div>
            </div>
        </div>
    );
}

/**
 * A real entry from the game list the app already has loaded.
 *
 * The specimens show this rather than invented copy: a made-up mod name and a
 * fabricated download count tell you nothing about how your theme will look on
 * the things actually on your screen.
 */
interface Sample {
    name: string;
    identifier: string;
    image?: string;
}

/**
 * Specimens.
 *
 * Each reproduces a real piece of the app rather than an abstract swatch — the
 * mod card, its install button, its status pills — so what you judge while
 * editing is the thing you will be looking at afterwards. The structure is
 * taken from ModCard (same type scale, 10% tint, 20% border) and the colours
 * are inlined from the draft so they move as you drag.
 */

/** The page with a mod card on it: elevation as the app really stacks it. */
function SurfacesSpecimen({ colors, sample }: { colors: ThemeColors; sample: Sample }) {
    return (
        <div className="rounded-xl p-3" style={{ backgroundColor: colors.background }}>
            <div
                className="rounded-xl border p-3"
                style={{ backgroundColor: colors.surface, borderColor: colors.border }}
            >
                <div className="flex items-center gap-2">
                    <div
                        className="h-9 w-9 shrink-0 rounded-lg border bg-cover bg-center"
                        style={{
                            backgroundColor: colors.background,
                            borderColor: colors.border,
                            backgroundImage: sample.image ? `url("${sample.image}")` : undefined,
                        }}
                    />
                    <div className="min-w-0">
                        <p className="truncate text-[13px] font-bold" style={{ color: colors.text }}>
                            {sample.name}
                        </p>
                        <p className="truncate text-[11px]" style={{ color: colors.text_muted }}>
                            {sample.identifier}
                        </p>
                    </div>
                </div>
            </div>
        </div>
    );
}

/** The type hierarchy of a mod card: name, meta line, description. */
function TextSpecimen({ colors, sample }: { colors: ThemeColors; sample: Sample }) {
    return (
        <div
            className="rounded-xl border p-3"
            style={{ backgroundColor: colors.surface, borderColor: colors.border }}
        >
            <p className="text-[15px] font-bold" style={{ color: colors.text }}>
                {sample.name}
            </p>
            <p className="mt-0.5 text-[11px]" style={{ color: colors.text_muted }}>
                {sample.identifier}
            </p>
        </div>
    );
}

/**
 * The card's install button and the hover border the card itself takes.
 * Both are accent states that cannot be judged without pointing at them.
 */
function AccentSpecimen({
    colors,
    accentHover,
    onAccent,
    sample,
}: {
    colors: ThemeColors;
    accentHover: string;
    onAccent: string;
    sample: Sample;
}) {
    const [hovered, setHovered] = useState(false);
    const [cardHovered, setCardHovered] = useState(false);
    return (
        <div className="rounded-xl p-3" style={{ backgroundColor: colors.background }}>
            <div
                onPointerEnter={() => setCardHovered(true)}
                onPointerLeave={() => setCardHovered(false)}
                className="flex items-center justify-between gap-3 rounded-xl border p-3 transition-colors"
                style={{
                    backgroundColor: colors.surface,
                    borderColor: cardHovered ? `${colors.accent}80` : colors.border,
                }}
            >
                <div className="min-w-0">
                    <p
                        className="truncate text-[13px] font-bold transition-colors"
                        style={{ color: cardHovered ? colors.accent : colors.text }}
                    >
                        {sample.name}
                    </p>
                    <p className="truncate text-[11px]" style={{ color: colors.text_muted }}>
                        {sample.identifier}
                    </p>
                </div>
                <button
                    type="button"
                    onClick={(e) => e.preventDefault()}
                    onPointerEnter={() => setHovered(true)}
                    onPointerLeave={() => setHovered(false)}
                    onFocus={() => setHovered(true)}
                    onBlur={() => setHovered(false)}
                    className="shrink-0 rounded-lg px-3 py-1.5 text-[12px] font-bold shadow-sm transition-colors active:scale-95"
                    style={{ backgroundColor: hovered ? accentHover : colors.accent, color: onAccent }}
                >
                    Install
                </button>
            </div>
        </div>
    );
}

/** The real status treatments: installed, update available, uninstall, deprecated. */
function StatusSpecimen({ colors }: { colors: ThemeColors }) {
    const pill = (color: string) => ({
        color,
        borderColor: `${color}33`,
        backgroundColor: `${color}1a`,
    });
    return (
        <div
            className="flex flex-wrap items-center gap-2 rounded-xl border p-3"
            style={{ backgroundColor: colors.surface, borderColor: colors.border }}
        >
            <span className="rounded-lg border px-2.5 py-1.5 text-[12px] font-bold" style={pill(colors.success)}>
                Installed
            </span>
            <span
                className="rounded-lg px-2.5 py-1.5 text-[12px] font-bold"
                style={{ backgroundColor: colors.warning, color: colors.background }}
            >
                Update available
            </span>
            <span className="rounded-lg border px-2.5 py-1.5 text-[12px] font-bold" style={pill(colors.danger)}>
                Uninstall
            </span>
            <span
                className="rounded-md border px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide"
                style={pill(colors.warning)}
            >
                Deprecated
            </span>
        </div>
    );
}

/**
 * A real cover with the chrome that floats on it.
 *
 * Interactive on purpose: in the real grid the favourite star only appears on
 * hover, so a static picture would hide exactly the state worth checking.
 */
function CoverSpecimen({ colors, sample }: { colors: ThemeColors; sample: Sample }) {
    return (
        <div className="flex items-center gap-4 rounded-xl p-3" style={{ backgroundColor: colors.background }}>
            <div
                className="group relative h-[104px] w-[74px] shrink-0 cursor-pointer overflow-hidden rounded-lg border bg-cover bg-center"
                style={{
                    borderColor: colors.border,
                    backgroundImage: sample.image
                        ? `url("${sample.image}")`
                        : 'linear-gradient(135deg,#7c3aed,#ec4899 45%,#f59e0b)',
                }}
                title="Hover to reveal the favourite control"
            >
                <span className="absolute left-1 top-1 flex h-5 w-5 items-center justify-center rounded-full border border-on-media/15 bg-scrim/70 text-on-media/80 opacity-0 backdrop-blur-sm transition-all group-hover:scale-110 group-hover:text-yellow-400 group-hover:opacity-100">
                    <svg className="h-3 w-3" viewBox="0 0 20 20" fill="currentColor">
                        <path d="M10 1.5l2.6 5.3 5.9.9-4.3 4.1 1 5.8-5.2-2.7-5.2 2.7 1-5.8L1.5 7.7l5.9-.9z" />
                    </svg>
                </span>
                <span className="absolute right-1 top-1 flex h-5 items-center gap-1 rounded-full border border-on-media/15 bg-scrim/75 px-1.5 text-on-media backdrop-blur-sm">
                    <svg className="h-2.5 w-2.5" viewBox="0 0 24 24" fill="currentColor">
                        <path d="M0 3.449L9.75 2.1v9.451H0m10.949-9.602L24 0v11.4H10.949M0 12.6h9.75v9.451L0 20.699M10.949 12.6H24V24l-12.9-1.801" />
                    </svg>
                </span>
                <span
                    className="absolute inset-x-0 bottom-0 px-1.5 pb-1 pt-5 text-[9px] font-bold leading-tight text-on-media opacity-0 transition-opacity group-hover:opacity-100"
                    style={{ backgroundImage: 'linear-gradient(to top, rgb(9 9 11 / 0.92), transparent)' }}
                >
                    {sample.name}
                </span>
            </div>
            <p className="text-[11px] leading-relaxed" style={{ color: colors.text_muted }}>
                Hover the cover. These badges default to a dark scrim because the artwork
                underneath is arbitrary, but both colours are yours to change below.
            </p>
        </div>
    );
}

/** Two fills at opposite ends, so the label choice is visible at a glance. */
function LabelsSpecimen({
    colors,
    palette,
}: {
    colors: ThemeColors;
    palette: ReturnType<typeof resolveTheme>;
}) {
    const chips = [
        ['Primary', colors.accent, palette.on.accent],
        ['Secondary', palette.gray[700], palette.on.surface],
        ['Delete', palette.red[600], palette.on.danger],
    ] as const;
    return (
        <div className="flex flex-wrap gap-2 rounded-xl p-3" style={{ backgroundColor: colors.surface }}>
            {chips.map(([label, fill, ink]) => (
                <span
                    key={label}
                    className="rounded-lg px-3 py-1.5 text-[12px] font-semibold"
                    style={{ backgroundColor: fill, color: ink }}
                >
                    {label}
                </span>
            ))}
        </div>
    );
}

const ColorRow = memo(function ColorRow({
    colorKey,
    value,
    presets,
    onChange,
    disabled,
}: {
    colorKey: keyof ThemeColors;
    value: string;
    presets: string[];
    onChange: (key: keyof ThemeColors, next: string) => void;
    disabled: boolean;
}) {
    const meta = THEME_COLOR_META[colorKey];
    const [draft, setDraft] = useState(value);
    const [lastValue, setLastValue] = useState(value);

    if (value !== lastValue) {
        setLastValue(value);
        setDraft(value);
    }

    const commit = (next: string) => {
        setDraft(next);
        if (isValidHex(next)) onChange(colorKey, normalizeHex(next));
    };

    return (
        <div className={`flex items-center justify-between gap-4 p-4 ${disabled ? 'opacity-60' : ''}`}>
            <div className="min-w-0">
                <p className="text-[15px] font-medium text-white">{meta.label}</p>
                <p className="mt-0.5 text-[13px] leading-snug text-gray-400">{meta.description}</p>
            </div>
            <div className="flex shrink-0 items-center gap-2">
                <input
                    value={draft}
                    onChange={(e) => commit(e.target.value)}
                    onBlur={() => setDraft(value)}
                    disabled={disabled}
                    spellCheck={false}
                    aria-label={`${meta.label} hex value`}
                    className={`w-[92px] rounded-lg border bg-gray-900 px-2.5 py-1.5 text-right font-mono text-[13px] text-white transition-colors focus:outline-none focus:ring-1 disabled:cursor-not-allowed ${
                        isValidHex(draft)
                            ? 'border-gray-600 focus:border-blue-500 focus:ring-blue-500'
                            : 'border-red-500/60 focus:border-red-500 focus:ring-red-500'
                    }`}
                />
                {disabled ? (
                    <span
                        className="h-9 w-9 shrink-0 rounded-lg border border-gray-500"
                        style={{ backgroundColor: value }}
                    />
                ) : (
                    <ColorField
                        label={meta.label}
                        value={value}
                        presets={presets}
                        onChange={(next) => onChange(colorKey, next)}
                    />
                )}
            </div>
        </div>
    );
});

function Slider({
    value, min, max, step, onChange, ariaLabel, disabled = false,
}: {
    value: number; min: number; max: number; step: number;
    onChange: (n: number) => void; ariaLabel: string; disabled?: boolean;
}) {
    const pct = ((value - min) / (max - min)) * 100;
    return (
        <input
            type="range"
            disabled={disabled}
            min={min}
            max={max}
            step={step}
            value={value}
            aria-label={ariaLabel}
            onChange={(e) => onChange(Number(e.target.value))}
            style={{
                background: `linear-gradient(to right, rgb(var(--r2-blue-600)) ${pct}%, rgb(var(--r2-gray-700)) ${pct}%)`,
            }}
            className="h-2 w-full cursor-pointer appearance-none rounded-full border border-gray-600/70 disabled:cursor-not-allowed disabled:opacity-50 [&::-moz-range-thumb]:h-4 [&::-moz-range-thumb]:w-4 [&::-moz-range-thumb]:appearance-none [&::-moz-range-thumb]:rounded-full [&::-moz-range-thumb]:border [&::-moz-range-thumb]:border-gray-400 [&::-moz-range-thumb]:bg-[#ffffff] [&::-moz-range-thumb]:shadow-sm [&::-webkit-slider-thumb]:h-4 [&::-webkit-slider-thumb]:w-4 [&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:border [&::-webkit-slider-thumb]:border-gray-400 [&::-webkit-slider-thumb]:bg-[#ffffff] [&::-webkit-slider-thumb]:shadow-sm"
        />
    );
}

export function ThemeEditorModal({ isOpen, onClose }: ThemeEditorModalProps) {
    const communities = useAppStore((s) => s.communities);
    const communityImages = useAppStore((s) => s.communityImages);
    const themes = useThemeStore((s) => s.themes);
    const activeFileName = useThemeStore((s) => s.activeFileName);
    const loadThemes = useThemeStore((s) => s.loadThemes);
    const setActive = useThemeStore((s) => s.setActive);
    const setPreview = useThemeStore((s) => s.setPreview);

    const [draft, setDraft] = useState<Theme | null>(null);
    const [dirty, setDirty] = useState(false);
    const [view, setView] = useState<'colours' | 'toml'>('colours');
    const [collapsedGroups, setCollapsedGroups] = useState<Set<string>>(new Set());
    const [saving, setSaving] = useState(false);
    const [busy, setBusy] = useState(false);
    const [imageUrl, setImageUrl] = useState<string | null>(null);

    // The store's selection is the only one. Keeping a second copy here meant
    // the editor could show one theme while the window painted another.
    const selectedId = activeFileName;

    const builtins = useMemo(() => allBuiltinThemes(), []);
    const builtin: ThemePreset | null = useMemo(
        () => builtins.find((b) => b.id === selectedId) ?? null,
        [builtins, selectedId]
    );
    const file: ThemeSummary | null = useMemo(
        () => themes.find((t) => t.file_name === selectedId) ?? null,
        [themes, selectedId]
    );

    /** Only real files on disk can be edited; presets are duplicated first. */
    const editable = !!selectedId && !isBuiltinId(selectedId) && !!file;

    const [wasOpen, setWasOpen] = useState(isOpen);

    /**
     * What the draft was built from, as content rather than object identity.
     *
     * The themes list is rebuilt on every reload — including the one the file
     * watcher triggers after each save — so comparing objects rebuilt the draft
     * constantly. Comparing content means a reload that changed nothing leaves
     * the editor alone, and one that did change it (someone editing the TOML in
     * another window) is picked up.
     */
    const signature = builtin
        ? `builtin:${builtin.id}`
        : file
          ? JSON.stringify([file.file_name, file.name, file.colors, file.background_image])
          : 'none';

    const [draftSignature, setDraftSignature] = useState<string | null>(null);

    // Adopting the selection during render — the pattern the other modals here
    // use — means the first paint is already correct.
    if (isOpen !== wasOpen) {
        setWasOpen(isOpen);
        if (isOpen) {
            setDirty(false);
            setView('colours');
            setDraftSignature(null);
        }
    }

    if (isOpen && !dirty && draftSignature !== signature) {
        setDraftSignature(signature);
        setDraft(builtin ? normalizeTheme(builtin) : file ? summaryToTheme(file) : null);
    }

    useEffect(() => {
        if (isOpen) void loadThemes();
    }, [isOpen, loadThemes]);

    // Paint edits across the whole window.
    useEffect(() => {
        if (!isOpen) return;
        setPreview(dirty ? draft : null);
    }, [isOpen, dirty, draft, setPreview]);

    useEffect(() => () => setPreview(null), [setPreview]);

    // Thumbnail for the background section. The stale thumbnail is dropped
    // during render when the path changes, so only the arrival of the new one
    // — genuinely external — happens in the effect.
    const imagePath = draft?.backgroundImage?.path ?? null;
    const [thumbnailFor, setThumbnailFor] = useState<string | null>(null);
    if (imagePath !== thumbnailFor) {
        setThumbnailFor(imagePath);
        setImageUrl(null);
    }

    useEffect(() => {
        if (!imagePath) return;
        let cancelled = false;
        void window.ipcRenderer
            .readThemeImage(imagePath)
            .then((url) => { if (!cancelled) setImageUrl(url); })
            .catch(() => { if (!cancelled) setImageUrl(null); });
        return () => { cancelled = true; };
    }, [imagePath]);

    // Keyed by the colours themselves so the array keeps its identity while
    // unrelated parts of the draft (the background picture) change.
    const presetKey = draft ? THEME_COLOR_KEYS.map((k) => draft.colors[k]).join('|') : '';
    const swatchPresets = useMemo(
        () => (presetKey ? presetKey.split('|') : []),
        [presetKey]
    );

    // The sidebar swatches depend only on the files, not on the draft. Without
    // this every theme in the list was re-normalised on each pointer move while
    // a colour was being dragged.
    const listColors = useMemo(
        () => new Map(themes.map((t) => [t.file_name, summaryToTheme(t).colors])),
        [themes]
    );

    const updateColor = useCallback((key: keyof ThemeColors, value: string) => {
        setDraft((prev) => (prev ? { ...prev, colors: { ...prev.colors, [key]: value } } : prev));
        setDirty(true);
    }, []);

    const updateImage = useCallback((patch: Partial<NonNullable<Theme['backgroundImage']>>) => {
        setDraft((prev) => {
            if (!prev?.backgroundImage) return prev;
            return { ...prev, backgroundImage: { ...prev.backgroundImage, ...patch } };
        });
        setDirty(true);
    }, []);

    const handleSelect = useCallback(
        async (id: string | null) => {
            if (dirty) {
                const discard = await window.ipcRenderer.confirm(
                    'Discard changes?',
                    'This theme has unsaved changes. Switching away will discard them.'
                );
                if (!discard) return;
                setDirty(false);
            }
            await setActive(id);
        },
        [dirty, setActive]
    );

    const handleSave = useCallback(async () => {
        if (!draft || !editable || !selectedId) return;
        setSaving(true);
        try {
            await window.ipcRenderer.writeTheme(selectedId, themeToToml(draft));
            forgetBackgroundImage(draft.backgroundImage?.path);
            // Reload before dropping the draft, so the reloaded file — not the
            // pre-save copy still in the list — is what the editor and the
            // window fall back to. Clearing first briefly repainted the old
            // theme and reset the fields the user had just edited.
            await loadThemes();
            setDirty(false);
            setPreview(null);
        } catch (error) {
            await window.ipcRenderer.alert('Could not save the theme', String(error));
        } finally {
            setSaving(false);
        }
    }, [draft, editable, selectedId, loadThemes, setPreview]);

    /** Copy whatever is selected into a new, editable file. */
    const handleDuplicate = useCallback(async () => {
        setBusy(true);
        try {
            const base = draft ?? DEFAULT_THEME;
            const name = builtin ? `${builtin.name} copy` : `${base.name} copy`;
            const fileName = await window.ipcRenderer.suggestThemeFileName(name);
            const created: Theme = { ...base, name, author: undefined };
            await window.ipcRenderer.writeTheme(fileName, themeToToml(created));
            await loadThemes();
            setDirty(false);
            // The draft rebuilds itself from the reloaded file, so there is no
            // second copy here that could disagree with what is on disk.
            await setActive(fileName);
        } catch (error) {
            await window.ipcRenderer.alert('Could not create the theme', String(error));
        } finally {
            setBusy(false);
        }
    }, [draft, builtin, loadThemes, setActive]);

    const handleDelete = useCallback(async () => {
        if (!file) return;
        const confirmed = await window.ipcRenderer.confirm(
            'Delete theme?',
            `"${file.name}" will be removed from disk. This cannot be undone.`
        );
        if (!confirmed) return;
        try {
            await window.ipcRenderer.deleteTheme(file.file_name);
            setDirty(false);
            setPreview(null);
            await setActive(null);
            await loadThemes();
        } catch (error) {
            await window.ipcRenderer.alert('Could not delete the theme', String(error));
        }
    }, [file, activeFileName, setActive, loadThemes, setPreview]);

    const handlePickImage = useCallback(async () => {
        setBusy(true);
        try {
            const path = await window.ipcRenderer.selectFile([
                { name: 'Images', extensions: ['png', 'jpg', 'jpeg', 'webp', 'gif', 'bmp'] },
            ]);
            if (!path) return;
            const relative = await window.ipcRenderer.importThemeImage(path);
            forgetBackgroundImage(relative);
            setDraft((prev) =>
                prev ? { ...prev, backgroundImage: { path: relative, opacity: 0.35, blur: 0, fit: 'cover', offset_x: 50, offset_y: 50 } } : prev
            );
            setDirty(true);
        } catch (error) {
            await window.ipcRenderer.alert('Could not use that image', String(error));
        } finally {
            setBusy(false);
        }
    }, []);

    const handleClose = useCallback(async () => {
        if (dirty) {
            const discard = await window.ipcRenderer.confirm(
                'Discard changes?',
                'This theme has unsaved changes. Closing will discard them.'
            );
            if (!discard) return;
        }
        setDirty(false);
        setPreview(null);
        onClose();
    }, [dirty, onClose, setPreview]);

    useEffect(() => {
        if (!isOpen) return;
        const onKeyDown = (e: KeyboardEvent) => {
            if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 's') {
                e.preventDefault();
                void handleSave();
            }
        };
        document.addEventListener('keydown', onKeyDown);
        return () => document.removeEventListener('keydown', onKeyDown);
    }, [isOpen, handleSave]);

    const warnings = useMemo(() => (draft ? findContrastWarnings(draft.colors) : []), [draft]);
    const autoContrast = draft?.options?.autoContrast ?? DEFAULT_THEME_OPTIONS.autoContrast;

    // Resolved once per edit and handed to the specimens, so each one shows the
    // same colours the app is being painted with rather than re-deriving them.
    const previewPalette = useMemo(
        () => resolveTheme(draft ?? DEFAULT_THEME),
        [draft]
    );

    // Borrowed from the covers already in memory, so the preview shows real
    // artwork rather than a stand-in; falls back to a gradient when the game
    // list has not loaded yet.
    const sample = useMemo<Sample>(() => {
        const withArt = communities.find((c) => communityImages[c.identifier]);
        const first = withArt ?? communities[0];
        if (!first) {
            // Only reached before the game list has loaded.
            return { name: 'Your games', identifier: 'appear here' };
        }
        return {
            name: first.name,
            identifier: first.identifier,
            image: communityImages[first.identifier],
        };
    }, [communities, communityImages]);

    const themeButton = (
        id: string | null,
        name: string,
        colors: ThemeColors,
        subtitle: string | null,
        error?: string | null
    ) => {
        const isSelected = selectedId === id;
        return (
            <button
                key={id ?? 'stock'}
                onClick={() => void handleSelect(id)}
                className={`w-full rounded-xl border p-3 text-left transition-colors ${
                    isSelected
                        ? 'border-blue-500/60 bg-blue-500/10'
                        : 'border-gray-700 hover:border-gray-600 hover:bg-gray-800'
                }`}
            >
                <div className="flex items-center gap-2">
                    <SwatchStrip colors={colors} className="h-5 w-14 shrink-0" />
                    <span className="min-w-0 flex-1 truncate text-[13px] font-medium text-white">{name}</span>
                    {isSelected && dirty && (
                        <span className="shrink-0 text-fg-warning" title="Unsaved changes">&#9679;</span>
                    )}
                </div>
                {error ? (
                    <p className="mt-1.5 truncate text-[11px] text-fg-danger" title={error}>
                        Could not be read: {error}
                    </p>
                ) : subtitle ? (
                    <p className="mt-1.5 truncate text-[11px] text-gray-400">{subtitle}</p>
                ) : null}
            </button>
        );
    };

    // The sidebar depends only on the files and the selection, never on the
    // draft. Held apart so dragging a colour or a background slider — which
    // rewrites the draft every frame — does not rebuild the whole list with it.
    /**
     * The list, grouped and collapsible.
     *
     * Built-ins go in one group; your own themes are grouped by the author
     * written in the file, so a set shared by one person stays together instead
     * of scattering through one long alphabetical list. Anything without an
     * author falls into "Unattributed" rather than being hidden.
     */
    const groupedThemes = useMemo(() => {
        const byAuthor = new Map<string, ThemeSummary[]>();
        for (const t of themes) {
            const author = t.author?.trim() || 'Unattributed';
            const list = byAuthor.get(author);
            if (list) list.push(t);
            else byAuthor.set(author, [t]);
        }
        // Named authors first, alphabetically; the unattributed pile last.
        return [...byAuthor.entries()].sort(([a], [b]) => {
            if (a === 'Unattributed') return 1;
            if (b === 'Unattributed') return -1;
            return a.localeCompare(b);
        });
    }, [themes]);

    const themeList = useMemo(() => {
        const group = (
            id: string,
            label: string,
            count: number,
            children: React.ReactNode
        ) => {
            const open = !collapsedGroups.has(id);
            return (
                <div key={id} className="space-y-2">
                    <button
                        type="button"
                        onClick={() =>
                            setCollapsedGroups((prev) => {
                                const next = new Set(prev);
                                if (next.has(id)) next.delete(id);
                                else next.add(id);
                                return next;
                            })
                        }
                        aria-expanded={open}
                        className="flex w-full items-center gap-1.5 rounded-md px-1 py-0.5 text-left transition-colors hover:bg-gray-800"
                    >
                        <svg
                            className={`h-3 w-3 shrink-0 text-gray-400 transition-transform duration-150 ${open ? '' : '-rotate-90'}`}
                            fill="none"
                            viewBox="0 0 24 24"
                            stroke="currentColor"
                            aria-hidden="true"
                        >
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.5} d="M19 9l-7 7-7-7" />
                        </svg>
                        <span className="min-w-0 flex-1 truncate text-[11px] font-semibold uppercase tracking-wider text-gray-400">
                            {label}
                        </span>
                        <span className="shrink-0 text-[10px] text-gray-400">{count}</span>
                    </button>
                    {open && <div className="space-y-2">{children}</div>}
                </div>
            );
        };

        return (
            <>
                {group(
                    'built-in',
                    'Built in',
                    builtins.length,
                    <>
                        {themeButton(null, 'Default', DEFAULT_THEME.colors, 'The stock r2modmac look')}
                        {builtins
                            .filter((b) => b.id !== 'builtin:default')
                            .map((b) => themeButton(b.id, b.name, b.colors, b.origin))}
                    </>
                )}

                {themes.length === 0 && (
                    <p className="px-1 text-[11px] leading-relaxed text-gray-400">
                        None yet. Duplicate a built-in theme to start your own.
                    </p>
                )}

                {groupedThemes.map(([author, list]) =>
                    group(
                        `author:${author}`,
                        author,
                        list.length,
                        list.map((t) =>
                            themeButton(
                                t.file_name,
                                t.name,
                                listColors.get(t.file_name) ?? DEFAULT_THEME.colors,
                                null,
                                t.error
                            )
                        )
                    )
                )}
            </>
        );
    }, [builtins, themes, groupedThemes, listColors, selectedId, dirty, collapsedGroups]);

    if (!isOpen) return null;


    return (
        <div className="fixed inset-0 z-[60] flex items-center justify-center p-4">
            <div className="absolute inset-0 bg-black/60 backdrop-blur-sm" onClick={() => void handleClose()} />

            <div
                className="relative flex h-[88vh] w-full max-w-[1000px] flex-col overflow-hidden rounded-2xl border border-gray-700 bg-gray-900 shadow-2xl"
                onClick={(e) => e.stopPropagation()}
            >
                <div className="flex shrink-0 items-center justify-between border-b border-gray-800 px-7 py-6">
                    <div>
                        <h2 className="text-2xl font-bold tracking-tight text-white">Themes</h2>
                        <p className="mt-1 text-[13px] text-gray-400">
                            Changes preview across the whole app as you make them.
                        </p>
                    </div>
                    <button
                        onClick={() => void handleClose()}
                        className="rounded-xl p-2 text-gray-400 transition-all hover:bg-gray-800 hover:text-white active:scale-95 focus:outline-none focus:ring-2 focus:ring-gray-700"
                        aria-label="Close"
                    >
                        <svg className="h-5 w-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                        </svg>
                    </button>
                </div>

                <div className="flex min-h-0 flex-1">
                    {/* Theme list */}
                    <div className="flex w-64 shrink-0 flex-col border-r border-gray-800">
                        {/* pr leaves room for the overlay scrollbar, which
                            otherwise draws on top of the cards. */}
                        <div className="min-h-0 flex-1 space-y-4 overflow-y-auto p-3 pr-2">
                            {themeList}
                        </div>

                        {/* A fade above the actions signals that the list keeps
                            going, since the overlay scrollbar only appears on
                            hover and gives no resting hint. */}
                        <div className="pointer-events-none -mt-6 h-6 shrink-0 bg-gradient-to-t from-gray-900 to-transparent" />
                        <div className="shrink-0 space-y-2 border-t border-gray-800 p-3">
                            <Button variant="secondary" fullWidth onClick={() => void handleDuplicate()} disabled={busy}>
                                {builtin || !selectedId ? 'Duplicate & edit' : 'New from this'}
                            </Button>
                            <button
                                onClick={() => void window.ipcRenderer.openThemesFolder()}
                                className="w-full rounded-lg px-3 py-1.5 text-[12px] text-gray-400 transition-colors hover:bg-gray-800 hover:text-white"
                            >
                                Open themes folder
                            </button>
                        </div>
                    </div>

                    {/* Editor */}
                    <div className="flex min-h-0 min-w-0 flex-1 flex-col">
                        {!draft ? (
                            <div className="flex flex-1 flex-col items-center justify-center px-8 text-center">
                                <p className="text-[15px] font-medium text-white">Using the default look</p>
                                <p className="mt-2 max-w-sm text-[13px] leading-relaxed text-gray-400">
                                    Pick a built-in theme, or duplicate one to make it your own. Themes are
                                    plain TOML files you can also edit in any editor — the app picks up your
                                    changes as soon as you save.
                                </p>
                                <Button variant="primary" onClick={() => void handleDuplicate()} disabled={busy} className="mt-5">
                                    Duplicate & edit
                                </Button>
                            </div>
                        ) : (
                            <>
                                <div className="flex shrink-0 items-center justify-between gap-4 border-b border-gray-800 px-6 py-4">
                                    <div className="flex min-w-0 flex-1 flex-col">
                                        <input
                                            value={draft.name}
                                            disabled={!editable}
                                            onChange={(e) => { setDraft({ ...draft, name: e.target.value }); setDirty(true); }}
                                            aria-label="Theme name"
                                            className="min-w-0 rounded-lg border border-transparent bg-transparent px-2 py-1 text-[17px] font-semibold text-white transition-colors hover:border-gray-700 focus:border-blue-500 focus:bg-gray-800 focus:outline-none disabled:hover:border-transparent"
                                        />
                                        {/* The author is what the sidebar groups by, so it is
                                            edited right beside the name rather than buried. */}
                                        <input
                                            value={draft.author ?? ''}
                                            disabled={!editable}
                                            placeholder="Author — groups your themes in the list"
                                            onChange={(e) => {
                                                setDraft({ ...draft, author: e.target.value || undefined });
                                                setDirty(true);
                                            }}
                                            aria-label="Theme author"
                                            className="min-w-0 rounded-lg border border-transparent bg-transparent px-2 py-0.5 text-[12px] text-gray-400 transition-colors placeholder:text-gray-400/60 hover:border-gray-700 focus:border-blue-500 focus:bg-gray-800 focus:outline-none disabled:hover:border-transparent"
                                        />
                                    </div>
                                    {/* A pencil rather than a labelled tab: this is a
                                        mode switch into the file itself, not a second
                                        view of the same controls. */}
                                    <button
                                        type="button"
                                        onClick={() => setView(view === 'toml' ? 'colours' : 'toml')}
                                        aria-pressed={view === 'toml'}
                                        title={view === 'toml' ? 'Back to the colour controls' : 'Edit the theme file directly'}
                                        aria-label={view === 'toml' ? 'Back to the colour controls' : 'Edit the theme file directly'}
                                        className={`shrink-0 rounded-lg border p-2 transition-colors ${
                                            view === 'toml'
                                                ? 'border-blue-500 bg-blue-600 text-on-accent'
                                                : 'border-gray-700 bg-gray-800 text-gray-400 hover:border-gray-600 hover:text-white'
                                        }`}
                                    >
                                        <svg className="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.8} d="M16.86 4.49l2.65 2.65M4 20h3.2l9.9-9.9-3.2-3.2L4 16.8V20z" />
                                        </svg>
                                    </button>
                                </div>

                                {view === 'colours' ? (
                                    <div className="min-h-0 flex-1 overflow-y-auto p-6 pr-5">
                                        <div className="space-y-5">
                                            {!editable && (
                                                <div className="flex items-center justify-between gap-4 rounded-2xl border border-fg-accent/30 bg-fg-accent/10 p-4">
                                                    <p className="text-[13px] text-fg-accent">
                                                        Built-in themes can't be edited. Duplicate this one to make it yours.
                                                    </p>
                                                    <Button variant="primary" size="sm" onClick={() => void handleDuplicate()} disabled={busy}>
                                                        Duplicate
                                                    </Button>
                                                </div>
                                            )}

                                            {warnings.length > 0 && (
                                                <div className="rounded-2xl border border-fg-warning/30 bg-fg-warning/10 p-4">
                                                    <p className="text-[13px] font-semibold text-fg-warning">
                                                        Some combinations may be hard to read
                                                    </p>
                                                    <ul className="mt-2 space-y-1">
                                                        {warnings.map((w) => (
                                                            <li key={w.pair} className="text-[12px] text-fg-warning/90">
                                                                {w.pair} — contrast {w.ratio.toFixed(1)}:1
                                                            </li>
                                                        ))}
                                                    </ul>
                                                </div>
                                            )}

                                            <PreviewCard colors={draft.colors} />

                                            <Section
                                                label="Automatic"
                                                hint="What the theme works out for itself"
                                                specimen={<LabelsSpecimen colors={draft.colors} palette={previewPalette} />}
                                            >
                                                <div className={`flex items-center justify-between gap-4 p-4 ${!editable ? 'opacity-60' : ''}`}>
                                                    <div className="min-w-0">
                                                        <p className="text-[15px] font-medium text-white">Readable labels</p>
                                                        <p className="mt-0.5 text-[13px] leading-snug text-gray-400">
                                                            Pick each button's label from your text and background colours, so
                                                            it stays legible on a pale confirm button and a dark cancel button
                                                            alike. Off, the single text colour is used everywhere.
                                                        </p>
                                                    </div>
                                                    <Toggle
                                                        value={autoContrast}
                                                        disabled={!editable}
                                                        label="Readable labels"
                                                        onChange={(next) => {
                                                            setDraft({ ...draft, options: { autoContrast: next } });
                                                            setDirty(true);
                                                        }}
                                                    />
                                                </div>
                                            </Section>

                                            {/* Revealed by switching the automatic
                                                behaviour off: the same decisions, made by
                                                hand. Hidden while it is on, so the common
                                                case stays at nine colours rather than
                                                fifteen. */}
                                            {!autoContrast && (
                                                <Section
                                                    label="Manual"
                                                    hint="Labels and icons, now yours to set"
                                                    specimen={<LabelsSpecimen colors={draft.colors} palette={previewPalette} />}
                                                >
                                                    {MANUAL_COLOR_KEYS.map((key) => (
                                                        <ColorRow
                                                            key={key}
                                                            colorKey={key}
                                                            value={draft.colors[key] ?? draft.colors.text}
                                                            presets={swatchPresets}
                                                            disabled={!editable}
                                                            onChange={updateColor}
                                                        />
                                                    ))}
                                                    <div className="p-4">
                                                        <p className="text-[12px] leading-relaxed text-gray-400">
                                                            Icons themselves need no setting: every one in the app takes its
                                                            colour from the text around it, so new ones follow the theme with
                                                            nothing to register.
                                                        </p>
                                                    </div>
                                                </Section>
                                            )}

                                            {THEME_COLOR_GROUPS.map((group) => (
                                                <Section
                                                    key={group.id}
                                                    label={group.label}
                                                    hint={group.hint}
                                                    specimen={
                                                        group.id === 'surfaces' ? (
                                                            <SurfacesSpecimen colors={draft.colors} sample={sample} />
                                                        ) : group.id === 'text' ? (
                                                            <TextSpecimen colors={draft.colors} sample={sample} />
                                                        ) : group.id === 'accent' ? (
                                                            <AccentSpecimen
                                                                colors={draft.colors}
                                                                accentHover={previewPalette.accentHover}
                                                                onAccent={previewPalette.on.accent}
                                                                sample={sample}
                                                            />
                                                        ) : (
                                                            <StatusSpecimen colors={draft.colors} />
                                                        )
                                                    }
                                                >
                                                    {group.keys.map((key) => (
                                                        <ColorRow
                                                            key={key}
                                                            colorKey={key}
                                                            value={draft.colors[key] ?? DEFAULT_THEME.colors[key] ?? '#1f2937'}
                                                            presets={swatchPresets}
                                                            disabled={!editable}
                                                            onChange={updateColor}
                                                        />
                                                    ))}
                                                </Section>
                                            ))}

                                            {/* Cover chrome is its own section: it is the one place
                                                the theme deliberately starts from a fixed default. */}
                                            <Section
                                                label="Game covers"
                                                hint="Chrome that floats on artwork"
                                                specimen={<CoverSpecimen colors={draft.colors} sample={sample} />}
                                            >
                                                {COVER_COLOR_KEYS.map((key) => (
                                                    <ColorRow
                                                        key={key}
                                                        colorKey={key}
                                                        value={
                                                            draft.colors[key] ??
                                                            (key === 'media_scrim' ? DEFAULT_SCRIM : DEFAULT_MEDIA_INK)
                                                        }
                                                        presets={swatchPresets}
                                                        disabled={!editable}
                                                        onChange={updateColor}
                                                    />
                                                ))}
                                            </Section>

                                            <Section
                                                label="Background"
                                                hint="An optional picture behind the app"
                                            >
                                                {draft.backgroundImage ? (
                                                    <div className="space-y-4 p-4">
                                                        <div className="flex items-center gap-4">
                                                            <div
                                                                className="h-16 w-28 shrink-0 rounded-lg border border-gray-600 bg-gray-900 bg-center"
                                                                style={{
                                                                    backgroundImage: imageUrl ? `url("${imageUrl}")` : undefined,
                                                                    backgroundSize:
                                                                        draft.backgroundImage.fit === 'contain'
                                                                            ? 'contain'
                                                                            : draft.backgroundImage.fit === 'fill'
                                                                              ? '100% 100%'
                                                                              : draft.backgroundImage.fit === 'tile'
                                                                                ? `${draft.backgroundImage.tile_scale ?? 25}% auto`
                                                                                : 'cover',
                                                                    backgroundRepeat:
                                                                        draft.backgroundImage.fit === 'tile' ? 'repeat' : 'no-repeat',
                                                                    backgroundPosition: `${draft.backgroundImage.offset_x ?? 50}% ${draft.backgroundImage.offset_y ?? 50}%`,
                                                                }}
                                                            />
                                                            <div className="min-w-0 flex-1">
                                                                <p className="truncate font-mono text-[12px] text-gray-300">
                                                                    {draft.backgroundImage.path}
                                                                </p>
                                                                <p className="mt-0.5 text-[11px] text-gray-400">
                                                                    Copied into the themes folder, so the theme stays shareable.
                                                                </p>
                                                            </div>
                                                            <button
                                                                disabled={!editable}
                                                                onClick={() => { setDraft({ ...draft, backgroundImage: null }); setDirty(true); }}
                                                                className="shrink-0 rounded-lg border border-gray-600 px-3 py-1.5 text-[12px] text-gray-300 transition-colors hover:border-gray-500 hover:bg-gray-700 disabled:opacity-50"
                                                            >
                                                                Remove
                                                            </button>
                                                        </div>

                                                        <div>
                                                            <div className="mb-2 flex items-baseline justify-between">
                                                                <span className="text-[12px] font-medium text-gray-300">Sizing</span>
                                                                <span className="font-mono text-[11px] capitalize text-gray-400">
                                                                    {draft.backgroundImage.fit || 'cover'}
                                                                </span>
                                                            </div>
                                                            <div className="grid grid-cols-5 gap-1.5 rounded-xl border border-gray-700 bg-gray-900/70 p-1">
                                                                {([
                                                                    { id: 'cover', label: 'Cover', desc: 'Fills, crops' },
                                                                    { id: 'contain', label: 'Contain', desc: 'Fits, whole' },
                                                                    { id: 'fill', label: 'Stretch', desc: 'Distorts' },
                                                                    { id: 'tile', label: 'Pattern', desc: 'Repeats' },
                                                                    { id: 'center', label: 'Original', desc: 'True size' },
                                                                ] as const).map((mode) => {
                                                                    const active = (draft.backgroundImage?.fit || 'cover') === mode.id;
                                                                    return (
                                                                        <button
                                                                            key={mode.id}
                                                                            type="button"
                                                                            disabled={!editable}
                                                                            onClick={() => updateImage({ fit: mode.id })}
                                                                            className={`flex flex-col items-center justify-center rounded-lg px-2 py-1.5 text-center transition-all disabled:opacity-50 ${
                                                                                active
                                                                                    ? 'bg-blue-600 font-semibold text-on-accent shadow-sm'
                                                                                    : 'text-gray-400 hover:bg-gray-800 hover:text-white'
                                                                            }`}
                                                                            title={`${mode.label} — ${mode.desc}`}
                                                                        >
                                                                            <span className="text-[12px] leading-tight">{mode.label}</span>
                                                                            <span className={`text-[9px] leading-tight ${active ? 'text-on-accent/70' : 'text-gray-400'}`}>
                                                                                {mode.desc}
                                                                            </span>
                                                                        </button>
                                                                    );
                                                                })}
                                                            </div>
                                                        </div>

                                                        {draft.backgroundImage.fit === 'tile' && (
                                                            <div>
                                                                <div className="mb-1.5 flex items-baseline justify-between">
                                                                    <span className="text-[12px] text-gray-300">Pattern scale</span>
                                                                    <span className="font-mono text-[11px] text-gray-400">
                                                                        {Math.round(draft.backgroundImage.tile_scale ?? 25)}%
                                                                    </span>
                                                                </div>
                                                                <Slider
                                                                    ariaLabel="Pattern scale"
                                                                    value={draft.backgroundImage.tile_scale ?? 25}
                                                                    min={2} max={100} step={1}
                                                                    disabled={!editable}
                                                                    onChange={(n) => updateImage({ tile_scale: n })}
                                                                />
                                                            </div>
                                                        )}

                                                        <div className="grid grid-cols-2 gap-4">
                                                            <div>
                                                                <div className="mb-1.5 flex items-baseline justify-between">
                                                                    <span className="text-[12px] text-gray-300">Visibility</span>
                                                                    <span className="font-mono text-[11px] text-gray-400">
                                                                        {Math.round(draft.backgroundImage.opacity * 100)}%
                                                                    </span>
                                                                </div>
                                                                <Slider
                                                                    ariaLabel="Background visibility"
                                                                    value={draft.backgroundImage.opacity}
                                                                    min={0} max={1} step={0.01}
                                                                    disabled={!editable}
                                                                    onChange={(n) => updateImage({ opacity: n })}
                                                                />
                                                            </div>
                                                            <div>
                                                                <div className="mb-1.5 flex items-baseline justify-between">
                                                                    <span className="text-[12px] text-gray-300">Blur</span>
                                                                    <span className="font-mono text-[11px] text-gray-400">
                                                                        {Math.round(draft.backgroundImage.blur)}px
                                                                    </span>
                                                                </div>
                                                                <Slider
                                                                    ariaLabel="Background blur"
                                                                    value={draft.backgroundImage.blur}
                                                                    min={0} max={40} step={1}
                                                                    disabled={!editable}
                                                                    onChange={(n) => updateImage({ blur: n })}
                                                                />
                                                            </div>
                                                            <div>
                                                                <div className="mb-1.5 flex items-baseline justify-between">
                                                                    <span className="text-[12px] text-gray-300">Horizontal position</span>
                                                                    <span className="font-mono text-[11px] text-gray-400">
                                                                        {Math.round(draft.backgroundImage.offset_x ?? 50)}%
                                                                    </span>
                                                                </div>
                                                                <Slider
                                                                    ariaLabel="Horizontal position"
                                                                    value={draft.backgroundImage.offset_x ?? 50}
                                                                    min={0} max={100} step={1}
                                                                    disabled={!editable}
                                                                    onChange={(n) => updateImage({ offset_x: n })}
                                                                />
                                                            </div>
                                                            <div>
                                                                <div className="mb-1.5 flex items-baseline justify-between">
                                                                    <span className="text-[12px] text-gray-300">Vertical position</span>
                                                                    <span className="font-mono text-[11px] text-gray-400">
                                                                        {Math.round(draft.backgroundImage.offset_y ?? 50)}%
                                                                    </span>
                                                                </div>
                                                                <Slider
                                                                    ariaLabel="Vertical position"
                                                                    value={draft.backgroundImage.offset_y ?? 50}
                                                                    min={0} max={100} step={1}
                                                                    disabled={!editable}
                                                                    onChange={(n) => updateImage({ offset_y: n })}
                                                                />
                                                            </div>
                                                        </div>
                                                    </div>
                                                ) : (
                                                    <div className="flex items-center justify-between gap-4 p-4">
                                                        <div>
                                                            <p className="text-[15px] font-medium text-white">Background image</p>
                                                            <p className="mt-0.5 text-[13px] leading-snug text-gray-400">
                                                                Sits behind the app. Panels stay solid so text keeps its contrast.
                                                                PNG, JPEG, WebP and GIF are accepted.
                                                            </p>
                                                        </div>
                                                        <button
                                                            disabled={!editable || busy}
                                                            onClick={() => void handlePickImage()}
                                                            className="shrink-0 rounded-lg border border-gray-600 px-4 py-2 text-[13px] font-medium text-gray-200 transition-colors hover:border-gray-500 hover:bg-gray-700 disabled:opacity-50"
                                                        >
                                                            Choose image
                                                        </button>
                                                    </div>
                                                )}
                                            </Section>
                                        </div>
                                    </div>
                                ) : (
                                    <ThemeTomlEditor
                                        fileName={editable ? selectedId : null}
                                        readOnlySource={themeToToml(draft)}
                                        editable={editable}
                                        onSaved={async () => {
                                            // The file is the truth now; drop the draft so the
                                            // colour controls rebuild from what was written.
                                            setDirty(false);
                                            setPreview(null);
                                            await loadThemes();
                                        }}
                                    />
                                )}
                            </>
                        )}
                    </div>
                </div>

                <div className="flex shrink-0 items-center justify-between gap-3 border-t border-gray-800 bg-gray-900 px-7 py-5">
                    <div>
                        {editable && (
                            <button
                                onClick={() => void handleDelete()}
                                className="rounded-xl border border-fg-danger/40 bg-fg-danger/10 px-4 py-2 text-[13px] font-semibold text-fg-danger transition-all hover:border-fg-danger/60 hover:bg-fg-danger/20"
                            >
                                Delete theme
                            </button>
                        )}
                    </div>
                    <div className="flex items-center gap-3">
                        {dirty && <span className="text-[12px] text-fg-warning">Unsaved changes</span>}
                        <Button variant="secondary" onClick={() => void handleClose()}>Close</Button>
                        <Button variant="primary" onClick={() => void handleSave()} disabled={!dirty || saving || !editable}>
                            {saving ? 'Saving…' : 'Save theme'}
                        </Button>
                    </div>
                </div>
            </div>
        </div>
    );
}
