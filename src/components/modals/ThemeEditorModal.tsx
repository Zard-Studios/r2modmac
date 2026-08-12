import {
    memo,
    useCallback,
    useEffect,
    useMemo,
    useRef,
    useState,
    type CSSProperties,
} from 'react';

import { Button } from '../ui';
import { AppIcon } from '../ui/icons';
import { Slider } from '../ui/Slider';
import { ColorField } from '../ui/ColorPicker';
import { Toggle } from '../ui/Toggle';
import { GameCard } from '../game/GameSelector';
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
    parseHex,
    backgroundLayerStyle,
    themeStyleVariables,
    themeToToml,
    type Theme,
    type ThemeColors,
} from '../../utils/theme';
import { patchThemeToml, themeEdits } from '../../utils/themeFile';
import { allBuiltinThemes, isBuiltinId, type ThemePreset } from '../../utils/themePresets';
import { formatAccelerator, isTextFieldTarget } from '../../utils/keybinds';
import type { ThemeSummary } from '../../types/electron';

interface ThemeEditorModalProps {
    isOpen: boolean;
    onClose: () => void;
}

/**
 * Where the cover specimen starts in the library, drawn once per run.
 *
 * Module scope on purpose: the editor walks one game forward on each opening,
 * which alone would show the same first cover every launch. Drawing this while
 * rendering is what the purity rule forbids, and rightly — a render that picks
 * a different game each time it runs would never settle.
 */
const PREVIEW_GAME_SEED = Math.floor(Math.random() * 1013);

const channels = (color: string) => {
    const rgb = parseHex(color);
    return rgb ? `${rgb.r} ${rgb.g} ${rgb.b}` : '0 0 0';
};

const hslToHex = (hue: number, saturation: number, lightness: number) => {
    const h = ((hue % 360) + 360) % 360;
    const s = saturation / 100;
    const l = lightness / 100;
    const chroma = (1 - Math.abs(2 * l - 1)) * s;
    const x = chroma * (1 - Math.abs(((h / 60) % 2) - 1));
    const m = l - chroma / 2;
    const [r, g, b] = h < 60 ? [chroma, x, 0]
        : h < 120 ? [x, chroma, 0]
            : h < 180 ? [0, chroma, x]
                : h < 240 ? [0, x, chroma]
                    : h < 300 ? [x, 0, chroma]
                        : [chroma, 0, x];
    const channel = (value: number) => Math.round((value + m) * 255).toString(16).padStart(2, '0');
    return `#${channel(r)}${channel(g)}${channel(b)}`;
};

/** 4-swatch pill preview showing the theme's core color identity. */
function SwatchStrip({ colors, className = '' }: { colors: ThemeColors; className?: string }) {
    return (
        <div className={`flex overflow-hidden rounded-md border border-gray-700 ${className}`}>
            {[colors.background, colors.surface, colors.accent, colors.text].map((color, i) => (
                <span key={i} className="h-full flex-1" style={{ backgroundColor: color }} />
            ))}
        </div>
    );
}

/** Standard r2modmac Color Row matching Preferences row styling */
const ColorRow = memo(function ColorRow({
    colorKey,
    value,
    presets,
    opacity,
    onChange,
    onOpacityChange,
    onInteractionStart,
    onInteractionEnd,
    portalTarget,
    disabled,
}: {
    colorKey: keyof ThemeColors;
    value: string;
    presets: string[];
    opacity: number;
    onChange: (key: keyof ThemeColors, next: string) => void;
    onOpacityChange: (key: keyof ThemeColors, next: number) => void;
    onInteractionStart: () => void;
    onInteractionEnd: () => void;
    portalTarget?: Element | null;
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
        <div className={`p-4 flex items-center justify-between gap-4 transition-colors hover:bg-surface-hover ${disabled ? 'opacity-60' : ''}`}>
            <div className="flex items-center gap-3.5 min-w-0">
                <div className="shrink-0">
                    {disabled ? (
                        <span
                            className="block h-9 w-9 rounded-lg border border-gray-600"
                            style={{ backgroundColor: value, opacity }}
                        />
                    ) : (
                        <ColorField
                            label={meta.label}
                            value={value}
                            presets={presets}
                            opacity={opacity}
                            onChange={(next) => onChange(colorKey, next)}
                            onOpacityChange={(next) => onOpacityChange(colorKey, next)}
                            onInteractionStart={onInteractionStart}
                            onInteractionEnd={onInteractionEnd}
                            portalTarget={portalTarget}
                        />
                    )}
                </div>
                <div className="min-w-0">
                    <p className="text-[15px] font-medium text-white">{meta.label}</p>
                    <p className="text-[13px] text-gray-400 mt-0.5 leading-snug truncate">{meta.description}</p>
                </div>
            </div>

            <div className="flex shrink-0 items-center gap-3">
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
            </div>
        </div>
    );
});


/**
 * A segmented control whose selection can be scrubbed like a slider.
 *
 * The moving background is the same gesture the platform filter on the game
 * list uses, with the same timing, so the two read as one idea rather than two
 * takes on it. Dragging across the strip walks the options: with five sizings
 * that each change the picture behind them, sweeping through to see them is
 * quicker than clicking one, looking, clicking the next.
 */
/** The `p-1` on the track, in pixels, needed for the maths above. */
const PADDING = 4;

function SegmentedControl<T extends string>({
    options,
    value,
    onChange,
    onScrubStart,
    onScrubEnd,
    disabled = false,
    ariaLabel,
}: {
    options: readonly { id: T; label: string }[];
    value: T;
    onChange: (id: T) => void;
    onScrubStart?: (id: T) => void;
    onScrubEnd?: () => void;
    disabled?: boolean;
    ariaLabel: string;
}) {
    const trackRef = useRef<HTMLDivElement>(null);
    const scrubbingRef = useRef(false);
    const selectedIndex = Math.max(0, options.findIndex((option) => option.id === value));

    /** Which segment the pointer is over, clamped to the ends of the strip. */
    const indexAt = useCallback((clientX: number) => {
        const track = trackRef.current;
        if (!track) return selectedIndex;
        const rect = track.getBoundingClientRect();
        // The padding either side is not part of any segment; without removing
        // it the last option would only be reachable past the right edge.
        const inner = rect.width - PADDING * 2;
        const position = (clientX - rect.left - PADDING) / Math.max(inner, 1);
        return Math.min(options.length - 1, Math.max(0, Math.floor(position * options.length)));
    }, [options.length, selectedIndex]);

    const moveTo = useCallback((index: number) => {
        const option = options[index];
        if (option && option.id !== value) onChange(option.id);
    }, [onChange, options, value]);

    return (
        <div
            ref={trackRef}
            role="radiogroup"
            aria-label={ariaLabel}
            className="relative grid rounded-xl border border-gray-700 bg-gray-900/70 p-1"
            style={{ gridTemplateColumns: `repeat(${options.length}, minmax(0, 1fr))` }}
            onPointerDown={(event) => {
                if (disabled || event.button !== 0) return;
                event.currentTarget.setPointerCapture(event.pointerId);
                scrubbingRef.current = true;
                const index = indexAt(event.clientX);
                moveTo(index);
                onScrubStart?.(options[index].id);
            }}
            onPointerMove={(event) => {
                if (!scrubbingRef.current) return;
                moveTo(indexAt(event.clientX));
            }}
            onPointerUp={() => {
                if (!scrubbingRef.current) return;
                scrubbingRef.current = false;
                onScrubEnd?.();
            }}
            onPointerCancel={() => {
                if (!scrubbingRef.current) return;
                scrubbingRef.current = false;
                onScrubEnd?.();
            }}
        >
            {/* The travelling selection. Width is a share of the track so the
                strip stays correct at any size, and the transform is what
                animates — moving `left` would repaint on every frame. */}
            <span
                aria-hidden="true"
                className={`pointer-events-none absolute bottom-1 top-1 left-1 rounded-lg bg-blue-600 shadow-sm transition-transform duration-300 ease-[cubic-bezier(0.25,0.1,0.25,1)] ${
                    disabled ? 'opacity-50' : ''
                }`}
                style={{
                    width: `calc((100% - ${PADDING * 2}px) / ${options.length})`,
                    transform: `translateX(${selectedIndex * 100}%)`,
                }}
            />

            {options.map((option, index) => {
                const active = index === selectedIndex;
                return (
                    <button
                        key={option.id}
                        type="button"
                        role="radio"
                        aria-checked={active}
                        disabled={disabled}
                        // The strip owns the pointer while scrubbing, so a click
                        // here only has to cover keyboard and assistive use.
                        onClick={() => moveTo(index)}
                        onKeyDown={(event) => {
                            if (event.key === 'ArrowRight' || event.key === 'ArrowDown') {
                                event.preventDefault();
                                moveTo(Math.min(options.length - 1, index + 1));
                            }
                            if (event.key === 'ArrowLeft' || event.key === 'ArrowUp') {
                                event.preventDefault();
                                moveTo(Math.max(0, index - 1));
                            }
                        }}
                        className={`relative z-10 flex select-none items-center justify-center rounded-lg px-2 py-2 text-center transition-colors disabled:opacity-50 ${
                            active ? 'font-semibold text-on-accent' : 'text-gray-400 hover:text-white'
                        }`}
                    >
                        <span className="pointer-events-none text-[12px] leading-tight">{option.label}</span>
                    </button>
                );
            })}
        </div>
    );
}

function BackgroundCanvas({
    theme,
    imageUrl,
    pinned,
    visible,
    onClose,
}: {
    theme: Theme;
    imageUrl: string | null;
    pinned: boolean;
    visible: boolean;
    onClose: () => void;
}) {
    const image = theme.backgroundImage;
    if (!image) return null;

    // The same layout the window itself uses, so the preview cannot lie.
    const layer = backgroundLayerStyle(image);

    return (
        <div
            className={`theme-background-canvas absolute inset-0 z-50 overflow-hidden transition-opacity duration-200 ease-out ${visible ? 'opacity-100' : 'opacity-0'} ${pinned && visible ? 'pointer-events-auto' : 'pointer-events-none'}`}
            style={{ backgroundColor: `rgb(${channels(theme.colors.background)} / ${theme.opacity?.background ?? 1})` }}
            onClick={pinned ? onClose : undefined}
            role={pinned ? 'button' : undefined}
            tabIndex={pinned ? 0 : undefined}
            aria-label={pinned ? 'Return to the theme editor' : undefined}
        >
            <div
                className="absolute inset-0"
                style={{
                    backgroundImage: imageUrl ? `url("${imageUrl}")` : undefined,
                    backgroundSize: layer.backgroundSize,
                    backgroundRepeat: layer.backgroundRepeat,
                    backgroundPosition: layer.backgroundPosition,
                    filter: layer.filter,
                    transform: `scale(${layer.scale}) translateZ(0)`,
                }}
            />
            <div
                className="absolute inset-0"
                style={{
                    backgroundColor: `rgb(${channels(theme.colors.background)} / ${theme.opacity?.background ?? 1})`,
                    opacity: 1 - image.opacity,
                }}
            />
        </div>
    );
}

export function ThemeEditorModal({ isOpen, onClose }: ThemeEditorModalProps) {
    const communities = useAppStore((s) => s.communities);
    const communityImages = useAppStore((s) => s.communityImages);
    const communityPlatforms = useAppStore((s) => s.communityPlatforms);
    const themes = useThemeStore((s) => s.themes);
    const activeFileName = useThemeStore((s) => s.activeFileName);
    const loadThemes = useThemeStore((s) => s.loadThemes);
    const setActive = useThemeStore((s) => s.setActive);
    const setPreview = useThemeStore((s) => s.setPreview);

    const [draft, setDraft] = useState<Theme | null>(null);
    const draftRef = useRef<Theme | null>(null);
    const [dirty, setDirty] = useState(false);
    const [visualUndo, setVisualUndo] = useState<Theme[]>([]);
    const [visualRedo, setVisualRedo] = useState<Theme[]>([]);
    const [visualBaseline, setVisualBaseline] = useState<Theme | null>(null);
    const visualGestureBaseRef = useRef<Theme | null>(null);
    const [view, setView] = useState<'colours' | 'toml'>('colours');
    const [collapsedGroups, setCollapsedGroups] = useState<Set<string>>(new Set());
    const [searchQuery, setSearchQuery] = useState('');
    const [saving, setSaving] = useState(false);
    const [busy, setBusy] = useState(false);
    const [imageUrl, setImageUrl] = useState<string | null>(null);
    const [fileSource, setFileSource] = useState<string | null>(null);
    const [selectedGameId, setSelectedGameId] = useState<string | null>(null);
    const [openCount, setOpenCount] = useState(0);
    const [isFavoriteSample, setIsFavoriteSample] = useState(false);
    const [backgroundPreviewHeld, setBackgroundPreviewHeld] = useState(false);
    const [backgroundPreviewPinned, setBackgroundPreviewPinned] = useState(false);
    const [backgroundPreviewControl, setBackgroundPreviewControl] = useState<
        'sizing' | 'tile-scale' | 'opacity' | 'blur' | 'offset-x' | 'offset-y' | null
    >(null);
    const [previewRoot, setPreviewRoot] = useState<HTMLDivElement | null>(null);

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

    const editable = !!selectedId && !isBuiltinId(selectedId) && !!file;
    const [wasOpen, setWasOpen] = useState(isOpen);
    const signature = builtin ? `builtin:${builtin.id}` : file ? JSON.stringify(file) : 'none';
    const [draftSignature, setDraftSignature] = useState<string | null>(null);

    if (isOpen !== wasOpen) {
        setWasOpen(isOpen);
        if (isOpen) {
            setDirty(false);
            setVisualUndo([]);
            setVisualRedo([]);
            setVisualBaseline(null);
            setView('colours');
            setDraftSignature(null);
            setBackgroundPreviewHeld(false);
            setBackgroundPreviewPinned(false);
            setBackgroundPreviewControl(null);
            // Judging cover chrome against the same box art every time says
            // little about how the colours hold up over the rest of a library,
            // so each opening drops the last choice and moves on one game.
            setSelectedGameId(null);
            setOpenCount((count) => count + 1);
        }
    }

    if (isOpen && !dirty && draftSignature !== signature) {
        const nextDraft = builtin ? normalizeTheme(builtin) : file ? summaryToTheme(file) : null;
        setDraftSignature(signature);
        setDraft(nextDraft);
        setVisualUndo([]);
        setVisualRedo([]);
        setVisualBaseline(nextDraft);
    }

    useEffect(() => {
        draftRef.current = draft;
    }, [draft]);

    useEffect(() => {
        visualGestureBaseRef.current = null;
    }, [isOpen, signature]);

    useEffect(() => {
        if (isOpen) void loadThemes();
    }, [isOpen, loadThemes]);


    // Drafts stay inside this editor. Repainting the whole application on each
    // pointer move makes the colour picker lag and changes the app before Save.
    useEffect(() => () => setPreview(null), [setPreview]);

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

    // The file's own text, kept beside the parsed draft so saving can edit it
    // rather than replace it. Re-read whenever the theme list reloads, so an
    // edit made in another editor is the thing being patched.
    const sourceKey = editable ? selectedId : null;
    const [sourceFor, setSourceFor] = useState<string | null>(null);
    if (sourceKey !== sourceFor) {
        setSourceFor(sourceKey);
        setFileSource(null);
    }

    useEffect(() => {
        if (!sourceKey) return;
        let cancelled = false;
        void window.ipcRenderer
            .readThemeSource(sourceKey)
            .then((text) => { if (!cancelled) setFileSource(text); })
            .catch(() => { if (!cancelled) setFileSource(null); });
        return () => { cancelled = true; };
    }, [sourceKey, themes]);

    /**
     * What Save would write: the file with this session's changes applied.
     *
     * Falls back to a generated file only when there is nothing to patch — a
     * theme just created, or a file that could not be read.
     */
    const draftToml = useCallback(
        (theme: Theme) => {
            if (fileSource === null || visualBaseline === null) return themeToToml(theme);
            return patchThemeToml(fileSource, themeEdits(visualBaseline, theme));
        },
        [fileSource, visualBaseline]
    );

    const presetKey = draft ? THEME_COLOR_KEYS.map((k) => draft.colors[k]).join('|') : '';
    const swatchPresets = useMemo(
        () => (presetKey ? presetKey.split('|') : []),
        [presetKey]
    );

    const listColors = useMemo(
        () => new Map(themes.map((t) => [t.file_name, summaryToTheme(t).colors])),
        [themes]
    );

    const applyVisualEdit = useCallback((change: (theme: Theme) => Theme, record = true) => {
        const current = draftRef.current;
        if (!current) return;
        const next = change(current);
        if (next === current) return;
        if (record && !visualGestureBaseRef.current) {
            setVisualUndo((history) => [...history, current].slice(-100));
            setVisualRedo([]);
        }
        draftRef.current = next;
        setDraft(next);
        setDirty(JSON.stringify(next) !== JSON.stringify(visualBaseline));
    }, [visualBaseline]);

    const beginVisualGesture = useCallback(() => {
        const current = draftRef.current;
        if (current && !visualGestureBaseRef.current) visualGestureBaseRef.current = current;
    }, []);

    const endVisualGesture = useCallback(() => {
        const base = visualGestureBaseRef.current;
        const current = draftRef.current;
        visualGestureBaseRef.current = null;
        if (!base || !current || current === base) return;
        setVisualUndo((history) => [...history, base].slice(-100));
        setVisualRedo([]);
    }, []);

    const undoVisual = useCallback(() => {
        const previous = visualUndo[visualUndo.length - 1];
        if (!previous || !draft) return;
        setVisualUndo((history) => history.slice(0, -1));
        setVisualRedo((history) => [...history, draft].slice(-100));
        draftRef.current = previous;
        setDraft(previous);
        setDirty(JSON.stringify(previous) !== JSON.stringify(visualBaseline));
    }, [draft, visualBaseline, visualUndo]);

    const redoVisual = useCallback(() => {
        const next = visualRedo[visualRedo.length - 1];
        if (!next || !draft) return;
        setVisualRedo((history) => history.slice(0, -1));
        setVisualUndo((history) => [...history, draft].slice(-100));
        draftRef.current = next;
        setDraft(next);
        setDirty(JSON.stringify(next) !== JSON.stringify(visualBaseline));
    }, [draft, visualBaseline, visualRedo]);

    const updateColor = useCallback((key: keyof ThemeColors, value: string) => {
        applyVisualEdit((theme) => ({ ...theme, colors: { ...theme.colors, [key]: value } }));
    }, [applyVisualEdit]);

    const updateOpacity = useCallback((key: keyof ThemeColors, value: number) => {
        applyVisualEdit((theme) => ({
            ...theme,
            opacity: { ...theme.opacity, [key]: Math.max(0, Math.min(1, value)) },
        }));
    }, [applyVisualEdit]);

    const surpriseMe = useCallback(() => {
        if (!draft) return;
        const next = (() => {
            const hue = Math.floor(Math.random() * 360);
            const accentHue = (hue + 105 + Math.floor(Math.random() * 150)) % 360;
            const light = Math.random() < 0.22;
            const colors: ThemeColors = light
                ? {
                    background: hslToHex(hue, 18, 96),
                    surface: hslToHex(hue, 16, 100),
                    surface_hover: hslToHex(hue, 20, 91),
                    border: hslToHex(hue, 16, 76),
                    text: hslToHex(hue, 28, 10),
                    text_muted: hslToHex(hue, 12, 38),
                    accent: hslToHex(accentHue, 72, 44),
                    accent_hover: hslToHex(accentHue, 76, 36),
                    danger: hslToHex(4, 72, 46),
                    warning: hslToHex(38, 82, 42),
                    success: hslToHex(145, 62, 35),
                }
                : {
                    background: hslToHex(hue, 27, 7),
                    surface: hslToHex(hue, 23, 13),
                    surface_hover: hslToHex(hue, 21, 19),
                    border: hslToHex(hue, 16, 28),
                    text: hslToHex(hue, 18, 96),
                    text_muted: hslToHex(hue, 10, 66),
                    accent: hslToHex(accentHue, 76, 58),
                    accent_hover: hslToHex(accentHue, 80, 66),
                    danger: hslToHex(4, 78, 61),
                    warning: hslToHex(38, 88, 58),
                    success: hslToHex(145, 64, 49),
                };

            return {
                ...draft,
                colors: {
                    ...draft.colors,
                    ...colors,
                    media_scrim: light ? '#111827' : colors.background,
                    media_ink: '#ffffff',
                },
            };
        })();
        applyVisualEdit(() => next);
        // Surprise is a discrete action, not a continuous drag: one global
        // repaint lets the user judge the palette in the real app without the
        // colour picker's per-frame cost.
        setPreview(next);
    }, [applyVisualEdit, draft, setPreview]);

    const updateImage = useCallback((patch: Partial<NonNullable<Theme['backgroundImage']>>, record = true) => {
        applyVisualEdit((theme) => {
            if (!theme.backgroundImage) return theme;
            return { ...theme, backgroundImage: { ...theme.backgroundImage, ...patch } };
        }, record);
    }, [applyVisualEdit]);

    const beginSizingPreview = useCallback((fit: NonNullable<Theme['backgroundImage']>['fit']) => {
        applyVisualEdit((theme) => {
            if (!theme.backgroundImage || theme.backgroundImage.fit === fit) return theme;
            return { ...theme, backgroundImage: { ...theme.backgroundImage, fit } };
        });
        setBackgroundPreviewControl('sizing');
        setBackgroundPreviewHeld(true);
    }, [applyVisualEdit]);

    const endBackgroundPreview = useCallback(() => {
        setBackgroundPreviewHeld(false);
        setBackgroundPreviewControl(null);
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
            // The draft being abandoned may still be painting the window — a
            // "Surprise me!" commits one globally. Without dropping it here the
            // app stayed on the discarded palette while the sidebar showed the
            // theme just chosen.
            setPreview(null);
            await setActive(id);
        },
        [dirty, setActive, setPreview]
    );

    const handleSave = useCallback(async () => {
        if (!draft || !editable || !selectedId || !dirty) return;
        setSaving(true);
        try {
            const written = draftToml(draft);
            await window.ipcRenderer.writeTheme(selectedId, written);
            forgetBackgroundImage(draft.backgroundImage?.path);
            await loadThemes();
            // Held from what was just written rather than waiting for the reread
            // the watcher will trigger: the next save patches this text, and a
            // second save arriving first would otherwise undo this one.
            setFileSource(written);
            setVisualBaseline(draft);
            setDirty(false);
            setVisualUndo([]);
            setVisualRedo([]);
            setPreview(null);
        } catch (error) {
            await window.ipcRenderer.alert('Could not save the theme', String(error));
        } finally {
            setSaving(false);
        }
    }, [draft, editable, selectedId, dirty, draftToml, loadThemes, setPreview]);

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
    }, [file, setActive, loadThemes, setPreview]);

    const handlePickImage = useCallback(async () => {
        setBusy(true);
        try {
            const path = await window.ipcRenderer.selectFile([
                { name: 'Images', extensions: ['png', 'jpg', 'jpeg', 'webp', 'gif', 'bmp'] },
            ]);
            if (!path) return;
            const relative = await window.ipcRenderer.importThemeImage(path);
            forgetBackgroundImage(relative);
            applyVisualEdit((theme) => ({
                ...theme,
                backgroundImage: { path: relative, opacity: 0.35, blur: 0, fit: 'cover', offset_x: 50, offset_y: 50 },
            }));
        } catch (error) {
            await window.ipcRenderer.alert('Could not use that image', String(error));
        } finally {
            setBusy(false);
        }
    }, [applyVisualEdit]);

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
            const modifier = e.metaKey || e.ctrlKey;
            const key = e.key.toLowerCase();
            // The name and author boxes are text fields inside the editor, and
            // this listener sits on the document: without standing aside, undo
            // there rolled back a colour instead of the letter just typed.
            const editingText = isTextFieldTarget(e.target as HTMLElement | null);
            if (view === 'colours' && modifier && !editingText && key === 'z') {
                e.preventDefault();
                if (e.shiftKey) redoVisual();
                else undoVisual();
                return;
            }
            if (view === 'colours' && modifier && !editingText && key === 'y') {
                e.preventDefault();
                redoVisual();
                return;
            }
            if (modifier && key === 's') {
                if (view === 'toml') return;
                e.preventDefault();
                void handleSave();
            }
        };
        document.addEventListener('keydown', onKeyDown);
        return () => document.removeEventListener('keydown', onKeyDown);
    }, [isOpen, handleSave, view, undoVisual, redoVisual]);

    useEffect(() => {
        if (!isOpen || !backgroundPreviewPinned) return;
        const leavePreview = (event: KeyboardEvent) => {
            if (event.key !== 'Escape') return;
            event.preventDefault();
            event.stopPropagation();
            setBackgroundPreviewPinned(false);
        };
        window.addEventListener('keydown', leavePreview, true);
        return () => window.removeEventListener('keydown', leavePreview, true);
    }, [isOpen, backgroundPreviewPinned]);

    const warnings = useMemo(() => (draft ? findContrastWarnings(draft.colors) : []), [draft]);
    const autoContrast = draft?.options?.autoContrast ?? DEFAULT_THEME_OPTIONS.autoContrast;
    const interfaceBlur = draft?.options?.interfaceBlur ?? DEFAULT_THEME_OPTIONS.interfaceBlur ?? 0;

    // Active sample game for cover preview
    const activeGame = useMemo(() => {
        if (communities.length === 0) return null;
        if (selectedGameId) {
            const found = communities.find((c) => c.identifier === selectedGameId);
            if (found) return found;
        }
        // Games whose cover has arrived come first: a specimen with no artwork
        // shows none of the chrome these colours are here to be judged against.
        const withArt = communities.filter((c) => communityImages[c.identifier]);
        const pool = withArt.length > 0 ? withArt : communities;
        return pool[(PREVIEW_GAME_SEED + openCount) % pool.length];
    }, [communities, communityImages, selectedGameId, openCount]);

    const activeGameImage = activeGame ? communityImages[activeGame.identifier] : undefined;
    const activeGamePlatform = activeGame ? communityPlatforms[activeGame.identifier] : undefined;
    // The whole editor is painted with the draft's own tokens, so every specimen
    // inside answers to an unsaved change at once. Scoping a hand-picked subset
    // to one card left the rest reading the *applied* theme, which is why some
    // elements moved with the opacity slider and others sat still.
    const draftStyle = useMemo(() => {
        if (!draft) return undefined;
        const blur = Math.max(0, Math.min(40, draft.options?.interfaceBlur ?? 0));
        return {
            ...themeStyleVariables(draft),
            ...(blur > 0
                ? {
                    WebkitBackdropFilter: `blur(${blur}px)`,
                    backdropFilter: `blur(${blur}px)`,
                }
                : {}),
        } as CSSProperties;
    }, [draft]);

    // Filter themes for sidebar
    const filteredThemes = useMemo(() => {
        const q = searchQuery.trim().toLowerCase();
        if (!q) return themes;
        return themes.filter((t) => t.name.toLowerCase().includes(q) || (t.author && t.author.toLowerCase().includes(q)));
    }, [themes, searchQuery]);

    const filteredBuiltins = useMemo(() => {
        const q = searchQuery.trim().toLowerCase();
        if (!q) return builtins;
        return builtins.filter((b) => b.name.toLowerCase().includes(q) || (b.origin && b.origin.toLowerCase().includes(q)));
    }, [builtins, searchQuery]);

    const groupedThemes = useMemo(() => {
        const byAuthor = new Map<string, ThemeSummary[]>();
        for (const t of filteredThemes) {
            const author = t.author?.trim() || 'Unattributed';
            const list = byAuthor.get(author);
            if (list) list.push(t);
            else byAuthor.set(author, [t]);
        }
        return [...byAuthor.entries()].sort(([a], [b]) => {
            if (a === 'Unattributed') return 1;
            if (b === 'Unattributed') return -1;
            return a.localeCompare(b);
        });
    }, [filteredThemes]);

    if (!isOpen) return null;

    return (
        <div
            ref={setPreviewRoot}
            className="fixed inset-0 z-50 flex items-center justify-center p-4"
            style={draftStyle}
        >
            {draft?.backgroundImage && (
                <BackgroundCanvas
                    theme={draft}
                    imageUrl={imageUrl}
                    pinned={backgroundPreviewPinned}
                    visible={backgroundPreviewHeld || backgroundPreviewPinned}
                    onClose={() => setBackgroundPreviewPinned(false)}
                />
            )}
            {/* Backdrop */}
            <div className="absolute inset-0 bg-black/60 backdrop-blur-sm" onClick={() => void handleClose()} />

            {/* Modal Container */}
            <div
                className="relative flex h-[88vh] w-full max-w-[1180px] flex-col overflow-hidden rounded-2xl border border-gray-700 bg-gray-900 shadow-2xl"
                onClick={(e) => e.stopPropagation()}
            >
                {/* Header */}
                <div className="flex shrink-0 items-center justify-between border-b border-gray-700 bg-gray-900 px-6 py-4">
                    <div className="flex items-center gap-4 min-w-0">
                        <div className="min-w-0">
                            <div className="flex items-center gap-2">
                                {draft ? (
                                    <input
                                        value={draft.name}
                                        disabled={!editable}
                                        // Typed text is one gesture, like dragging a slider is:
                                        // recording a step per keystroke buried every colour
                                        // change under the letters of the name above it.
                                        onFocus={beginVisualGesture}
                                        onBlur={endVisualGesture}
                                        onChange={(e) => applyVisualEdit((theme) => ({ ...theme, name: e.target.value }), false)}
                                        aria-label="Theme name"
                                        placeholder="Theme Name"
                                        className="text-xl font-bold text-white tracking-tight bg-transparent border border-transparent rounded-lg px-2 py-0.5 hover:border-gray-700 focus:border-blue-500 focus:bg-gray-800 focus:outline-none disabled:hover:border-transparent min-w-0"
                                    />
                                ) : (
                                    <h2 className="text-xl font-bold text-white tracking-tight">Theme Editor</h2>
                                )}
                            </div>
                            {draft ? (
                                <input
                                    value={draft.author ?? ''}
                                    disabled={!editable}
                                    placeholder="Author name"
                                    onFocus={beginVisualGesture}
                                    onBlur={endVisualGesture}
                                    onChange={(e) => applyVisualEdit((theme) => ({ ...theme, author: e.target.value || undefined }), false)}
                                    aria-label="Theme author"
                                    className="text-xs text-gray-400 bg-transparent border border-transparent rounded-lg px-2 py-0.5 hover:border-gray-700 focus:border-blue-500 focus:bg-gray-800 focus:outline-none disabled:hover:border-transparent min-w-0"
                                />
                            ) : (
                                <p className="text-xs text-gray-400 px-2">Preview locally, apply on Save.</p>
                            )}
                        </div>
                    </div>

                    <div className="flex items-center gap-3">
                        {draft && view === 'colours' && (
                            <>
                            <button
                                type="button"
                                onClick={surpriseMe}
                                disabled={!editable}
                                className="inline-flex items-center gap-1.5 rounded-lg border border-gray-700 bg-gray-800 px-3 py-2 text-xs font-semibold text-white transition-colors hover:border-gray-600 hover:bg-gray-700 disabled:cursor-not-allowed disabled:opacity-40"
                                title="Generate a balanced palette"
                            >
                                <svg className="h-4 w-4 text-fg-accent" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.8} d="M12 3l1.25 3.75L17 8l-3.75 1.25L12 13l-1.25-3.75L7 8l3.75-1.25L12 3zm6 9l.8 2.2L21 15l-2.2.8L18 18l-.8-2.2L15 15l2.2-.8L18 12zM6 14l1.1 2.9L10 18l-2.9 1.1L6 22l-1.1-2.9L2 18l2.9-1.1L6 14z" />
                                </svg>
                                Surprise me!
                            </button>
                            <div className="flex items-center gap-1 bg-gray-800 border border-gray-700 rounded-lg p-1" aria-label="Visual edit history">
                                <button
                                    type="button"
                                    onClick={undoVisual}
                                    disabled={!editable || visualUndo.length === 0}
                                    title={`Undo (${formatAccelerator('Mod+Z')})`}
                                    aria-label="Undo visual change"
                                    className="p-1.5 rounded text-gray-400 hover:text-white hover:bg-gray-700 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
                                >
                                    <svg className="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 10h10a5 5 0 010 10h-3M3 10l4-4M3 10l4 4" />
                                    </svg>
                                </button>
                                <button
                                    type="button"
                                    onClick={redoVisual}
                                    disabled={!editable || visualRedo.length === 0}
                                    title={`Redo (${formatAccelerator('Mod+Shift+Z')})`}
                                    aria-label="Redo visual change"
                                    className="p-1.5 rounded text-gray-400 hover:text-white hover:bg-gray-700 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
                                >
                                    <svg className="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 10H11a5 5 0 000 10h3M21 10l-4-4M21 10l-4 4" />
                                    </svg>
                                </button>
                            </div>
                            </>
                        )}

                        {draft && (
                            // The same switch the mod list uses for grid and list: a
                            // neutral pill that slides between two equal halves. Two
                            // toggles of the same kind reading differently is what made
                            // this one look like a button that had been left pressed.
                            <div
                                className="relative flex overflow-hidden rounded-lg border border-gray-700 bg-gray-800 p-1"
                                role="group"
                                aria-label="Editor view"
                            >
                                <div
                                    aria-hidden="true"
                                    className={`absolute top-1 bottom-1 w-[calc(50%-4px)] rounded-md bg-gray-600 transition-all duration-300 ease-[cubic-bezier(0.25,0.1,0.25,1)] ${
                                        view === 'colours' ? 'left-1' : 'left-1/2'
                                    }`}
                                />
                                {(['colours', 'toml'] as const).map((nextView) => (
                                    <button
                                        key={nextView}
                                        type="button"
                                        onClick={() => setView(nextView)}
                                        aria-pressed={view === nextView}
                                        className={`relative z-10 flex-1 rounded-md px-3 py-1.5 text-[11px] font-medium transition-colors ${
                                            view === nextView ? 'text-white' : 'text-gray-400 hover:text-white'
                                        }`}
                                    >
                                        {nextView === 'colours' ? 'Visual' : 'TOML'}
                                    </button>
                                ))}
                            </div>
                        )}

                        <button
                            onClick={() => void handleClose()}
                            className="p-2 rounded-xl hover:bg-gray-800 text-gray-400 hover:text-white transition-all active:scale-95 focus:outline-none focus:ring-2 focus:ring-gray-700"
                            aria-label="Close"
                        >
                            <svg className="h-5 w-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M6 18L18 6M6 6l12 12" />
                            </svg>
                        </button>
                    </div>
                </div>

                {/* Body */}
                <div className="flex min-h-0 flex-1">
                    {/* Left Sidebar */}
                    <div className="flex w-64 shrink-0 flex-col border-r border-gray-700 bg-gray-900/50">
                        <div className="p-3">
                            <input
                                type="text"
                                value={searchQuery}
                                onChange={(e) => setSearchQuery(e.target.value)}
                                placeholder="Search themes..."
                                className="w-full bg-gray-800 border border-gray-700 rounded-lg px-3 py-1.5 text-xs text-white placeholder-gray-500 focus:outline-none focus:border-blue-500"
                            />
                        </div>

                        <div className="min-h-0 flex-1 space-y-4 overflow-y-auto p-3 pr-2 scrollbar-thin">
                            {/* Built-ins */}
                            {filteredBuiltins.length > 0 && (
                                <div className="space-y-2">
                                    <button
                                        type="button"
                                        onClick={() => {
                                            setCollapsedGroups((previous) => {
                                                const next = new Set(previous);
                                                if (next.has('built-in')) next.delete('built-in');
                                                else next.add('built-in');
                                                return next;
                                            });
                                        }}
                                        className="flex w-full items-center justify-between px-1 text-left text-[11px] font-semibold uppercase tracking-wider text-gray-400 hover:text-gray-300"
                                    >
                                        <span className="flex items-center gap-1">
                                            <svg className={`h-3 w-3 text-gray-500 transition-transform ${collapsedGroups.has('built-in') ? '-rotate-90' : ''}`} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.5} d="M19 9l-7 7-7-7" />
                                            </svg>
                                            Built-in
                                        </span>
                                        <span className="text-[10px] text-gray-500">{filteredBuiltins.length}</span>
                                    </button>
                                    {!collapsedGroups.has('built-in') && <div className="space-y-1">
                                        {filteredBuiltins.map((b) => {
                                            const isSelected = selectedId === b.id;
                                            return (
                                                <button
                                                    key={b.id}
                                                    onClick={() => void handleSelect(b.id)}
                                                    title={b.origin ?? 'The stock r2modmac look'}
                                                    className={`flex w-full items-center gap-2.5 rounded-lg border p-2 text-left transition-colors ${
                                                        isSelected
                                                            ? 'border-blue-500/60 bg-blue-500/10'
                                                            : 'border-gray-700 hover:border-gray-600 hover:bg-gray-800'
                                                    }`}
                                                >
                                                    <SwatchStrip colors={b.colors} className="h-8 w-8 shrink-0" />
                                                    <span className="min-w-0 flex-1 truncate text-[13px] font-medium text-white">
                                                        {b.name}
                                                    </span>
                                                    {isSelected && (
                                                        <AppIcon name="apply" className="h-4 w-4 shrink-0 text-fg-accent" />
                                                    )}
                                                </button>
                                            );
                                        })}
                                    </div>}
                                </div>
                            )}

                            {/* Grouped custom themes */}
                            {groupedThemes.map(([author, list]) => {
                                const groupId = `author:${author}`;
                                const open = !collapsedGroups.has(groupId);
                                return (
                                    <div key={groupId} className="space-y-2">
                                        <button
                                            type="button"
                                            onClick={() => {
                                                setCollapsedGroups((prev) => {
                                                    const next = new Set(prev);
                                                    if (next.has(groupId)) next.delete(groupId);
                                                    else next.add(groupId);
                                                    return next;
                                                });
                                            }}
                                            className="flex w-full items-center justify-between px-1 text-left text-[11px] font-semibold uppercase tracking-wider text-gray-400 hover:text-gray-300"
                                        >
                                            <div className="flex items-center gap-1 truncate">
                                                <svg className={`h-3 w-3 text-gray-500 transition-transform ${open ? '' : '-rotate-90'}`} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.5} d="M19 9l-7 7-7-7" />
                                                </svg>
                                                <span className="truncate">{author}</span>
                                            </div>
                                            <span className="text-[10px] text-gray-500">{list.length}</span>
                                        </button>

                                        {open && (
                                            <div className="space-y-1">
                                                {list.map((t) => {
                                                    const isSelected = selectedId === t.file_name;
                                                    const colors = listColors.get(t.file_name) ?? DEFAULT_THEME.colors;
                                                    return (
                                                        <button
                                                            key={t.file_name}
                                                            onClick={() => void handleSelect(t.file_name)}
                                                            title={t.error ? `${t.file_name} — ${t.error}` : t.file_name}
                                                            className={`flex w-full items-center gap-2.5 rounded-lg border p-2 text-left transition-colors ${
                                                                isSelected
                                                                    ? 'border-blue-500/60 bg-blue-500/10'
                                                                    : t.error
                                                                      ? 'border-fg-danger/40 hover:border-fg-danger/60 hover:bg-gray-800'
                                                                      : 'border-gray-700 hover:border-gray-600 hover:bg-gray-800'
                                                            }`}
                                                        >
                                                            <SwatchStrip colors={colors} className="h-8 w-8 shrink-0" />
                                                            <span className="min-w-0 flex-1 truncate text-[13px] font-medium text-white">
                                                                {t.name}
                                                            </span>
                                                            {/* A file that will not parse keeps a marker of its own: with
                                                                the description line gone there is nowhere else for the
                                                                reason to live, and a theme that silently stops applying
                                                                is the one case worth interrupting the list for. */}
                                                            {t.error && (
                                                                <AppIcon name="warning" className="h-4 w-4 shrink-0 text-fg-danger" />
                                                            )}
                                                            {isSelected && dirty ? (
                                                                <span className="h-2 w-2 shrink-0 rounded-full bg-amber-400" title="Unsaved changes" />
                                                            ) : isSelected ? (
                                                                <AppIcon name="apply" className="h-4 w-4 shrink-0 text-fg-accent" />
                                                            ) : null}
                                                        </button>
                                                    );
                                                })}
                                            </div>
                                        )}
                                    </div>
                                );
                            })}
                        </div>

                        {/* Sidebar Footer */}
                        <div className="shrink-0 space-y-2 border-t border-gray-700 p-3">
                            <Button variant="secondary" fullWidth onClick={() => void handleDuplicate()} disabled={busy}>
                                {builtin || !selectedId ? 'Duplicate & edit' : 'New from this'}
                            </Button>
                            <button
                                onClick={() => void window.ipcRenderer.openThemesFolder()}
                                className="flex w-full items-center justify-center gap-2 rounded-lg px-3 py-1.5 text-[12px] text-gray-400 transition-colors hover:bg-gray-800 hover:text-white"
                            >
                                <AppIcon name="folder" className="h-3.5 w-3.5" />
                                Open themes folder
                            </button>
                        </div>
                    </div>

                    {/* Main Workspace */}
                    <div className="flex min-h-0 min-w-0 flex-1 flex-col">
                        {!draft ? (
                            <div className="flex flex-1 flex-col items-center justify-center p-8 text-center">
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
                        ) : view === 'colours' ? (
                            <div className="min-h-0 flex-1 overflow-y-auto p-7 space-y-8 bg-gray-900 scrollbar-thin">
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

                                {/* ── Game Covers & Media Chrome Section ── */}
                                <div className="space-y-3">
                                    <h3 className="text-xs font-semibold text-gray-400 uppercase tracking-widest px-1">
                                        Game Covers & Media
                                    </h3>

                                    <div className="bg-gray-800 border border-gray-700 rounded-2xl p-5">
                                        <div className="flex flex-col sm:flex-row items-center sm:items-start gap-6">
                                            {/* Game Card rendered exactly with app tokens */}
                                            {activeGame && (
                                                <div className="w-[220px] shrink-0">
                                                    <GameCard
                                                        community={activeGame}
                                                        isSelected={false}
                                                        isFavorite={isFavoriteSample}
                                                        imageUrl={activeGameImage}
                                                        platform={activeGamePlatform}
                                                        eager
                                                        searchQuery="preview"
                                                        onSelect={() => undefined}
                                                        onToggleFavorite={(_identifier, event) => {
                                                            event.stopPropagation();
                                                            setIsFavoriteSample((favorite) => !favorite);
                                                        }}
                                                    />
                                                </div>
                                            )}

                                            {/* Media color controls */}
                                            <div className="flex-1 min-w-0 space-y-3 w-full">
                                                <div>
                                                    <p className="text-[14px] font-medium text-white">Cover Badges & Chrome</p>
                                                    <p className="text-[13px] text-gray-400 mt-0.5 leading-snug">
                                                        Floating badges and overlays default to a dark scrim to keep text legible on any game artwork.
                                                    </p>
                                                </div>

                                                <div className="bg-gray-900/60 border border-gray-700 rounded-xl divide-y divide-gray-700 overflow-hidden">
                                                    {COVER_COLOR_KEYS.map((key) => (
                                                        <ColorRow
                                                            key={key}
                                                            colorKey={key}
                                                            value={draft.colors[key] ?? (key === 'media_scrim' ? DEFAULT_SCRIM : DEFAULT_MEDIA_INK)}
                                                            opacity={draft.opacity?.[key] ?? 1}
                                                            presets={swatchPresets}
                                                            disabled={!editable}
                                                            onChange={updateColor}
                                                            onOpacityChange={updateOpacity}
                                                            onInteractionStart={beginVisualGesture}
                                                            onInteractionEnd={endVisualGesture}
                                                            portalTarget={previewRoot}
                                                        />
                                                    ))}
                                                </div>

                                                {/* Below the colours it belongs to: this picks what the
                                                    specimen beside them is drawn from, so it reads as
                                                    part of the same group rather than as a section
                                                    heading control. */}
                                                {communities.length > 1 && (
                                                    <div className="flex items-center justify-between gap-4 rounded-xl border border-gray-700 bg-gray-900/60 px-4 py-3">
                                                        <p className="min-w-0 text-[15px] font-medium text-white">Preview game</p>
                                                        <select
                                                            value={activeGame?.identifier ?? ''}
                                                            onChange={(e) => setSelectedGameId(e.target.value)}
                                                            aria-label="Preview game"
                                                            className="h-9 max-w-[220px] shrink-0 rounded-lg border border-gray-600 bg-gray-800 px-3 text-[13px] text-white transition-colors hover:border-gray-500 focus:border-blue-500 focus:outline-none"
                                                        >
                                                            {communities.map((c) => (
                                                                <option key={c.identifier} value={c.identifier}>
                                                                    {c.name}
                                                                </option>
                                                            ))}
                                                        </select>
                                                    </div>
                                                )}
                                            </div>
                                        </div>
                                    </div>
                                </div>

                                {/* ── Surfaces Section ── */}
                                <div className="space-y-3">
                                    <h3 className="text-xs font-semibold text-gray-400 uppercase tracking-widest px-1">
                                        Surfaces
                                    </h3>
                                    <div className="bg-gray-800 border border-gray-700 rounded-2xl divide-y divide-gray-700 overflow-hidden">
                                        {THEME_COLOR_GROUPS.find((g) => g.id === 'surfaces')?.keys.map((key) => (
                                            <ColorRow
                                                key={key}
                                                colorKey={key}
                                                value={draft.colors[key] ?? DEFAULT_THEME.colors[key] ?? '#1f2937'}
                                                opacity={draft.opacity?.[key] ?? 1}
                                                presets={swatchPresets}
                                                disabled={!editable}
                                                onChange={updateColor}
                                                onOpacityChange={updateOpacity}
                                                onInteractionStart={beginVisualGesture}
                                                onInteractionEnd={endVisualGesture}
                                                portalTarget={previewRoot}
                                            />
                                        ))}
                                        <div className="p-4">
                                            <div className="mb-2 flex items-baseline justify-between gap-4">
                                                <div>
                                                    <p className="text-[15px] font-medium text-white">Interface blur</p>
                                                    <p className="mt-0.5 text-[13px] leading-snug text-gray-400">
                                                        Softens content behind translucent surfaces.
                                                    </p>
                                                </div>
                                                <span className="shrink-0 font-mono text-[12px] text-gray-300">
                                                    {Math.round(interfaceBlur)}px
                                                </span>
                                            </div>
                                            <Slider
                                                ariaLabel="Interface blur"
                                                value={interfaceBlur}
                                                min={0}
                                                max={40}
                                                step={1}
                                                disabled={!editable}
                                                onPreviewStart={beginVisualGesture}
                                                onPreviewEnd={endVisualGesture}
                                                onChange={(value) => applyVisualEdit((theme) => ({
                                                    ...theme,
                                                    options: {
                                                        ...theme.options,
                                                        autoContrast:
                                                            theme.options?.autoContrast
                                                            ?? DEFAULT_THEME_OPTIONS.autoContrast,
                                                        interfaceBlur: value,
                                                    },
                                                }), false)}
                                            />
                                        </div>
                                    </div>
                                </div>

                                {/* ── Text & Readability Section ── */}
                                <div className="space-y-3">
                                    <h3 className="text-xs font-semibold text-gray-400 uppercase tracking-widest px-1">
                                        Text & Readability
                                    </h3>
                                    <div className="bg-gray-800 border border-gray-700 rounded-2xl divide-y divide-gray-700 overflow-hidden">
                                        {THEME_COLOR_GROUPS.find((g) => g.id === 'text')?.keys.map((key) => (
                                            <ColorRow
                                                key={key}
                                                colorKey={key}
                                                value={draft.colors[key] ?? DEFAULT_THEME.colors[key] ?? '#ffffff'}
                                                opacity={draft.opacity?.[key] ?? 1}
                                                presets={swatchPresets}
                                                disabled={!editable}
                                                onChange={updateColor}
                                                onOpacityChange={updateOpacity}
                                                onInteractionStart={beginVisualGesture}
                                                onInteractionEnd={endVisualGesture}
                                                portalTarget={previewRoot}
                                            />
                                        ))}

                                        {/* Auto-contrast Toggle */}
                                        <div className={`p-4 flex items-center justify-between gap-4 transition-colors hover:bg-surface-hover ${!editable ? 'opacity-60' : ''}`}>
                                            <div>
                                                <p className="text-[15px] font-medium text-white">Readable labels</p>
                                                <p className="text-[13px] text-gray-400 mt-0.5 leading-snug">
                                                    Automatically chooses white or dark text on filled buttons to ensure readability.
                                                </p>
                                            </div>
                                            <Toggle
                                                value={autoContrast}
                                                disabled={!editable}
                                                label="Readable labels"
                                                onChange={(next) => {
                                                    applyVisualEdit((theme) => ({
                                                        ...theme,
                                                        options: { ...theme.options, autoContrast: next },
                                                    }));
                                                }}
                                            />
                                        </div>

                                        {!autoContrast && MANUAL_COLOR_KEYS.map((key) => (
                                            <ColorRow
                                                key={key}
                                                colorKey={key}
                                                value={draft.colors[key] ?? draft.colors.text}
                                                opacity={draft.opacity?.[key] ?? 1}
                                                presets={swatchPresets}
                                                disabled={!editable}
                                                onChange={updateColor}
                                                onOpacityChange={updateOpacity}
                                                onInteractionStart={beginVisualGesture}
                                                onInteractionEnd={endVisualGesture}
                                                portalTarget={previewRoot}
                                            />
                                        ))}
                                    </div>
                                </div>

                                {/* ── Accent Section ── */}
                                <div className="space-y-3">
                                    <h3 className="text-xs font-semibold text-gray-400 uppercase tracking-widest px-1">
                                        Accent
                                    </h3>
                                    <div className="bg-gray-800 border border-gray-700 rounded-2xl divide-y divide-gray-700 overflow-hidden">
                                        {THEME_COLOR_GROUPS.find((g) => g.id === 'accent')?.keys.map((key) => (
                                            <ColorRow
                                                key={key}
                                                colorKey={key}
                                                value={draft.colors[key] ?? DEFAULT_THEME.colors[key] ?? '#2563eb'}
                                                opacity={draft.opacity?.[key] ?? 1}
                                                presets={swatchPresets}
                                                disabled={!editable}
                                                onChange={updateColor}
                                                onOpacityChange={updateOpacity}
                                                onInteractionStart={beginVisualGesture}
                                                onInteractionEnd={endVisualGesture}
                                                portalTarget={previewRoot}
                                            />
                                        ))}
                                    </div>
                                </div>

                                {/* ── Status Section ── */}
                                <div className="space-y-3">
                                    <h3 className="text-xs font-semibold text-gray-400 uppercase tracking-widest px-1">
                                        Status
                                    </h3>
                                    <div className="bg-gray-800 border border-gray-700 rounded-2xl divide-y divide-gray-700 overflow-hidden">
                                        {THEME_COLOR_GROUPS.find((g) => g.id === 'status')?.keys.map((key) => (
                                            <ColorRow
                                                key={key}
                                                colorKey={key}
                                                value={draft.colors[key] ?? DEFAULT_THEME.colors[key] ?? '#22c55e'}
                                                opacity={draft.opacity?.[key] ?? 1}
                                                presets={swatchPresets}
                                                disabled={!editable}
                                                onChange={updateColor}
                                                onOpacityChange={updateOpacity}
                                                onInteractionStart={beginVisualGesture}
                                                onInteractionEnd={endVisualGesture}
                                                portalTarget={previewRoot}
                                            />
                                        ))}
                                    </div>
                                </div>

                                {/* ── Background Image Section ── */}
                                <div className="space-y-3">
                                    <h3 className="text-xs font-semibold text-gray-400 uppercase tracking-widest px-1">
                                        Background Image
                                    </h3>
                                    <div className="bg-gray-800 border border-gray-700 rounded-2xl overflow-hidden p-4">
                                        {draft.backgroundImage ? (
                                            <div className="space-y-4">
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
                                                            Copied into the themes folder for easy sharing.
                                                        </p>
                                                    </div>
                                                    <button
                                                        type="button"
                                                        disabled={!imageUrl}
                                                        onClick={() => setBackgroundPreviewPinned(true)}
                                                        className="shrink-0 rounded-lg border border-gray-600 px-3 py-1.5 text-[12px] text-gray-300 transition-colors hover:border-gray-500 hover:bg-gray-700 disabled:opacity-50"
                                                    >
                                                        Preview
                                                    </button>
                                                    <button
                                                        disabled={!editable}
                                                        onClick={() => applyVisualEdit((theme) => ({ ...theme, backgroundImage: null }))}
                                                        className="shrink-0 rounded-lg border border-gray-600 px-3 py-1.5 text-[12px] text-gray-300 transition-colors hover:border-gray-500 hover:bg-gray-700 disabled:opacity-50"
                                                    >
                                                        Remove
                                                    </button>
                                                </div>

                                                <div className={backgroundPreviewHeld && backgroundPreviewControl === 'sizing'
                                                    ? "relative z-[60] isolate before:pointer-events-none before:absolute before:-inset-3 before:-z-10 before:rounded-2xl before:border before:border-gray-600 before:bg-gray-800/95 before:shadow-2xl before:backdrop-blur-xl"
                                                    : 'relative'}>
                                                    <div className="mb-2">
                                                        <span className="text-[12px] font-medium text-gray-300">Sizing</span>
                                                    </div>
                                                    <SegmentedControl
                                                        ariaLabel="Background sizing"
                                                        disabled={!editable}
                                                        value={draft.backgroundImage.fit || 'cover'}
                                                        options={[
                                                            { id: 'cover', label: 'Cover' },
                                                            { id: 'contain', label: 'Fit' },
                                                            { id: 'fill', label: 'Stretch' },
                                                            { id: 'tile', label: 'Pattern' },
                                                            { id: 'center', label: 'Original' },
                                                        ] as const}
                                                        onChange={(fit) => updateImage({ fit })}
                                                        onScrubStart={(fit) => beginSizingPreview(fit)}
                                                        onScrubEnd={endBackgroundPreview}
                                                    />
                                                </div>

                                                <div className="grid grid-cols-2 gap-4">
                                                    <div className={backgroundPreviewHeld && backgroundPreviewControl === 'opacity'
                                                        ? "relative z-[60] isolate before:pointer-events-none before:absolute before:-inset-3 before:-z-10 before:rounded-2xl before:border before:border-gray-600 before:bg-gray-800/95 before:shadow-2xl before:backdrop-blur-xl"
                                                        : 'relative'}>
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
                                                            onPreviewStart={() => {
                                                                beginVisualGesture();
                                                                setBackgroundPreviewControl('opacity');
                                                                setBackgroundPreviewHeld(true);
                                                            }}
                                                            onPreviewEnd={() => {
                                                                endVisualGesture();
                                                                setBackgroundPreviewHeld(false);
                                                                setBackgroundPreviewControl(null);
                                                            }}
                                                            onChange={(n) => updateImage({ opacity: n }, false)}
                                                        />
                                                    </div>
                                                    <div className={backgroundPreviewHeld && backgroundPreviewControl === 'blur'
                                                        ? "relative z-[60] isolate before:pointer-events-none before:absolute before:-inset-3 before:-z-10 before:rounded-2xl before:border before:border-gray-600 before:bg-gray-800/95 before:shadow-2xl before:backdrop-blur-xl"
                                                        : 'relative'}>
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
                                                            onPreviewStart={() => {
                                                                beginVisualGesture();
                                                                setBackgroundPreviewControl('blur');
                                                                setBackgroundPreviewHeld(true);
                                                            }}
                                                            onPreviewEnd={() => {
                                                                endVisualGesture();
                                                                setBackgroundPreviewHeld(false);
                                                                setBackgroundPreviewControl(null);
                                                            }}
                                                            onChange={(n) => updateImage({ blur: n }, false)}
                                                        />
                                                    </div>
                                                </div>

                                                <div className="grid grid-cols-2 gap-4">
                                                    <div className={backgroundPreviewHeld && backgroundPreviewControl === 'offset-x'
                                                        ? "relative z-[60] isolate before:pointer-events-none before:absolute before:-inset-3 before:-z-10 before:rounded-2xl before:border before:border-gray-600 before:bg-gray-800/95 before:shadow-2xl before:backdrop-blur-xl"
                                                        : 'relative'}>
                                                        <div className="mb-1.5 flex items-baseline justify-between">
                                                            <span className="text-[12px] text-gray-300">Horizontal position</span>
                                                            <span className="font-mono text-[11px] text-gray-400">
                                                                {Math.round(draft.backgroundImage.offset_x ?? 50)}%
                                                            </span>
                                                        </div>
                                                        <Slider
                                                            ariaLabel="Background horizontal position"
                                                            value={draft.backgroundImage.offset_x ?? 50}
                                                            min={0} max={100} step={1}
                                                            disabled={!editable}
                                                            onPreviewStart={() => {
                                                                beginVisualGesture();
                                                                setBackgroundPreviewControl('offset-x');
                                                                setBackgroundPreviewHeld(true);
                                                            }}
                                                            onPreviewEnd={() => {
                                                                endVisualGesture();
                                                                setBackgroundPreviewHeld(false);
                                                                setBackgroundPreviewControl(null);
                                                            }}
                                                            onChange={(n) => updateImage({ offset_x: n }, false)}
                                                        />
                                                    </div>
                                                    <div className={backgroundPreviewHeld && backgroundPreviewControl === 'offset-y'
                                                        ? "relative z-[60] isolate before:pointer-events-none before:absolute before:-inset-3 before:-z-10 before:rounded-2xl before:border before:border-gray-600 before:bg-gray-800/95 before:shadow-2xl before:backdrop-blur-xl"
                                                        : 'relative'}>
                                                        <div className="mb-1.5 flex items-baseline justify-between">
                                                            <span className="text-[12px] text-gray-300">Vertical position</span>
                                                            <span className="font-mono text-[11px] text-gray-400">
                                                                {Math.round(draft.backgroundImage.offset_y ?? 50)}%
                                                            </span>
                                                        </div>
                                                        <Slider
                                                            ariaLabel="Background vertical position"
                                                            value={draft.backgroundImage.offset_y ?? 50}
                                                            min={0} max={100} step={1}
                                                            disabled={!editable}
                                                            onPreviewStart={() => {
                                                                beginVisualGesture();
                                                                setBackgroundPreviewControl('offset-y');
                                                                setBackgroundPreviewHeld(true);
                                                            }}
                                                            onPreviewEnd={() => {
                                                                endVisualGesture();
                                                                setBackgroundPreviewHeld(false);
                                                                setBackgroundPreviewControl(null);
                                                            }}
                                                            onChange={(n) => updateImage({ offset_y: n }, false)}
                                                        />
                                                    </div>
                                                </div>

                                                {draft.backgroundImage.fit === 'tile' && (
                                                    <div className={backgroundPreviewHeld && backgroundPreviewControl === 'tile-scale'
                                                        ? "relative z-[60] isolate before:pointer-events-none before:absolute before:-inset-3 before:-z-10 before:rounded-2xl before:border before:border-gray-600 before:bg-gray-800/95 before:shadow-2xl before:backdrop-blur-xl"
                                                        : 'relative'}>
                                                        <div className="mb-1.5 flex items-baseline justify-between">
                                                            <span className="text-[12px] text-gray-300">Pattern size</span>
                                                            <span className="font-mono text-[11px] text-gray-400">
                                                                {Math.round(draft.backgroundImage.tile_scale ?? 25)}%
                                                            </span>
                                                        </div>
                                                        <Slider
                                                            ariaLabel="Background pattern size"
                                                            value={draft.backgroundImage.tile_scale ?? 25}
                                                            min={2} max={100} step={1}
                                                            disabled={!editable}
                                                            onPreviewStart={() => {
                                                                beginVisualGesture();
                                                                setBackgroundPreviewControl('tile-scale');
                                                                setBackgroundPreviewHeld(true);
                                                            }}
                                                            onPreviewEnd={() => {
                                                                endVisualGesture();
                                                                setBackgroundPreviewHeld(false);
                                                                setBackgroundPreviewControl(null);
                                                            }}
                                                            onChange={(n) => updateImage({ tile_scale: n }, false)}
                                                        />
                                                    </div>
                                                )}
                                            </div>
                                        ) : (
                                            <div className="flex items-center justify-between gap-4 p-2">
                                                <div>
                                                    <p className="text-[15px] font-medium text-white">Background picture</p>
                                                    <p className="text-[13px] text-gray-400 mt-0.5 leading-snug">
                                                        Sits behind the app. Panels stay solid so text keeps its contrast.
                                                    </p>
                                                </div>
                                                <button
                                                    disabled={!editable || busy}
                                                    onClick={() => void handlePickImage()}
                                                    className="rounded-lg border border-gray-600 px-4 py-2 text-[13px] font-medium text-gray-200 transition-colors hover:border-gray-500 hover:bg-gray-700 disabled:opacity-50"
                                                >
                                                    Choose image
                                                </button>
                                            </div>
                                        )}
                                    </div>
                                </div>
                            </div>
                        ) : (
                            <ThemeTomlEditor
                                fileName={editable ? selectedId : null}
                                readOnlySource={themeToToml(draft)}
                                draftSource={editable && dirty ? draftToml(draft) : undefined}
                                editable={editable}
                                onSaved={async () => {
                                    setDirty(false);
                                    setPreview(null);
                                    await loadThemes();
                                }}
                            />
                        )}
                    </div>
                </div>

                {/* Footer */}
                <div className="flex shrink-0 items-center justify-between gap-3 border-t border-gray-700 bg-gray-900 px-6 py-3.5">
                    <div>
                        {editable && (
                            <Button
                                variant="dangerSecondary"
                                onClick={() => void handleDelete()}
                                className="rounded-xl text-[13px]"
                            >
                                Delete theme
                            </Button>
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
