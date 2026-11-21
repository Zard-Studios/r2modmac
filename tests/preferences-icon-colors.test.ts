import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

import {
    PREFERENCE_ICON_CATALOG,
    PREFERENCE_ICON_COLORS,
    PREFERENCE_ICON_NAMES,
    themedPreferenceIconStyle,
} from '../src/utils/preferencesIconColors.ts';

test('the Preferences SVG catalogue is the single source of truth', () => {
    assert.deepEqual(PREFERENCE_ICON_NAMES, Object.keys(PREFERENCE_ICON_CATALOG));
    for (const icon of PREFERENCE_ICON_NAMES) {
        assert.equal(PREFERENCE_ICON_COLORS[icon], PREFERENCE_ICON_CATALOG[icon].className, icon);
    }
});

test('the theme editor only offers icons that are still rendered in Preferences', () => {
    const preferencesSource = readFileSync(
        new URL('../src/components/modals/PreferencesModal.tsx', import.meta.url),
        'utf8'
    );
    const gameLocationsSource = readFileSync(
        new URL('../src/components/modals/GameLocationsSection.tsx', import.meta.url),
        'utf8'
    );
    const renderedIcons = [
        ...Array.from(
            preferencesSource.matchAll(/<RowIcon kind="([a-z-]+)"/g),
            (match) => match[1]
        ),
        // Game locations draws its own, themed the same way.
        ...Array.from(
            gameLocationsSource.matchAll(/themedPreferenceIconStyle\('([a-z-]+)'/g),
            (match) => match[1]
        ),
    ];

    assert.deepEqual(
        [...PREFERENCE_ICON_NAMES].sort(),
        [...new Set(renderedIcons)].sort(),
        'Removing or adding a Preferences row must update the custom-theme SVG controls too.'
    );
});

test('no two Preferences rows share an icon', () => {
    const source = readFileSync(
        new URL('../src/components/modals/PreferencesModal.tsx', import.meta.url),
        'utf8'
    );
    const kinds = Array.from(source.matchAll(/<RowIcon kind="([a-z-]+)"/g), (match) => match[1]);
    const repeated = kinds.filter((kind, index) => kinds.indexOf(kind) !== index);
    assert.deepEqual(repeated, [], 'Two settings with the same icon are easy to mix up.');
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

test('the Default theme leaves icon colours to their stock classes', () => {
    assert.equal(themedPreferenceIconStyle('version', false), undefined);
    assert.deepEqual(themedPreferenceIconStyle('version', true), {
        color: 'rgb(var(--r2-pref-icon-version))',
    });
});
