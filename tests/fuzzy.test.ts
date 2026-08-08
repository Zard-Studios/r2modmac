import assert from 'node:assert/strict';
import test from 'node:test';

import { fuzzyScore } from '../src/utils/fuzzy.ts';

/**
 * Ranking is what makes the palette feel like it read your mind or like it did
 * not, so the cases here are about order, not merely about matching.
 */

test('initials find a profile', () => {
    assert.notEqual(fuzzyScore('bl', 'Best Lethal'), null);
    assert.equal(fuzzyScore('zq', 'Best Lethal'), null);
});

test('characters have to appear in order', () => {
    assert.equal(fuzzyScore('lb', 'Best Lethal'), null);
});

test('word starts beat letters buried mid-word', () => {
    const atWordStart = fuzzyScore('bl', 'Best Lethal')!;
    const buried = fuzzyScore('bl', 'Bumbling')!;
    assert.ok(atWordStart > buried, `${atWordStart} should beat ${buried}`);
});

test('an adjacent run beats the same letters scattered', () => {
    const adjacent = fuzzyScore('mod', 'Modded')!;
    const scattered = fuzzyScore('mod', 'My Old Default')!;
    assert.ok(adjacent > scattered, `${adjacent} should beat ${scattered}`);
});

test('an empty query matches everything so the list starts whole', () => {
    assert.equal(fuzzyScore('', 'anything'), 0);
});

test('matching ignores case', () => {
    assert.notEqual(fuzzyScore('LETHAL', 'best lethal'), null);
});
