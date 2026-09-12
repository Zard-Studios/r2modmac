import assert from 'node:assert/strict';
import test from 'node:test';

import { shouldShowPreferencesModal } from '../src/utils/modalVisibility.ts';

test('an update interrupts Preferences instead of rendering behind it', () => {
    assert.equal(shouldShowPreferencesModal(true, true), false);
});

test('Preferences returns after the update dialog closes', () => {
    assert.equal(shouldShowPreferencesModal(true, false), true);
});

test('closing Preferences remains closed regardless of update state', () => {
    assert.equal(shouldShowPreferencesModal(false, false), false);
    assert.equal(shouldShowPreferencesModal(false, true), false);
});
