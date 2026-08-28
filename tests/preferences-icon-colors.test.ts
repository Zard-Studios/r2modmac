import assert from 'node:assert/strict';
import test from 'node:test';

import { PREFERENCE_ICON_COLORS } from '../src/utils/preferencesIconColors.ts';

test('preference decoration follows the theme icon token', () => {
    const decorative = [
        'install', 'version', 'parallel', 'logs', 'layout', 'stream',
        'support', 'folder', 'game', 'profile', 'theme', 'keyboard',
    ] as const;
    for (const icon of decorative) {
        assert.equal(PREFERENCE_ICON_COLORS[icon], 'text-fg-icon', icon);
    }
});

test('semantic preference icons keep their status meaning', () => {
    assert.equal(PREFERENCE_ICON_COLORS.apply, 'text-fg-success');
    assert.equal(PREFERENCE_ICON_COLORS.update, 'text-fg-success');
    assert.equal(PREFERENCE_ICON_COLORS.warning, 'text-fg-warning');
    assert.equal(PREFERENCE_ICON_COLORS.cache, 'text-fg-danger');
});
