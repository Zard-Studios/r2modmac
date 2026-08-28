import assert from 'node:assert/strict';
import test from 'node:test';

import {
    PREFERENCE_ICON_CATALOG,
    PREFERENCE_ICON_COLORS,
    PREFERENCE_ICON_NAMES,
} from '../src/utils/preferencesIconColors.ts';

test('the Preferences SVG catalogue is the single source of truth', () => {
    assert.deepEqual(PREFERENCE_ICON_NAMES, Object.keys(PREFERENCE_ICON_CATALOG));
    for (const icon of PREFERENCE_ICON_NAMES) {
        assert.equal(PREFERENCE_ICON_COLORS[icon], PREFERENCE_ICON_CATALOG[icon].className, icon);
    }
});

test('the default palette remains multicolour with semantic status icons', () => {
    assert.equal(PREFERENCE_ICON_COLORS.version, 'text-cyan-400');
    assert.equal(PREFERENCE_ICON_COLORS.parallel, 'text-violet-400');
    assert.equal(PREFERENCE_ICON_COLORS.profile, 'text-purple-400');
    assert.equal(PREFERENCE_ICON_COLORS.apply, 'text-fg-success');
    assert.equal(PREFERENCE_ICON_COLORS.update, 'text-fg-success');
    assert.equal(PREFERENCE_ICON_COLORS.warning, 'text-fg-warning');
    assert.equal(PREFERENCE_ICON_COLORS.cache, 'text-fg-danger');
});
