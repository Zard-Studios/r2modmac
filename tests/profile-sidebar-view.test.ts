import assert from 'node:assert/strict';
import test from 'node:test';

import { resolveProfileModView } from '../src/utils/profileModView.ts';

test('keeps All selected when pending Sync becomes available', () => {
    assert.equal(resolveProfileModView('all', ['all', 'sync']), 'all');
});

test('falls back to All when the selected view is no longer available', () => {
    assert.equal(resolveProfileModView('sync', ['all']), 'all');
});
