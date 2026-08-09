import { useCallback, useEffect, useRef, useState } from 'react';

import { sliderTrackBackground } from './sliderGeometry';

/**
 * The application's slider.
 *
 * One implementation, used everywhere. There were three before — the theme
 * editor's, two hand-rolled range inputs in Preferences, and the colour
 * picker's — each with its own take on the track geometry, so a fix to one left
 * the others wrong.
 */

export function Slider({
    value, min, max, step, onChange, ariaLabel, disabled = false,
    onPreviewStart, onPreviewEnd,
}: {
    value: number; min: number; max: number; step: number;
    onChange: (n: number) => void; ariaLabel: string; disabled?: boolean;
    onPreviewStart?: () => void; onPreviewEnd?: () => void;
}) {
    const [displayValue, setDisplayValue] = useState(value);
    const liveValueRef = useRef(value);
    const draggingRef = useRef(false);
    const pendingValueRef = useRef<number | null>(null);
    const frameRef = useRef<number | null>(null);

    // While the pointer or a key is down the control belongs to the user, and
    // the incoming prop — which trails a frame behind because publishing is
    // coalesced — must not be written back over what they are doing. The two
    // pulling against each other is what made the handle jitter and refuse to
    // settle on the value being aimed at.
    useEffect(() => {
        liveValueRef.current = value;
        if (!draggingRef.current) setDisplayValue(value);
    }, [value]);

    useEffect(() => () => {
        if (frameRef.current !== null) cancelAnimationFrame(frameRef.current);
    }, []);

    const publishNextFrame = useCallback((next: number) => {
        pendingValueRef.current = next;
        if (frameRef.current !== null) return;
        frameRef.current = requestAnimationFrame(() => {
            frameRef.current = null;
            const pending = pendingValueRef.current;
            pendingValueRef.current = null;
            if (pending !== null) onChange(pending);
        });
    }, [onChange]);

    const [dragging, setDragging] = useState(false);

    const startInteraction = useCallback(() => {
        if (draggingRef.current) return;
        draggingRef.current = true;
        setDragging(true);
        onPreviewStart?.();
    }, [onPreviewStart]);

    const finishInteraction = useCallback((finalValue: number) => {
        if (!draggingRef.current) return;
        draggingRef.current = false;
        setDragging(false);
        liveValueRef.current = finalValue;
        setDisplayValue(finalValue);
        if (frameRef.current !== null) {
            cancelAnimationFrame(frameRef.current);
            frameRef.current = null;
        }
        pendingValueRef.current = null;
        onChange(finalValue);
        onPreviewEnd?.();
    }, [onChange, onPreviewEnd]);

    // A button released outside the window never reaches the input, and the
    // gesture would otherwise stay open for good — the prop would stop syncing
    // and the handle would go on answering the cursor. This backstop replaces
    // the old `blur` handler, which fired *during* the drag whenever the preview
    // overlay took focus and ended the gesture out from under the pointer.
    useEffect(() => {
        if (!dragging) return;
        const end = () => finishInteraction(liveValueRef.current);
        window.addEventListener('pointerup', end);
        window.addEventListener('pointercancel', end);
        return () => {
            window.removeEventListener('pointerup', end);
            window.removeEventListener('pointercancel', end);
        };
    }, [dragging, finishInteraction]);

    const ratio = (displayValue - min) / (max - min);
    return (
        <input
            type="range"
            disabled={disabled}
            min={min}
            max={max}
            step={step}
            value={displayValue}
            aria-label={ariaLabel}
            onInput={(event) => {
                const next = Number(event.currentTarget.value);
                liveValueRef.current = next;
                setDisplayValue(next);
                publishNextFrame(next);
            }}
            // No setPointerCapture: a native range already captures the pointer
            // for the duration of a drag. Adding a second, explicit capture that
            // nothing ever released left the control believing the button was
            // still down, so it went on tracking the cursor long after the mouse
            // had been let go.
            onPointerDown={startInteraction}
            onPointerUp={(event) => finishInteraction(Number(event.currentTarget.value))}
            onPointerCancel={() => finishInteraction(liveValueRef.current)}
            onKeyDown={(event) => {
                if (['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown', 'PageUp', 'PageDown', 'Home', 'End'].includes(event.key)) {
                    startInteraction();
                }
            }}
            onKeyUp={(event) => finishInteraction(Number(event.currentTarget.value))}
            style={{ background: sliderTrackBackground(ratio) }}
            className="h-2 w-full cursor-pointer appearance-none rounded-full border border-gray-600/70 disabled:cursor-not-allowed disabled:opacity-50 [&::-moz-range-thumb]:h-4 [&::-moz-range-thumb]:w-4 [&::-moz-range-thumb]:appearance-none [&::-moz-range-thumb]:rounded-full [&::-moz-range-thumb]:border [&::-moz-range-thumb]:border-gray-400 [&::-moz-range-thumb]:bg-white [&::-moz-range-thumb]:shadow-sm [&::-webkit-slider-thumb]:h-4 [&::-webkit-slider-thumb]:w-4 [&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:border [&::-webkit-slider-thumb]:border-gray-400 [&::-webkit-slider-thumb]:bg-white [&::-webkit-slider-thumb]:shadow-sm"
        />
    );
}
