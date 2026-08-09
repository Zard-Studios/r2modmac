import assert from 'node:assert/strict';
import test from 'node:test';

import { backgroundLayerStyle } from '../src/utils/theme.ts';

/**
 * How the background picture is laid out.
 *
 * There were two of these once — the window's and the editor preview's — and
 * they disagreed. These cases pin the one that is left.
 */

const image = (over: Partial<Parameters<typeof backgroundLayerStyle>[0]> = {}) =>
    backgroundLayerStyle({ path: 'assets/wall.png', opacity: 1, blur: 0, ...over } as never);

test('each sizing maps to the CSS that means it', () => {
    assert.equal(image({ fit: 'cover' }).backgroundSize, 'cover');
    assert.equal(image({ fit: 'contain' }).backgroundSize, 'contain');
    assert.equal(image({ fit: 'fill' }).backgroundSize, '100% 100%');
    assert.equal(image({ fit: 'center' }).backgroundSize, 'auto');
});

test('only the pattern repeats', () => {
    assert.equal(image({ fit: 'tile', tile_scale: 40 }).backgroundRepeat, 'repeat');
    assert.equal(image({ fit: 'tile', tile_scale: 40 }).backgroundSize, '40% auto');
    for (const fit of ['cover', 'contain', 'fill', 'center'] as const) {
        assert.equal(image({ fit }).backgroundRepeat, 'no-repeat', fit);
    }
});

test('the pattern scale is clamped so a tile cannot vanish or fill the screen', () => {
    assert.equal(image({ fit: 'tile', tile_scale: 0 }).backgroundSize, '2% auto');
    assert.equal(image({ fit: 'tile', tile_scale: 900 }).backgroundSize, '100% auto');
});

test('a picture with no offsets set is centred', () => {
    // Which is what "contain" and "original" are expected to look like: the
    // whole picture, in the middle, with the spare room split evenly.
    for (const fit of ['cover', 'contain', 'fill', 'tile', 'center'] as const) {
        assert.equal(image({ fit }).backgroundPosition, '50% 50%', fit);
    }
});

test('offsets are honoured and clamped to the track', () => {
    assert.equal(image({ offset_x: 0, offset_y: 100 }).backgroundPosition, '0% 100%');
    assert.equal(image({ offset_x: -20, offset_y: 480 }).backgroundPosition, '0% 100%');
});

test('the blur overscan applies only when there is blur to hide', () => {
    // Enlarging the layer is how a blurred edge is kept off screen. Doing it
    // unconditionally cropped the very edges "contain" promises to keep.
    assert.equal(image({ fit: 'contain', blur: 0 }).scale, '1');
    assert.equal(image({ fit: 'center', blur: 0 }).scale, '1');
    assert.equal(image({ fit: 'contain', blur: 12 }).scale, '1.06');
    assert.equal(image({ fit: 'cover', blur: 0 }).scale, '1');
});

test('blur reaches the layer as a length', () => {
    assert.equal(image({ blur: 12 }).filter, 'blur(12px)');
    assert.equal(image({ blur: 0 }).filter, 'blur(0px)');
});
