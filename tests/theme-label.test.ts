import assert from 'node:assert/strict';
import test from 'node:test';

import { activeThemeLabel } from '../src/utils/themeLabel.ts';

const files = [{ file_name: 'claudio-dark-copy.toml', name: 'Claudio Dark copy' }];

test('no selection, or the stock entry, reads Default', () => {
    assert.equal(activeThemeLabel(files, null), 'Default');
    assert.equal(activeThemeLabel(files, 'builtin:default'), 'Default');
});

test('a built-in preset shows its own name, not Default', () => {
    assert.equal(activeThemeLabel(files, 'builtin:claudio-dark'), 'Claudio Dark');
    assert.equal(activeThemeLabel([], 'builtin:github-dark'), 'GitHub Dark');
});

test('a theme file shows the name written in the file', () => {
    assert.equal(activeThemeLabel(files, 'claudio-dark-copy.toml'), 'Claudio Dark copy');
});

test('a file that is not listed yet is named, not called Default', () => {
    assert.equal(activeThemeLabel([], 'claudio-dark-copy.toml'), 'claudio-dark-copy');
});

test('a preset from an older build that no longer exists falls back to Default', () => {
    assert.equal(activeThemeLabel(files, 'builtin:gone-for-good'), 'Default');
});
