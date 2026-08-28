import assert from 'node:assert/strict';
import test from 'node:test';

import { patchThemeToml, themeEdits } from '../src/utils/themeFile.ts';
import { normalizeTheme, themeToToml, type Theme } from '../src/utils/theme.ts';

/**
 * Saving from the visual editor used to regenerate the file from the theme held
 * in memory, which has had every missing colour filled in from the defaults. A
 * file holding three colours and the author's own comments came back holding
 * twenty colours and the app's boilerplate.
 *
 * These tests are about what the file looks like afterwards, not what the app
 * reads back: a theme that still applies but no longer resembles what its
 * author wrote is exactly the failure being fixed.
 */

/** A hand-written theme: partial, commented, and in the author's own order. */
const HAND_WRITTEN = `# My theme. Do not @ me.
name = "Midnight"

[colors]
# Only the two I actually care about.
accent     = "#88c0d0"  # picked off a photo
background = "#2e3440"
`;

function loaded(source: Partial<Theme> & { colors?: Partial<Theme['colors']> }): Theme {
    return normalizeTheme(source);
}

test('changing one colour leaves the rest of the file exactly as it was', () => {
    const base = loaded({ name: 'Midnight', colors: { accent: '#88c0d0', background: '#2e3440' } });
    const next: Theme = { ...base, colors: { ...base.colors, accent: '#a3be8c' } };

    const saved = patchThemeToml(HAND_WRITTEN, themeEdits(base, next));

    assert.equal(
        saved,
        `# My theme. Do not @ me.
name = "Midnight"

[colors]
# Only the two I actually care about.
accent     = "#a3be8c"  # picked off a photo
background = "#2e3440"
`
    );
});

test('a colour the user never touched is not written into the file', () => {
    // The regression that started this: the draft carries a default for every
    // colour, so a full rewrite spelled all of them out whether or not the
    // author had ever expressed an opinion about them.
    const base = loaded({ name: 'Midnight', colors: { accent: '#88c0d0', background: '#2e3440' } });
    const next: Theme = { ...base, colors: { ...base.colors, accent: '#a3be8c' } };

    const saved = patchThemeToml(HAND_WRITTEN, themeEdits(base, next));

    for (const untouched of ['surface', 'text_muted', 'danger', 'warning', 'success']) {
        assert.equal(saved.includes(untouched), false, `${untouched} should not appear`);
    }
    assert.equal(saved.includes('[options]'), false, 'no section the author never wrote');
});

test('comments, blank lines and key order all survive a save', () => {
    const base = loaded({ name: 'Midnight', colors: { accent: '#88c0d0', background: '#2e3440' } });
    const next: Theme = { ...base, colors: { ...base.colors, background: '#1a1a1a' } };

    const saved = patchThemeToml(HAND_WRITTEN, themeEdits(base, next));

    assert.equal(saved.startsWith('# My theme. Do not @ me.'), true);
    assert.equal(saved.includes('# Only the two I actually care about.'), true);
    assert.equal(saved.includes('# picked off a photo'), true);
    // Order is the author's, not the order the app happens to iterate in.
    assert.ok(saved.indexOf('accent') < saved.indexOf('background'));
});

test('a key the file does not have is added to its own section', () => {
    const base = loaded({ name: 'Midnight', colors: { accent: '#88c0d0', background: '#2e3440' } });
    const next: Theme = { ...base, colors: { ...base.colors, text: '#eceff4' } };

    const saved = patchThemeToml(HAND_WRITTEN, themeEdits(base, next));

    assert.equal(saved.includes('text = "#eceff4"'), true);
    // Inside [colors], not dangling at the top of the file.
    assert.ok(saved.indexOf('[colors]') < saved.indexOf('text = "#eceff4"'));
});

test('a section the file has never had is opened at the end', () => {
    const base = loaded({ name: 'Midnight', colors: { accent: '#88c0d0' } });
    const next: Theme = {
        ...base,
        options: { autoContrast: false, interfaceBlur: base.options?.interfaceBlur ?? 0 },
    };

    const saved = patchThemeToml(HAND_WRITTEN, themeEdits(base, next));

    assert.equal(saved.includes('[options]'), true);
    assert.equal(saved.includes('auto_contrast = false'), true);
    assert.ok(saved.indexOf('[colors]') < saved.indexOf('[options]'));
});

test('an individual SVG colour is patched without disturbing an unknown icon', () => {
    const source = `name = "Midnight"

[icons]
version = "#22d3ee" # cool blue
future_svg = "#ffffff"
`;
    const base = loaded({ name: 'Midnight', colors: {}, icons: { version: '#22d3ee', future_svg: '#ffffff' } });
    const next: Theme = { ...base, icons: { ...base.icons, version: '#8b5cf6' } };
    const saved = patchThemeToml(source, themeEdits(base, next));

    assert.match(saved, /^version = "#8b5cf6" # cool blue$/m);
    assert.match(saved, /^future_svg = "#ffffff"$/m);
});

test('removing the background picture takes its whole section with it', () => {
    const source = `name = "Wall"

[colors]
accent = "#88c0d0"

[background_image]
path = "assets/wall.jpg"
opacity = 0.4
blur = 6
`;
    const base = loaded({
        name: 'Wall',
        colors: { accent: '#88c0d0' },
        backgroundImage: { path: 'assets/wall.jpg', opacity: 0.4, blur: 6 },
    });
    const next: Theme = { ...base, backgroundImage: null };

    const saved = patchThemeToml(source, themeEdits(base, next));

    assert.equal(saved.includes('[background_image]'), false);
    assert.equal(saved.includes('assets/wall.jpg'), false);
    // The rest of the file is untouched.
    assert.equal(saved.includes('accent = "#88c0d0"'), true);
});

test('clearing the author removes the line rather than writing an empty one', () => {
    const source = `name = "Midnight"
author = "someone"

[colors]
accent = "#88c0d0"
`;
    const base = loaded({ name: 'Midnight', author: 'someone', colors: { accent: '#88c0d0' } });
    const next: Theme = { ...base, author: undefined };

    const saved = patchThemeToml(source, themeEdits(base, next));

    assert.equal(saved.includes('author'), false);
    assert.equal(saved.includes('name = "Midnight"'), true);
});

test('a hash inside a value is part of the colour, not the start of a comment', () => {
    // Every value in this format starts with `#`. Cutting at the first hash
    // would truncate the line being rewritten.
    const source = '[colors]\naccent = "#88c0d0"\n';
    const patched = patchThemeToml(source, [
        { kind: 'set', section: 'colors', key: 'accent', value: '#a3be8c' },
    ]);
    assert.equal(patched, '[colors]\naccent = "#a3be8c"\n');
});

test('a quote in a theme name still cannot break out of the string', () => {
    const patched = patchThemeToml('name = "old"\n', [
        { kind: 'set', section: '', key: 'name', value: 'say "hi"\\' },
    ]);
    assert.equal(patched, 'name = "say \\"hi\\"\\\\"\n');
});

test('an untouched theme saves back byte for byte', () => {
    const base = loaded({ name: 'Midnight', colors: { accent: '#88c0d0', background: '#2e3440' } });
    assert.deepEqual(themeEdits(base, base), []);
    assert.equal(patchThemeToml(HAND_WRITTEN, themeEdits(base, base)), HAND_WRITTEN);
});

test('a generated file is still patched rather than regenerated', () => {
    // Themes made in the app start life from the generator, so the patcher has
    // to handle its alignment padding and trailing comments too.
    const base = normalizeTheme({ name: 'Made here', colors: { accent: '#88c0d0' } });
    const source = themeToToml(base);
    const next: Theme = { ...base, colors: { ...base.colors, accent: '#a3be8c' } };

    const saved = patchThemeToml(source, themeEdits(base, next));

    assert.equal(
        saved.includes('accent         = "#a3be8c"  # Buttons, links and highlights'),
        true,
        'the generator\'s alignment and trailing comment both survive'
    );
    assert.equal(saved.includes('# r2modmac theme'), true);
    assert.equal(saved.split('\n').length, source.split('\n').length, 'no lines gained or lost');
});

test('opacity is written into its own section, not among the colours', () => {
    const base = loaded({ name: 'Midnight', colors: { accent: '#88c0d0' } });
    const next: Theme = { ...base, opacity: { accent: 0.5 } };

    const saved = patchThemeToml(HAND_WRITTEN, themeEdits(base, next));

    assert.equal(saved.includes('[opacity]'), true);
    assert.equal(saved.includes('accent = 0.5'), true);
    // The colour itself is untouched.
    assert.equal(saved.includes('accent     = "#88c0d0"'), true);
});
