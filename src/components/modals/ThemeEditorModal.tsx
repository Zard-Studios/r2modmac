import { memo, useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { Button } from '../ui';
import { ColorField } from '../ui/ColorPicker';
import { Toggle } from '../ui/Toggle';
import { LazyImage } from '../LazyImage';
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

const getInitials = (name: string) => {
    return name.split(' ').map((w) => w[0]).join('').slice(0, 2).toUpperCase();
};

const colorWithOpacity = (color: string, opacity = 1) => {
    const hex = normalizeHex(color).replace('#', '');
    const alpha = Math.round(Math.max(0, Math.min(1, opacity)) * 255)
        .toString(16)
        .padStart(2, '0');
    return `#${hex}${alpha}`;
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
        <div className={`p-4 flex items-center justify-between gap-4 transition-colors hover:bg-gray-750 ${disabled ? 'opacity-60' : ''}`}>
            <div className="flex items-center gap-3.5 min-w-0">
                <div className="shrink-0">
                    {disabled ? (
                        <span
                            className="block h-9 w-9 rounded-lg border border-gray-600"
                            style={{ backgroundColor: value }}
                        />
                    ) : (
                        <ColorField
                            label={meta.label}
                            value={value}
                            presets={presets}
                            onChange={(next) => onChange(colorKey, next)}
                            onInteractionStart={onInteractionStart}
                            onInteractionEnd={onInteractionEnd}
                        />
                    )}
                </div>
                <div className="min-w-0">
                    <p className="text-[15px] font-medium text-white">{meta.label}</p>
                    <p className="text-[13px] text-gray-400 mt-0.5 leading-snug truncate">{meta.description}</p>
                </div>
            </div>

            <div className="flex shrink-0 items-center gap-3">
                <label className="w-28" aria-label={`${meta.label} opacity`}>
                    <span className="mb-1 flex items-center justify-between text-[10px] font-medium uppercase tracking-wider text-gray-400">
                        <span>Opacity</span>
                        <span className="font-mono normal-case tracking-normal">{Math.round(opacity * 100)}%</span>
                    </span>
                    <input
                        type="range"
                        min={0}
                        max={1}
                        step={0.01}
                        value={opacity}
                        disabled={disabled}
                        onPointerDown={onInteractionStart}
                        onPointerUp={onInteractionEnd}
                        onPointerCancel={onInteractionEnd}
                        onChange={(event) => onOpacityChange(colorKey, Number(event.target.value))}
                        className="h-1.5 w-full cursor-pointer appearance-none rounded-full border border-gray-700 disabled:cursor-not-allowed [&::-webkit-slider-thumb]:h-3.5 [&::-webkit-slider-thumb]:w-3.5 [&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:border [&::-webkit-slider-thumb]:border-gray-400 [&::-webkit-slider-thumb]:bg-white"
                        style={{ background: `linear-gradient(to right, ${value} 0%, ${value} ${opacity * 100}%, transparent ${opacity * 100}%, transparent 100%)` }}
                    />
                </label>
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

function Slider({
    value, min, max, step, onChange, ariaLabel, disabled = false,
    onPreviewStart, onPreviewEnd,
}: {
    value: number; min: number; max: number; step: number;
    onChange: (n: number) => void; ariaLabel: string; disabled?: boolean;
    onPreviewStart?: () => void; onPreviewEnd?: () => void;
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
            onPointerDown={(event) => {
                event.currentTarget.setPointerCapture(event.pointerId);
                onPreviewStart?.();
            }}
            onPointerUp={onPreviewEnd}
            onPointerCancel={onPreviewEnd}
            onKeyDown={(event) => {
                if (['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown', 'PageUp', 'PageDown', 'Home', 'End'].includes(event.key)) {
                    onPreviewStart?.();
                }
            }}
            onKeyUp={onPreviewEnd}
            style={{
                background: `linear-gradient(to right, rgb(var(--r2-blue-600) / var(--r2-blue-600-alpha, 1)) ${pct}%, rgb(var(--r2-gray-700) / var(--r2-gray-700-alpha, 1)) ${pct}%)`,
            }}
            className="h-2 w-full cursor-pointer appearance-none rounded-full border border-gray-600/70 disabled:cursor-not-allowed disabled:opacity-50 [&::-moz-range-thumb]:h-4 [&::-moz-range-thumb]:w-4 [&::-moz-range-thumb]:appearance-none [&::-moz-range-thumb]:rounded-full [&::-moz-range-thumb]:border [&::-moz-range-thumb]:border-gray-400 [&::-moz-range-thumb]:bg-white [&::-moz-range-thumb]:shadow-sm [&::-webkit-slider-thumb]:h-4 [&::-webkit-slider-thumb]:w-4 [&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:border [&::-webkit-slider-thumb]:border-gray-400 [&::-webkit-slider-thumb]:bg-white [&::-webkit-slider-thumb]:shadow-sm"
        />
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

    const size = image.fit === 'contain'
        ? 'contain'
        : image.fit === 'fill'
          ? '100% 100%'
          : image.fit === 'tile'
            ? `${image.tile_scale ?? 25}% auto`
            : image.fit === 'center'
              ? 'auto'
              : 'cover';

    return (
        <div
            className={`theme-background-canvas absolute inset-0 z-50 overflow-hidden ${visible ? 'opacity-100' : 'opacity-0'} ${pinned && visible ? 'pointer-events-auto' : 'pointer-events-none'}`}
            style={{ backgroundColor: theme.colors.background }}
            onClick={pinned ? onClose : undefined}
            role={pinned ? 'button' : undefined}
            tabIndex={pinned ? 0 : undefined}
            aria-label={pinned ? 'Return to the theme editor' : undefined}
        >
            <div
                className="absolute inset-0 bg-center"
                style={{
                    backgroundImage: imageUrl ? `url("${imageUrl}")` : undefined,
                    backgroundSize: size,
                    backgroundRepeat: image.fit === 'tile' ? 'repeat' : 'no-repeat',
                    backgroundPosition: `${image.offset_x ?? 50}% ${image.offset_y ?? 50}%`,
                    filter: `blur(${image.blur}px)`,
                    transform: 'scale(1.06) translateZ(0)',
                }}
            />
            <div
                className="absolute inset-0"
                style={{
                    backgroundColor: theme.colors.background,
                    opacity: 1 - image.opacity,
                }}
            />
            {pinned && (
                <div className="absolute bottom-6 left-1/2 -translate-x-1/2 rounded-full border border-gray-700 bg-gray-900/90 px-4 py-2 text-xs text-gray-300 shadow-xl backdrop-blur-md">
                    Press <kbd className="rounded bg-gray-800 px-1.5 py-0.5 text-[11px] font-mono text-white">Esc</kbd> or click to return
                </div>
            )}
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
    const [selectedGameId, setSelectedGameId] = useState<string | null>(null);
    const [isFavoriteSample, setIsFavoriteSample] = useState(false);
    const [backgroundPreviewHeld, setBackgroundPreviewHeld] = useState(false);
    const [backgroundPreviewPinned, setBackgroundPreviewPinned] = useState(false);
    const [backgroundPreviewControl, setBackgroundPreviewControl] = useState<
        'tile-scale' | 'opacity' | 'blur' | 'offset-x' | 'offset-y' | null
    >(null);
    const sizingPreviewTimerRef = useRef<number | null>(null);

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
        if (!draft) return;
        const next = change(draft);
        if (next === draft) return;
        if (record && !visualGestureBaseRef.current) {
            setVisualUndo((history) => [...history, draft].slice(-100));
            setVisualRedo([]);
        }
        setDraft(next);
        setDirty(JSON.stringify(next) !== JSON.stringify(visualBaseline));
    }, [draft, visualBaseline]);

    const beginVisualGesture = useCallback(() => {
        if (draft && !visualGestureBaseRef.current) visualGestureBaseRef.current = draft;
    }, [draft]);

    const endVisualGesture = useCallback(() => {
        const base = visualGestureBaseRef.current;
        visualGestureBaseRef.current = null;
        if (!base || !draft || draft === base) return;
        setVisualUndo((history) => [...history, base].slice(-100));
        setVisualRedo([]);
    }, [draft]);

    const undoVisual = useCallback(() => {
        const previous = visualUndo[visualUndo.length - 1];
        if (!previous || !draft) return;
        setVisualUndo((history) => history.slice(0, -1));
        setVisualRedo((history) => [...history, draft].slice(-100));
        setDraft(previous);
        setDirty(JSON.stringify(previous) !== JSON.stringify(visualBaseline));
    }, [draft, visualBaseline, visualUndo]);

    const redoVisual = useCallback(() => {
        const next = visualRedo[visualRedo.length - 1];
        if (!next || !draft) return;
        setVisualRedo((history) => history.slice(0, -1));
        setVisualUndo((history) => [...history, draft].slice(-100));
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
        applyVisualEdit((theme) => {
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
                ...theme,
                colors: {
                    ...theme.colors,
                    ...colors,
                    media_scrim: light ? '#111827' : colors.background,
                    media_ink: '#ffffff',
                },
            };
        });
    }, [applyVisualEdit]);

    const updateImage = useCallback((patch: Partial<NonNullable<Theme['backgroundImage']>>, record = true) => {
        applyVisualEdit((theme) => {
            if (!theme.backgroundImage) return theme;
            return { ...theme, backgroundImage: { ...theme.backgroundImage, ...patch } };
        }, record);
    }, [applyVisualEdit]);

    const previewSizing = useCallback((fit: NonNullable<Theme['backgroundImage']>['fit']) => {
        updateImage({ fit });
        setBackgroundPreviewControl(null);
        setBackgroundPreviewHeld(true);
        if (sizingPreviewTimerRef.current !== null) {
            window.clearTimeout(sizingPreviewTimerRef.current);
        }
        sizingPreviewTimerRef.current = window.setTimeout(() => {
            sizingPreviewTimerRef.current = null;
            setBackgroundPreviewHeld(false);
        }, 2000);
    }, [updateImage]);

    useEffect(() => () => {
        if (sizingPreviewTimerRef.current !== null) {
            window.clearTimeout(sizingPreviewTimerRef.current);
        }
    }, []);

    useEffect(() => {
        if (!backgroundPreviewHeld || !backgroundPreviewControl) return;
        const endDragPreview = () => {
            endVisualGesture();
            setBackgroundPreviewHeld(false);
            setBackgroundPreviewControl(null);
        };
        window.addEventListener('pointerup', endDragPreview, true);
        window.addEventListener('blur', endDragPreview);
        return () => {
            window.removeEventListener('pointerup', endDragPreview, true);
            window.removeEventListener('blur', endDragPreview);
        };
    }, [backgroundPreviewHeld, backgroundPreviewControl, endVisualGesture]);

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
        if (!draft || !editable || !selectedId || !dirty) return;
        setSaving(true);
        try {
            await window.ipcRenderer.writeTheme(selectedId, themeToToml(draft));
            forgetBackgroundImage(draft.backgroundImage?.path);
            await loadThemes();
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
    }, [draft, editable, selectedId, dirty, loadThemes, setPreview]);

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
            if (view === 'colours' && modifier && key === 'z') {
                e.preventDefault();
                if (e.shiftKey) redoVisual();
                else undoVisual();
                return;
            }
            if (view === 'colours' && modifier && key === 'y') {
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

    // Active sample game for cover preview
    const activeGame = useMemo(() => {
        if (communities.length === 0) return null;
        if (selectedGameId) {
            const found = communities.find((c) => c.identifier === selectedGameId);
            if (found) return found;
        }
        const withArt = communities.find((c) => communityImages[c.identifier]);
        return withArt ?? communities[0];
    }, [communities, communityImages, selectedGameId]);

    const activeGameImage = activeGame ? communityImages[activeGame.identifier] : undefined;
    const activeGamePlatform = activeGame ? communityPlatforms[activeGame.identifier] : undefined;
    const isWindowsCompatible = activeGamePlatform?.windows ?? true;
    const isMacCompatible = activeGamePlatform?.mac ?? false;

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
        <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
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
                                        onChange={(e) => applyVisualEdit((theme) => ({ ...theme, name: e.target.value }))}
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
                                    placeholder="Author — groups your themes in the list"
                                    onChange={(e) => applyVisualEdit((theme) => ({ ...theme, author: e.target.value || undefined }))}
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
                                    title="Undo (⌘Z)"
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
                                    title="Redo (⇧⌘Z)"
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
                            <div className="flex bg-gray-800 border border-gray-700 rounded-lg p-1" aria-label="Editor view">
                                {(['colours', 'toml'] as const).map((nextView) => (
                                    <button
                                        key={nextView}
                                        type="button"
                                        onClick={() => setView(nextView)}
                                        aria-pressed={view === nextView}
                                        className={`rounded-md px-3 py-1.5 text-[11px] font-medium transition-colors ${
                                            view === nextView
                                                ? 'bg-blue-600 text-on-accent shadow-sm'
                                                : 'text-gray-400 hover:text-white'
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
                                    {!collapsedGroups.has('built-in') && <div className="space-y-1.5">
                                        {filteredBuiltins.map((b) => {
                                            const isSelected = selectedId === b.id;
                                            return (
                                                <button
                                                    key={b.id}
                                                    onClick={() => void handleSelect(b.id)}
                                                    className={`w-full rounded-xl border p-2.5 text-left transition-colors ${
                                                        isSelected
                                                            ? 'border-blue-500/60 bg-blue-500/10'
                                                            : 'border-gray-700 hover:border-gray-600 hover:bg-gray-800'
                                                    }`}
                                                >
                                                    <div className="flex items-center gap-2">
                                                        <SwatchStrip colors={b.colors} className="h-5 w-12 shrink-0" />
                                                        <span className="min-w-0 flex-1 truncate text-[13px] font-medium text-white">
                                                            {b.name}
                                                        </span>
                                                    </div>
                                                    <p className="mt-1 truncate text-[11px] text-gray-400">{b.origin ?? 'The stock r2modmac look'}</p>
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
                                            <div className="space-y-1.5">
                                                {list.map((t) => {
                                                    const isSelected = selectedId === t.file_name;
                                                    const colors = listColors.get(t.file_name) ?? DEFAULT_THEME.colors;
                                                    return (
                                                        <button
                                                            key={t.file_name}
                                                            onClick={() => void handleSelect(t.file_name)}
                                                            className={`w-full rounded-xl border p-2.5 text-left transition-colors ${
                                                                isSelected
                                                                    ? 'border-blue-500/60 bg-blue-500/10'
                                                                    : 'border-gray-700 hover:border-gray-600 hover:bg-gray-800'
                                                            }`}
                                                        >
                                                            <div className="flex items-center gap-2">
                                                                <SwatchStrip colors={colors} className="h-5 w-12 shrink-0" />
                                                                <span className="min-w-0 flex-1 truncate text-[13px] font-medium text-white">
                                                                    {t.name}
                                                                </span>
                                                                {isSelected && dirty && (
                                                                    <span className="h-2 w-2 rounded-full bg-amber-400" title="Unsaved changes" />
                                                                )}
                                                            </div>
                                                            {t.error ? (
                                                                <p className="mt-1 truncate text-[11px] text-fg-danger">{t.error}</p>
                                                            ) : (
                                                                <p className="mt-1 truncate text-[11px] text-gray-400">{t.file_name}</p>
                                                            )}
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
                                className="w-full rounded-lg px-3 py-1.5 text-[12px] text-gray-400 transition-colors hover:bg-gray-800 hover:text-white"
                            >
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

                                <section className="space-y-3">
                                    <div className="flex items-center justify-between px-1">
                                        <h3 className="text-xs font-semibold uppercase tracking-widest text-gray-400">Live sample</h3>
                                        <span className="text-[11px] text-gray-500">The app changes only after Save</span>
                                    </div>
                                    <div
                                        className="overflow-hidden rounded-2xl border p-4"
                                        style={{
                                            backgroundColor: colorWithOpacity(draft.colors.background, draft.opacity?.background ?? 1),
                                            borderColor: colorWithOpacity(draft.colors.border, draft.opacity?.border ?? 1),
                                        }}
                                    >
                                        <div
                                            className="flex items-center justify-between gap-5 rounded-xl border p-4"
                                            style={{
                                                backgroundColor: colorWithOpacity(draft.colors.surface, draft.opacity?.surface ?? 1),
                                                borderColor: colorWithOpacity(draft.colors.border, draft.opacity?.border ?? 1),
                                            }}
                                        >
                                            <div className="min-w-0">
                                                <p className="truncate text-[15px] font-semibold" style={{ color: colorWithOpacity(draft.colors.text, draft.opacity?.text ?? 1) }}>
                                                    Waterpark Simulator
                                                </p>
                                                <p className="mt-1 text-[12px]" style={{ color: colorWithOpacity(draft.colors.text_muted, draft.opacity?.text_muted ?? 1) }}>
                                                    A compact sample of surfaces, type and status colours.
                                                </p>
                                            </div>
                                            <button
                                                type="button"
                                                className="shrink-0 rounded-lg px-4 py-2 text-[12px] font-semibold"
                                                style={{
                                                    backgroundColor: colorWithOpacity(draft.colors.accent, draft.opacity?.accent ?? 1),
                                                    color: colorWithOpacity(draft.colors.on_accent ?? draft.colors.text, draft.opacity?.on_accent ?? draft.opacity?.text ?? 1),
                                                }}
                                            >
                                                Primary
                                            </button>
                                        </div>
                                        <div className="mt-3 flex flex-wrap gap-2">
                                            {([
                                                ['Danger', 'danger'],
                                                ['Warning', 'warning'],
                                                ['Success', 'success'],
                                            ] as const).map(([label, key]) => (
                                                <span
                                                    key={key}
                                                    className="rounded-full border px-2.5 py-1 text-[11px] font-medium"
                                                    style={{
                                                        borderColor: colorWithOpacity(draft.colors[key], draft.opacity?.[key] ?? 1),
                                                        color: colorWithOpacity(draft.colors[key], draft.opacity?.[key] ?? 1),
                                                    }}
                                                >
                                                    {label}
                                                </span>
                                            ))}
                                        </div>
                                    </div>
                                </section>

                                {/* ── Game Covers & Media Chrome Section ── */}
                                <div className="space-y-3">
                                    <div className="flex items-center justify-between px-1">
                                        <h3 className="text-xs font-semibold text-gray-400 uppercase tracking-widest">
                                            Game Covers & Media
                                        </h3>
                                        {communities.length > 1 && (
                                            <div className="flex items-center gap-2">
                                                <span className="text-[11px] text-gray-500">Preview game:</span>
                                                <select
                                                    value={activeGame?.identifier ?? ''}
                                                    onChange={(e) => setSelectedGameId(e.target.value)}
                                                    className="rounded-lg border border-gray-700 bg-gray-800 px-2.5 py-1 text-xs text-white focus:border-blue-500 focus:outline-none"
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

                                    <div className="bg-gray-800 border border-gray-700 rounded-2xl p-5">
                                        <div className="flex flex-col sm:flex-row items-center sm:items-start gap-6">
                                            {/* Game Card rendered exactly with app tokens */}
                                            {activeGame && (
                                                <div className="shrink-0">
                                                    <div
                                                        className="aspect-[3/4] w-[180px] relative overflow-hidden rounded-xl border bg-gray-950 shadow-xl"
                                                        style={{ borderColor: colorWithOpacity(draft.colors.border, draft.opacity?.border ?? 1) }}
                                                    >
                                                        {activeGameImage ? (
                                                            <LazyImage
                                                                src={activeGameImage}
                                                                alt={activeGame.name}
                                                                eager
                                                                className="absolute inset-0 z-10 h-full w-full object-cover"
                                                            />
                                                        ) : (
                                                            <div className="absolute inset-0 flex items-center justify-center bg-gray-800 text-3xl font-black text-white/30 select-none">
                                                                {getInitials(activeGame.name)}
                                                            </div>
                                                        )}

                                                        {/* Scrim Gradient */}
                                                        <div
                                                            className="absolute inset-x-0 bottom-0 h-2/3 z-20 pointer-events-none transition-opacity"
                                                            style={{
                                                                background: `linear-gradient(to top, ${colorWithOpacity(draft.colors.media_scrim ?? DEFAULT_SCRIM, draft.opacity?.media_scrim ?? 1)} 0%, ${colorWithOpacity(draft.colors.media_scrim ?? DEFAULT_SCRIM, (draft.opacity?.media_scrim ?? 1) * 0.6)} 40%, transparent 100%)`,
                                                            }}
                                                        />

                                                        {/* Platform badge */}
                                                        <div className="absolute top-2 right-2 flex flex-col gap-1 z-30 pointer-events-none items-end">
                                                            <div
                                                                className={`p-1 rounded-full shadow-lg border backdrop-blur-sm flex items-center justify-center gap-2 h-7 ${isMacCompatible ? 'px-2' : 'w-7 px-0'}`}
                                                                style={{
                                                                    backgroundColor: colorWithOpacity(draft.colors.media_scrim ?? DEFAULT_SCRIM, draft.opacity?.media_scrim ?? 1),
                                                                    borderColor: colorWithOpacity(draft.colors.media_ink ?? DEFAULT_MEDIA_INK, (draft.opacity?.media_ink ?? 1) * 0.15),
                                                                    color: colorWithOpacity(draft.colors.media_ink ?? DEFAULT_MEDIA_INK, draft.opacity?.media_ink ?? 1),
                                                                }}
                                                            >
                                                                {isWindowsCompatible && (
                                                                    <span title="Windows Compatible" className="flex items-center justify-center w-3.5 h-3.5 shrink-0">
                                                                        <svg xmlns="http://www.w3.org/2000/svg" className="w-[14px] h-[14px] shrink-0" viewBox="0 0 24 24" fill="currentColor">
                                                                            <path d="M0 3.449L9.75 2.1v9.451H0m10.949-9.602L24 0v11.4H10.949M0 12.6h9.75v9.451L0 20.699M10.949 12.6H24V24l-12.9-1.801" />
                                                                        </svg>
                                                                    </span>
                                                                )}
                                                                {isMacCompatible && (
                                                                    <>
                                                                        {isWindowsCompatible && (
                                                                            <div className="w-[1px] h-3.5 opacity-25" style={{ backgroundColor: colorWithOpacity(draft.colors.media_ink ?? DEFAULT_MEDIA_INK, draft.opacity?.media_ink ?? 1) }} />
                                                                        )}
                                                                        <span title="MacOS Compatible" className="flex items-center justify-center w-3 h-3.5 shrink-0">
                                                                            <svg xmlns="http://www.w3.org/2000/svg" className="w-[12px] h-[14px] shrink-0" viewBox="0 0 384 512" fill="currentColor">
                                                                                <path d="M318.7 268.7c-.2-36.7 16.4-64.4 50-84.8-18.8-26.9-47.2-41.7-84.7-44.6-35.5-2.8-74.3 20.7-88.5 20.7-15 0-49.4-19.7-76.4-19.7C63.3 141.2 4 184.8 4 273.5q0 39.3 14.4 81.2c12.8 36.7 59 126.7 107.2 125.2 25.2-.6 43-17.9 75.8-17.9 31.8 0 48.3 17.9 76.4 17.9 48.6-.7 90.4-82.5 102.6-119.3-65.2-30.7-61.7-90-61.7-91.9zm-56.6-164.2c27.3-32.4 24.8-61.9 24-72.5-24.1 1.4-52 16.4-67.9 34.9-17.5 19.8-27.8 44.3-25.6 71.9 26.1 2 49.9-11.4 69.5-34.3z" />
                                                                            </svg>
                                                                        </span>
                                                                    </>
                                                                )}
                                                            </div>
                                                        </div>

                                                        {/* Favorite star */}
                                                        <button
                                                            type="button"
                                                            onClick={() => setIsFavoriteSample((f) => !f)}
                                                            className="absolute top-2 left-2 p-1.5 rounded-full z-20 shadow-md border backdrop-blur-sm transition-all hover:scale-110 active:scale-95"
                                                            style={{
                                                                backgroundColor: colorWithOpacity(draft.colors.media_scrim ?? DEFAULT_SCRIM, draft.opacity?.media_scrim ?? 1),
                                                                borderColor: colorWithOpacity(draft.colors.media_ink ?? DEFAULT_MEDIA_INK, (draft.opacity?.media_ink ?? 1) * 0.15),
                                                                color: isFavoriteSample ? '#facc15' : colorWithOpacity(draft.colors.media_ink ?? DEFAULT_MEDIA_INK, draft.opacity?.media_ink ?? 1),
                                                            }}
                                                            title="Toggle favorite badge"
                                                        >
                                                            <svg xmlns="http://www.w3.org/2000/svg" className="h-3.5 w-3.5" viewBox="0 0 20 20" fill="currentColor">
                                                                <path d="M9.049 2.927c.3-.921 1.603-.921 1.902 0l1.07 3.292a1 1 0 00.95.69h3.462c.969 0 1.371 1.24.588 1.81l-2.8 2.034a1 1 0 00-.364 1.118l1.07 3.292c.3.921-.755 1.688-1.54 1.118l-2.8-2.034a1 1 0 00-1.175 0l-2.8 2.034c-.784.57-1.838-.197-1.539-1.118l1.07-3.292a1 1 0 00-.364-1.118L2.98 8.72c-.783-.57-.38-1.81.588-1.81h3.461a1 1 0 00.951-.69l1.07-3.292z" />
                                                            </svg>
                                                        </button>

                                                        {/* Title overlay */}
                                                        <div
                                                            className="absolute inset-x-0 bottom-0 p-3 z-30 pointer-events-none"
                                                            style={{ color: colorWithOpacity(draft.colors.media_ink ?? DEFAULT_MEDIA_INK, draft.opacity?.media_ink ?? 1) }}
                                                        >
                                                            <p className="text-sm font-bold leading-tight drop-shadow-md">
                                                                {activeGame.name}
                                                            </p>
                                                        </div>
                                                    </div>
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
                                                        />
                                                    ))}
                                                </div>
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
                                            />
                                        ))}
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
                                            />
                                        ))}

                                        {/* Auto-contrast Toggle */}
                                        <div className={`p-4 flex items-center justify-between gap-4 transition-colors hover:bg-gray-750 ${!editable ? 'opacity-60' : ''}`}>
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
                                                    applyVisualEdit((theme) => ({ ...theme, options: { autoContrast: next } }));
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
                                                                    onClick={() => previewSizing(mode.id)}
                                                                    className={`flex flex-col items-center justify-center rounded-lg px-2 py-1.5 text-center transition-all disabled:opacity-50 ${
                                                                        active
                                                                            ? 'bg-blue-600 font-semibold text-on-accent shadow-sm'
                                                                            : 'text-gray-400 hover:bg-gray-800 hover:text-white'
                                                                    }`}
                                                                >
                                                                    <span className="text-[12px] leading-tight">{mode.label}</span>
                                                                    <span className={`text-[9px] leading-tight ${active ? 'text-on-accent/70' : 'text-gray-500'}`}>
                                                                        {mode.desc}
                                                                    </span>
                                                                </button>
                                                            );
                                                        })}
                                                    </div>
                                                </div>

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
