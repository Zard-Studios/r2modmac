import assert from 'node:assert/strict';
import test from 'node:test';

import { looksLikeUnrenderedMarkdown, repairEscapedMarkdown } from '../src/utils/markdownRepair.ts';

test('an escaped list marker at the start of a line is repaired', () => {
    assert.equal(repairEscapedMarkdown('\\- one\n\\- two'), '- one\n- two');
    assert.equal(repairEscapedMarkdown('\\* star\n\\+ plus'), '* star\n+ plus');
    assert.equal(repairEscapedMarkdown('  \\- nested'), '  - nested');
    assert.equal(repairEscapedMarkdown('1\\. first\n2\\. second'), '1. first\n2. second');
});

test('escaped headings and quotes are repaired', () => {
    assert.equal(repairEscapedMarkdown('\\## Title'), '## Title');
    assert.equal(repairEscapedMarkdown('\\> quoted'), '> quoted');
});

test('an escaped image that is also a link is repaired', () => {
    const escaped = '\\[!\\[Watch the trailer](https://img.example/a.jpg)](https://www.youtube.com/watch?v=abc)';
    assert.equal(
        repairEscapedMarkdown(escaped),
        '[![Watch the trailer](https://img.example/a.jpg)](https://www.youtube.com/watch?v=abc)',
    );
});

test('a backslash that is not at the start of a line is left alone', () => {
    assert.equal(repairEscapedMarkdown('a \\- b'), 'a \\- b');
    assert.equal(repairEscapedMarkdown('use C:\\- path'), 'use C:\\- path');
    assert.equal(repairEscapedMarkdown('\\-no space'), '\\-no space');
});

test('code is never touched', () => {
    const fenced = '```\n\\- not a list\n\\[!\\[x](a)](b)\n```\n\\- real';
    assert.equal(repairEscapedMarkdown(fenced), '```\n\\- not a list\n\\[!\\[x](a)](b)\n```\n- real');
});

test('Windows line endings survive', () => {
    assert.equal(repairEscapedMarkdown('\\- a\r\n\\- b\r\n'), '- a\r\n- b\r\n');
});

test('html that still shows markdown symbols is recognised', () => {
    const polarite =
        '<p>Works with <strong>Steam.</strong></p>\n<p>- Player movement</p>\n<p>- Weapons equipped</p>\n<p>- Respawning</p>';
    assert.ok(looksLikeUnrenderedMarkdown(polarite));
    assert.ok(looksLikeUnrenderedMarkdown('<p>[![Watch](<a href="x">x</a>)](<a href="y">y</a>)</p>'));
});

test('well made html is not mistaken for it', () => {
    assert.ok(!looksLikeUnrenderedMarkdown('<ul><li>One</li><li>Two</li></ul><p>Hello</p>'));
    assert.ok(!looksLikeUnrenderedMarkdown('<p>- only one stray line</p><p>Fine</p>'));
    assert.ok(!looksLikeUnrenderedMarkdown('<p>Version 2.0 is out</p>'));
    assert.ok(!looksLikeUnrenderedMarkdown(''));
});
