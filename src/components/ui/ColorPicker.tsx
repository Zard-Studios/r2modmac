import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { sliderKnobOffset } from './sliderGeometry';

import { isValidHex, normalizeHex, parseHex } from '../../utils/theme';

/**
 * A colour picker the app owns.
 *
 * The native `<input type="color">` opens the operating system's picker, which
 * looks and behaves differently on every platform and ignores the app's styling
 * entirely. This is the familiar saturation/value square plus hue slider, drawn
 * with the app's own tokens so it themes along with everything else.
 */

// ── HSV ──────────────────────────────────────────────────────────────────────
// The picker works in HSV because that is the model the square-and-slider
// layout represents: x is saturation, y is value, the slider is hue. Theme
// derivation still happens in OKLab; this is purely the interaction surface.

interface Hsv { h: number; s: number; v: number }

function hexToHsv(hex: string): Hsv {
    const rgb = parseHex(hex);
    if (!rgb) return { h: 0, s: 0, v: 0 };
    const r = rgb.r / 255;
    const g = rgb.g / 255;
    const b = rgb.b / 255;
    const max = Math.max(r, g, b);
    const min = Math.min(r, g, b);
    const d = max - min;

    let h = 0;
    if (d !== 0) {
        if (max === r) h = ((g - b) / d) % 6;
        else if (max === g) h = (b - r) / d + 2;
        else h = (r - g) / d + 4;
        h *= 60;
        if (h < 0) h += 360;
    }
    return { h, s: max === 0 ? 0 : d / max, v: max };
}

function hsvToHex({ h, s, v }: Hsv): string {
    const c = v * s;
    const x = c * (1 - Math.abs(((h / 60) % 2) - 1));
    const m = v - c;
    const i = Math.floor(h / 60) % 6;
    const [r, g, b] = [
        [c, x, 0], [x, c, 0], [0, c, x], [0, x, c], [x, 0, c], [c, 0, x],
    ][i];
    const to = (n: number) =>
        Math.round((n + m) * 255).toString(16).padStart(2, '0');
    return `#${to(r)}${to(g)}${to(b)}`;
}

const clamp01 = (n: number) => (n < 0 ? 0 : n > 1 ? 1 : n);

// ── Drag surface ─────────────────────────────────────────────────────────────

/**
 * Track a pointer across an element as a 0–1 pair, capturing it so the drag
 * survives leaving the element — releasing the mouse outside the square should
 * still end on the colour you dragged to.
 *
 * The element comes from the event rather than a ref: `currentTarget` is
 * exactly the surface being dragged, so there is nothing to store.
 */
function useDragRatio(
    onMove: (x: number, y: number) => void,
    onInteractionStart?: () => void,
    onInteractionEnd?: () => void
) {
    const emit = useCallback(
        (el: HTMLElement, clientX: number, clientY: number) => {
            const r = el.getBoundingClientRect();
            onMove(clamp01((clientX - r.left) / r.width), clamp01((clientY - r.top) / r.height));
        },
        [onMove]
    );

    const onPointerDown = useCallback(
        (e: React.PointerEvent<HTMLDivElement>) => {
            e.preventDefault();
            onInteractionStart?.();
            e.currentTarget.setPointerCapture(e.pointerId);
            emit(e.currentTarget, e.clientX, e.clientY);
        },
        [emit, onInteractionStart]
    );

    const onPointerMove = useCallback(
        (e: React.PointerEvent<HTMLDivElement>) => {
            if (e.currentTarget.hasPointerCapture(e.pointerId)) {
                emit(e.currentTarget, e.clientX, e.clientY);
            }
        },
        [emit]
    );

    const onPointerEnd = useCallback(() => onInteractionEnd?.(), [onInteractionEnd]);

    return {
        onPointerDown,
        onPointerMove,
        onPointerUp: onPointerEnd,
        onPointerCancel: onPointerEnd,
        onLostPointerCapture: onPointerEnd,
    };
}

// ── Picker body ──────────────────────────────────────────────────────────────

interface ColorPickerProps {
    value: string;
    onChange: (hex: string) => void;
    /** Transparency belongs to the colour, so it is edited in this popover too. */
    opacity?: number;
    onOpacityChange?: (opacity: number) => void;
    /** Colours offered as one-click choices — normally the rest of the theme. */
    presets?: string[];
    /** Groups a continuous picker drag into one undoable edit. */
    onInteractionStart?: () => void;
    onInteractionEnd?: () => void;
}

function PickerBody({
    value,
    onChange,
    opacity = 1,
    onOpacityChange,
    presets = [],
    onInteractionStart,
    onInteractionEnd,
}: ColorPickerProps) {
    // Hue is held separately: at zero saturation or value the hex no longer
    // encodes it, and without this the slider would snap back to red whenever
    // the user dragged into a corner of the square.
    const [hue, setHue] = useState(() => hexToHsv(value).h);
    const [hexDraft, setHexDraft] = useState(value);
    const [lastValue, setLastValue] = useState(value);

    if (value !== lastValue) {
        setLastValue(value);
        setHexDraft(value);
        const { h, s, v } = hexToHsv(value);
        if (s > 0.01 && v > 0.01) setHue(h);
    }

    const { s, v } = hexToHsv(value);

    const sv = useDragRatio(
        (x, y) => onChange(hsvToHex({ h: hue, s: x, v: 1 - y })),
        onInteractionStart,
        onInteractionEnd
    );
    const hueBar = useDragRatio((x) => {
        const h = x * 360;
        setHue(h);
        onChange(hsvToHex({ h, s: s || 1, v: v || 1 }));
    }, onInteractionStart, onInteractionEnd);
    const opacityBar = useDragRatio(
        (x) => onOpacityChange?.(x),
        onInteractionStart,
        onInteractionEnd
    );

    const commitHex = (next: string) => {
        setHexDraft(next);
        if (isValidHex(next)) onChange(normalizeHex(next));
    };

    const nudge = (dx: number, dy: number) => {
        onChange(hsvToHex({ h: hue, s: clamp01(s + dx), v: clamp01(v + dy) }));
    };

    return (
        <div className="w-[248px] space-y-3">
            {/* Saturation / value */}
            <div
                onPointerDown={sv.onPointerDown}
                onPointerMove={sv.onPointerMove}
                onPointerUp={sv.onPointerUp}
                onPointerCancel={sv.onPointerCancel}
                onKeyDown={(e) => {
                    const step = e.shiftKey ? 0.1 : 0.02;
                    const map: Record<string, [number, number]> = {
                        ArrowLeft: [-step, 0], ArrowRight: [step, 0],
                        ArrowUp: [0, step], ArrowDown: [0, -step],
                    };
                    const delta = map[e.key];
                    if (delta) { e.preventDefault(); nudge(delta[0], delta[1]); }
                }}
                role="application"
                aria-label="Saturation and brightness"
                tabIndex={0}
                className="relative h-[132px] w-full cursor-crosshair rounded-lg border border-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500/60"
                style={{
                    backgroundColor: hsvToHex({ h: hue, s: 1, v: 1 }),
                    backgroundImage:
                        'linear-gradient(to top, #000, transparent), linear-gradient(to right, #fff, transparent)',
                }}
            >
                <span
                    className="pointer-events-none absolute h-3.5 w-3.5 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-[#ffffff] shadow-[0_0_0_1px_rgba(0,0,0,0.5)]"
                    style={{ left: `${s * 100}%`, top: `${(1 - v) * 100}%`, backgroundColor: value }}
                />
            </div>

            {/* Hue */}
            <div
                onPointerDown={hueBar.onPointerDown}
                onPointerMove={hueBar.onPointerMove}
                onPointerUp={hueBar.onPointerUp}
                onPointerCancel={hueBar.onPointerCancel}
                onKeyDown={(e) => {
                    const step = e.shiftKey ? 20 : 4;
                    if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
                        e.preventDefault();
                        const h = (hue + (e.key === 'ArrowLeft' ? -step : step) + 360) % 360;
                        setHue(h);
                        onChange(hsvToHex({ h, s: s || 1, v: v || 1 }));
                    }
                }}
                role="slider"
                aria-label="Hue"
                aria-valuemin={0}
                aria-valuemax={360}
                aria-valuenow={Math.round(hue)}
                tabIndex={0}
                className="relative h-3.5 w-full cursor-ew-resize rounded-full border border-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500/60"
                style={{
                    backgroundImage:
                        'linear-gradient(to right, #f00 0%, #ff0 17%, #0f0 33%, #0ff 50%, #00f 67%, #f0f 83%, #f00 100%)',
                }}
            >
                <span
                    className="pointer-events-none absolute top-1/2 h-4 w-4 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-[#ffffff] shadow-[0_0_0_1px_rgba(0,0,0,0.5)]"
                    style={{ left: `${(hue / 360) * 100}%`, backgroundColor: hsvToHex({ h: hue, s: 1, v: 1 }) }}
                />
            </div>

            {/* Opacity */}
            {onOpacityChange && (
                <div>
                    <div className="mb-1.5 flex items-baseline justify-between">
                        <span className="text-[11px] font-medium uppercase tracking-wider text-gray-500">Opacity</span>
                        <span className="font-mono text-[11px] text-gray-300">{Math.round(opacity * 100)}%</span>
                    </div>
                    <div
                        onPointerDown={opacityBar.onPointerDown}
                        onPointerMove={opacityBar.onPointerMove}
                        onPointerUp={opacityBar.onPointerUp}
                        onPointerCancel={opacityBar.onPointerCancel}
                        onLostPointerCapture={opacityBar.onLostPointerCapture}
                        onKeyDown={(event) => {
                            if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return;
                            event.preventDefault();
                            const step = event.shiftKey ? 0.1 : 0.02;
                            onOpacityChange(clamp01(opacity + (event.key === 'ArrowLeft' ? -step : step)));
                        }}
                        role="slider"
                        aria-label="Opacity"
                        aria-valuemin={0}
                        aria-valuemax={100}
                        aria-valuenow={Math.round(opacity * 100)}
                        tabIndex={0}
                        className="relative h-3.5 w-full cursor-ew-resize overflow-visible rounded-full border border-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500/60"
                        style={{
                            backgroundColor: '#ffffff',
                            backgroundImage: 'linear-gradient(45deg, #cbd5e1 25%, transparent 25%), linear-gradient(-45deg, #cbd5e1 25%, transparent 25%), linear-gradient(45deg, transparent 75%, #cbd5e1 75%), linear-gradient(-45deg, transparent 75%, #cbd5e1 75%)',
                            backgroundPosition: '0 0, 0 4px, 4px -4px, -4px 0',
                            backgroundSize: '8px 8px',
                        }}
                    >
                        <span
                            className="absolute inset-0 rounded-full"
                            style={{ backgroundImage: `linear-gradient(to right, transparent, ${value})` }}
                        />
                        <span
                            className="pointer-events-none absolute top-1/2 h-4 w-4 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-[#ffffff] shadow-[0_0_0_1px_rgba(0,0,0,0.5)]"
                            // Shares the app slider's geometry: a knob placed at a
                            // plain share of the width hangs half outside at both
                            // ends, since it travels between its own half-widths.
                            style={{ left: sliderKnobOffset(opacity), backgroundColor: value }}
                        />
                    </div>
                </div>
            )}

            {/* Hex */}
            <div className="flex items-center gap-2">
                <span
                    className="h-8 w-8 shrink-0 rounded-lg border border-gray-600"
                    style={{ backgroundColor: value }}
                />
                <input
                    value={hexDraft}
                    onChange={(e) => commitHex(e.target.value)}
                    onBlur={() => setHexDraft(value)}
                    spellCheck={false}
                    aria-label="Hex value"
                    className={`min-w-0 flex-1 rounded-lg border bg-gray-900 px-2.5 py-1.5 font-mono text-[13px] text-white focus:outline-none focus:ring-1 ${
                        isValidHex(hexDraft)
                            ? 'border-gray-600 focus:border-blue-500 focus:ring-blue-500'
                            : 'border-red-500/60 focus:border-red-500 focus:ring-red-500'
                    }`}
                />
            </div>

            {presets.length > 0 && (
                <div>
                    <p className="mb-1.5 text-[11px] font-medium uppercase tracking-wider text-gray-500">
                        From this theme
                    </p>
                    <div className="flex flex-wrap gap-1.5">
                        {presets.map((preset, i) => (
                            <button
                                key={`${preset}-${i}`}
                                type="button"
                                onClick={() => onChange(preset)}
                                title={preset}
                                aria-label={`Use ${preset}`}
                                className={`h-6 w-6 rounded-md border transition-transform hover:scale-110 ${
                                    preset.toLowerCase() === value.toLowerCase()
                                        ? 'border-blue-400 ring-1 ring-blue-500/60'
                                        : 'border-gray-600'
                                }`}
                                style={{ backgroundColor: preset }}
                            />
                        ))}
                    </div>
                </div>
            )}
        </div>
    );
}

// ── Trigger + popover ────────────────────────────────────────────────────────

interface ColorFieldProps extends ColorPickerProps {
    label: string;
}

/** A swatch that opens the picker in a popover anchored beneath it. */
export function ColorField({
    value,
    onChange,
    opacity = 1,
    onOpacityChange,
    presets,
    label,
    onInteractionStart,
    onInteractionEnd,
}: ColorFieldProps) {
    const [open, setOpen] = useState(false);
    const [position, setPosition] = useState<{ top: number; left: number } | null>(null);
    const triggerRef = useRef<HTMLButtonElement>(null);
    const popoverRef = useRef<HTMLDivElement>(null);

    // Positioned in a portal rather than inline: the editor's colour rows live
    // in a scrolling panel, which would clip an absolutely positioned popover.
    useLayoutEffect(() => {
        if (!open) return;
        const place = () => {
            const trigger = triggerRef.current;
            if (!trigger) return;
            const r = trigger.getBoundingClientRect();
            const width = 280;
            const height = 370;
            const left = Math.min(Math.max(8, r.right - width), window.innerWidth - width - 8);
            const top = r.bottom + height > window.innerHeight - 8
                ? Math.max(8, r.top - height - 8)
                : r.bottom + 8;
            setPosition({ top, left });
        };
        place();
        window.addEventListener('resize', place);
        window.addEventListener('scroll', place, true);
        return () => {
            window.removeEventListener('resize', place);
            window.removeEventListener('scroll', place, true);
        };
    }, [open]);

    useEffect(() => {
        if (!open) return;
        const onPointerDown = (e: MouseEvent) => {
            const target = e.target as Node;
            if (popoverRef.current?.contains(target) || triggerRef.current?.contains(target)) return;
            setOpen(false);
        };
        const onKeyDown = (e: KeyboardEvent) => {
            if (e.key === 'Escape') {
                e.stopPropagation();
                setOpen(false);
                triggerRef.current?.focus();
            }
        };
        document.addEventListener('mousedown', onPointerDown);
        document.addEventListener('keydown', onKeyDown, true);
        return () => {
            document.removeEventListener('mousedown', onPointerDown);
            document.removeEventListener('keydown', onKeyDown, true);
        };
    }, [open]);

    return (
        <>
            <button
                ref={triggerRef}
                type="button"
                onClick={() => setOpen((o) => !o)}
                aria-label={`${label} colour`}
                aria-expanded={open}
                className={`h-9 w-9 shrink-0 rounded-lg border transition-colors ${
                    open ? 'border-blue-500 ring-2 ring-blue-500/40' : 'border-gray-600 hover:border-gray-500'
                }`}
                style={{
                    backgroundColor: '#ffffff',
                    backgroundImage: 'linear-gradient(45deg, #cbd5e1 25%, transparent 25%), linear-gradient(-45deg, #cbd5e1 25%, transparent 25%), linear-gradient(45deg, transparent 75%, #cbd5e1 75%), linear-gradient(-45deg, transparent 75%, #cbd5e1 75%)',
                    backgroundPosition: '0 0, 0 6px, 6px -6px, -6px 0',
                    backgroundSize: '12px 12px',
                }}
            >
                <span className="block h-full w-full rounded-[7px]" style={{ backgroundColor: value, opacity }} />
            </button>

            {open && position && createPortal(
                <div
                    ref={popoverRef}
                    style={{ top: position.top, left: position.left }}
                    className="fixed z-[80] rounded-xl border border-gray-700 bg-gray-800 p-3 shadow-2xl"
                >
                    <PickerBody
                        value={value}
                        onChange={onChange}
                        opacity={opacity}
                        onOpacityChange={onOpacityChange}
                        presets={presets}
                        onInteractionStart={onInteractionStart}
                        onInteractionEnd={onInteractionEnd}
                    />
                </div>,
                document.body
            )}
        </>
    );
}
