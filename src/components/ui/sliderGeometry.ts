/**
 * Slider track geometry, shared by every slider in the app.
 *
 * Kept apart from the component so the colour picker — whose track shows a
 * checkerboard and the colour itself, not a plain fill — can still place its
 * knob by the same rules. A knob positioned at a plain share of the width hangs
 * half outside at both ends, which is the mistake this exists to prevent.
 */

/** The knob's diameter in pixels; the track geometry is derived from it. */
export const SLIDER_THUMB = 16;

/**
 * Where the filled part of a track ends, for a value between 0 and 1.
 *
 * The knob only travels between its own half-widths, so a fill measured against
 * the whole track overshoots it. Ending the fill at the knob's *centre* fixes
 * the arithmetic but not the picture: any pixel of disagreement with where the
 * engine actually lays the knob is left on show. Ending it at the knob's
 * trailing edge hides that disagreement underneath the knob, which is opaque —
 * so nothing peeks out at either end, whatever the engine rounds to. At the
 * extremes it works out to exactly the knob and exactly the full track.
 */
export function sliderFillStop(ratio: number, thumb = SLIDER_THUMB): string {
    const clamped = Math.min(1, Math.max(0, ratio));
    return `calc(${thumb}px + (100% - ${thumb}px) * ${clamped})`;
}

/** Where the knob's centre sits, for a value between 0 and 1. */
export function sliderKnobOffset(ratio: number, thumb = SLIDER_THUMB): string {
    const clamped = Math.min(1, Math.max(0, ratio));
    return `calc(${thumb / 2}px + (100% - ${thumb}px) * ${clamped})`;
}

/** The track background: filled up to the knob, plain after it. */
export function sliderTrackBackground(ratio: number): string {
    const stop = sliderFillStop(ratio);
    return [
        'linear-gradient(to right,',
        `rgb(var(--r2-blue-600) / var(--r2-blue-600-alpha, 1)) ${stop},`,
        `rgb(var(--r2-gray-700) / var(--r2-gray-700-alpha, 1)) ${stop})`,
    ].join(' ');
}
