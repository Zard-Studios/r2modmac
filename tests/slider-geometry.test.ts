import assert from 'node:assert/strict';
import test from 'node:test';

import { SLIDER_THUMB, sliderFillStop, sliderKnobOffset } from '../src/components/ui/sliderGeometry.ts';

/**
 * The rule the sliders kept getting wrong: the knob travels between its own
 * half-widths, so anything measured against the whole track drifts from it —
 * invisibly in the middle, and glaringly at the ends.
 */

test('the fill ends under the knob, never past it', () => {
    // At rest it is exactly the knob, so nothing shows beside it.
    assert.equal(sliderFillStop(0), `calc(${SLIDER_THUMB}px + (100% - ${SLIDER_THUMB}px) * 0)`);
    // At full it is the whole track, with no gap left at the end.
    assert.equal(sliderFillStop(1), `calc(${SLIDER_THUMB}px + (100% - ${SLIDER_THUMB}px) * 1)`);
});

test('the knob is placed by its centre, inset by half its width', () => {
    assert.equal(sliderKnobOffset(0), `calc(${SLIDER_THUMB / 2}px + (100% - ${SLIDER_THUMB}px) * 0)`);
    assert.equal(sliderKnobOffset(1), `calc(${SLIDER_THUMB / 2}px + (100% - ${SLIDER_THUMB}px) * 1)`);
});

test('the fill always reaches half a knob further than the knob centre', () => {
    // Which is what keeps a pixel of disagreement hidden underneath the knob
    // rather than on show beside it.
    for (const ratio of [0, 0.25, 0.58, 0.99, 1]) {
        const fill = sliderFillStop(ratio);
        const knob = sliderKnobOffset(ratio);
        assert.ok(fill.startsWith(`calc(${SLIDER_THUMB}px`), fill);
        assert.ok(knob.startsWith(`calc(${SLIDER_THUMB / 2}px`), knob);
    }
});

test('values outside the range are clamped rather than overflowing the track', () => {
    assert.equal(sliderFillStop(-0.5), sliderFillStop(0));
    assert.equal(sliderFillStop(2), sliderFillStop(1));
    assert.equal(sliderKnobOffset(-1), sliderKnobOffset(0));
    assert.equal(sliderKnobOffset(9), sliderKnobOffset(1));
});
