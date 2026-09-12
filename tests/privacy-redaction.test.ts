import assert from 'node:assert/strict';
import test from 'node:test';

import { censorText } from '../src/utils/pathCensorUtils.ts';

test('privacy redaction covers macOS, Linux, and Windows home paths', () => {
    const raw = '/Users/alice/Games; /home/bob/.steam; C:\\Users\\carol\\AppData';
    assert.equal(
        censorText(raw, null),
        '/Users/[user]/Games; /home/[user]/.steam; C:\\Users\\[user]\\AppData',
    );
});

test('privacy redaction covers a configured username in arbitrary text', () => {
    const raw = 'Profile ALICE failed; owner=alice; email=Alice@example.test';
    assert.equal(
        censorText(raw, 'alice'),
        'Profile [user] failed; owner=[user]; email=[user]@example.test',
    );
});

test('a username learned from a path is redacted throughout the same message', () => {
    const raw = 'Could not open /Users/alice/Game for profile alice';
    assert.equal(censorText(raw, null), 'Could not open /Users/[user]/Game for profile [user]');
});

test('special regex characters in usernames are treated literally', () => {
    assert.equal(censorText('User a.b owns /tmp/a.b', 'a.b'), 'User [user] owns /tmp/[user]');
});

test('privacy redaction leaves unrelated text unchanged', () => {
    assert.equal(censorText('Installing ExampleMod 1.2.3', null), 'Installing ExampleMod 1.2.3');
});
