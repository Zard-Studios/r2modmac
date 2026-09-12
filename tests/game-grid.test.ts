import assert from 'node:assert/strict';
import test from 'node:test';

import { gameGridColumnCount } from '../src/utils/gameGrid.ts';

test('large screens cap the game grid at eight cards per row', () => {
    assert.equal(gameGridColumnCount(1200), 8);
    assert.equal(gameGridColumnCount(1900), 8);
});

test('medium and small screens keep readable card sizes', () => {
    assert.equal(gameGridColumnCount(1000), 7);
    assert.equal(gameGridColumnCount(768), 5);
    assert.equal(gameGridColumnCount(640), 4);
    assert.equal(gameGridColumnCount(639), 3);
});
