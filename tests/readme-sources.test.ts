import assert from 'node:assert/strict';
import test from 'node:test';

import {
    MIN_USEFUL_README_CHARS,
    availableReadmeSources,
    githubBases,
    hasUsefulReadme,
    readmeSourceName,
    resolveAgainst,
    visibleTextLength,
} from '../src/utils/readmeSources.ts';

const { image, link } = githubBases('Hydraxous', 'UltraFunGuns');

test('visible text leaves out markup, pictures and addresses', () => {
    assert.equal(visibleTextLength({ markdown: '# Title\n\n![pic](https://x.example/a.png)\n\n[docs](https://x.example) here' }), 'Title docs here'.length);
    assert.equal(visibleTextLength({ html: '<h1>Title</h1><p>Some <b>bold</b> text</p>' }), 'Title Some bold text'.length);
    assert.equal(visibleTextLength({ markdown: '' }), 0);
});

test('a README of only pictures is not useful', () => {
    assert.ok(!hasUsefulReadme({ markdown: '![a](https://x/a.png)\n![b](https://x/b.png)' }));
    assert.ok(!hasUsefulReadme(null));
    assert.ok(!hasUsefulReadme({ markdown: 'Short.' }));
    assert.ok(hasUsefulReadme({ markdown: 'word '.repeat(MIN_USEFUL_README_CHARS) }));
});

test('relative pictures and links are made to point into the repository', () => {
    assert.equal(resolveAgainst('docs/a.png', image), 'https://raw.githubusercontent.com/Hydraxous/UltraFunGuns/HEAD/docs/a.png');
    assert.equal(resolveAgainst('./docs/a.png', image), 'https://raw.githubusercontent.com/Hydraxous/UltraFunGuns/HEAD/docs/a.png');
    assert.equal(resolveAgainst('/docs/a.png', image), 'https://raw.githubusercontent.com/Hydraxous/UltraFunGuns/HEAD/docs/a.png');
    assert.equal(resolveAgainst('CHANGELOG.md', link), 'https://github.com/Hydraxous/UltraFunGuns/blob/HEAD/CHANGELOG.md');
});

test('complete addresses and in-page links are left alone', () => {
    assert.equal(resolveAgainst('https://i.imgur.com/a.png', image), 'https://i.imgur.com/a.png');
    assert.equal(resolveAgainst('//cdn.example/a.png', image), '//cdn.example/a.png');
    assert.equal(resolveAgainst('#installation', link), '#installation');
    assert.equal(resolveAgainst('mailto:me@example.com', link), 'mailto:me@example.com');
    assert.equal(resolveAgainst('data:image/png;base64,AAAA', image), 'data:image/png;base64,AAAA');
    assert.equal(resolveAgainst('', image), '');
});

test('an address that would climb out of the repository stays as it was', () => {
    assert.equal(resolveAgainst('../../secret.png', image), '../../secret.png');
    assert.equal(resolveAgainst('docs/../../x.png', image), 'docs/../../x.png');
});

test('each source has a name to show', () => {
    assert.equal(readmeSourceName('thunderstore'), 'Thunderstore');
    assert.equal(readmeSourceName('hexium'), 'Hexium');
    assert.equal(readmeSourceName('outerwilds'), 'Outer Wilds Mods');
    assert.equal(readmeSourceName('github'), 'GitHub');
});

test('a mod offers its store first, and GitHub only when it points to a repository', () => {
    assert.deepEqual(availableReadmeSources('thunderstore', true), ['thunderstore', 'github']);
    assert.deepEqual(availableReadmeSources('hexium', true), ['hexium', 'github']);
    assert.deepEqual(availableReadmeSources('hexium', false), ['hexium']);
    assert.deepEqual(availableReadmeSources(undefined, false), ['thunderstore']);
});
