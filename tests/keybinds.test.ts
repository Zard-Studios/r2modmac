import assert from 'node:assert/strict';
import test from 'node:test';

import {
    DEFAULT_KEYBINDS,
    KEYBIND_ACTIONS,
    acceleratorFromEvent,
    actionForEvent,
    findKeybindConflicts,
    formatAccelerator,
    fuzzyScore,
    isUsableAccelerator,
    normalizeAccelerator,
    overridesFromKeybinds,
    resolveKeybinds,
    type KeybindMap,
} from '../src/utils/keybinds.ts';

/**
 * The shortcut table is hand-editable and survives app updates, so the parts
 * that matter are the ones that decide whether a saved file still means what it
 * meant when it was written.
 */

function keyEvent(code: string, modifiers: Partial<Record<'ctrl' | 'alt' | 'shift' | 'meta', boolean>> = {}) {
    return {
        code,
        ctrlKey: !!modifiers.ctrl,
        altKey: !!modifiers.alt,
        shiftKey: !!modifiers.shift,
        metaKey: !!modifiers.meta,
    };
}

// ── Reading a combination off an event ───────────────────────────────────────

test('a key event becomes the combination it represents', () => {
    assert.equal(acceleratorFromEvent(keyEvent('KeyR', { meta: true })), 'Mod+R');
    assert.equal(acceleratorFromEvent(keyEvent('KeyR', { meta: true, shift: true })), 'Mod+Shift+R');
    assert.equal(acceleratorFromEvent(keyEvent('Period', { meta: true })), 'Mod+.');
    assert.equal(acceleratorFromEvent(keyEvent('Enter', { meta: true })), 'Mod+Enter');
});

test('holding a modifier on its own is not yet a shortcut', () => {
    // Otherwise the recorder would capture the instant the user pressed Command.
    assert.equal(acceleratorFromEvent(keyEvent('MetaLeft', { meta: true })), null);
    assert.equal(acceleratorFromEvent(keyEvent('ShiftLeft', { shift: true })), null);
});

test('the physical key decides, not the character it types', () => {
    // Shift+/ arrives as "?" in `key`, and a non-QWERTY layout reports another
    // letter entirely; `code` is what keeps a saved shortcut meaning one key.
    assert.equal(acceleratorFromEvent(keyEvent('Slash', { meta: true, shift: true })), 'Mod+Shift+/');
});

// ── Canonical form ───────────────────────────────────────────────────────────

test('the spellings people write by hand all land on one form', () => {
    for (const written of ['Cmd+Shift+R', 'command+shift+r', 'Shift+Mod+R', 'meta+SHIFT+R']) {
        assert.equal(normalizeAccelerator(written), 'Mod+Shift+R', written);
    }
});

test('stored modifiers read the way accelerators are normally written', () => {
    assert.equal(normalizeAccelerator('Shift+Alt+Ctrl+Mod+K'), 'Mod+Ctrl+Alt+Shift+K');
});

test('but they are printed in the order macOS uses', () => {
    assert.equal(formatAccelerator('Mod+Ctrl+Alt+Shift+K'), '⌃⌥⇧⌘K');
});

test('nonsense in the settings file is refused rather than half-read', () => {
    assert.equal(normalizeAccelerator(''), null);
    assert.equal(normalizeAccelerator('Hyper+R'), null);
    assert.equal(normalizeAccelerator('Mod+NotAKey'), null);
});

test('a shortcut has to hold something beyond an ordinary keystroke', () => {
    assert.equal(isUsableAccelerator('R'), false, 'a bare letter would fire while typing');
    assert.equal(isUsableAccelerator('Mod+R'), true);
    assert.equal(isUsableAccelerator('F5'), true, 'function keys type nothing');
});

test('combinations are shown as symbols', () => {
    assert.equal(formatAccelerator('Mod+Shift+R'), '⇧⌘R');
    assert.equal(formatAccelerator('Mod+Enter'), '⌘↩');
    assert.equal(formatAccelerator('Mod+.'), '⌘.');
});

// ── Overrides on top of defaults ─────────────────────────────────────────────

test('settings carry only what the user changed', () => {
    const keybinds: KeybindMap = { ...DEFAULT_KEYBINDS, 'launch-modded': 'Mod+Shift+L' };
    assert.deepEqual(overridesFromKeybinds(keybinds), { 'launch-modded': 'Mod+Shift+L' });
});

test('an action added in a later version arrives at its default', () => {
    // The point of storing overrides: settings written before an action existed
    // must not leave it bound to nothing.
    const resolved = resolveKeybinds({ 'launch-modded': 'Mod+Shift+L' });
    assert.equal(resolved['launch-modded'], 'Mod+Shift+L');
    assert.equal(resolved['find-profile'], DEFAULT_KEYBINDS['find-profile']);
});

test('an empty override means the user switched the shortcut off', () => {
    assert.equal(resolveKeybinds({ 'stop-game': '' })['stop-game'], '');
});

test('a stale action id or unusable combination falls back to the default', () => {
    const resolved = resolveKeybinds({
        'launched-modded': 'Mod+Z', // renamed away in some older version
        'launch-vanilla': 'R', // no modifier, would fire while typing
    });
    assert.equal(resolved['launch-vanilla'], DEFAULT_KEYBINDS['launch-vanilla']);
    assert.equal(Object.keys(resolved).length, KEYBIND_ACTIONS.length);
});

test('overrides are accepted however they were spelled by hand', () => {
    assert.equal(resolveKeybinds({ 'new-profile': 'cmd+shift+p' })['new-profile'], 'Mod+Shift+P');
});

// ── Conflicts and dispatch ───────────────────────────────────────────────────

test('the shipped defaults do not collide with each other', () => {
    assert.deepEqual([...findKeybindConflicts(DEFAULT_KEYBINDS).keys()], []);
});

test('two actions on one combination are reported', () => {
    const clashing: KeybindMap = { ...DEFAULT_KEYBINDS, 'new-profile': DEFAULT_KEYBINDS['launch-modded'] };
    const conflicts = findKeybindConflicts(clashing);
    assert.deepEqual(conflicts.get(DEFAULT_KEYBINDS['launch-modded']), ['launch-modded', 'new-profile']);
});

test('switched-off shortcuts do not count as conflicting with each other', () => {
    const off: KeybindMap = { ...DEFAULT_KEYBINDS, 'stop-game': '', 'new-profile': '' };
    assert.deepEqual([...findKeybindConflicts(off).keys()], []);
});

test('an event resolves to the action bound to it', () => {
    assert.equal(actionForEvent(keyEvent('KeyR', { meta: true }), DEFAULT_KEYBINDS), 'launch-modded');
    assert.equal(actionForEvent(keyEvent('KeyR', { meta: true, shift: true }), DEFAULT_KEYBINDS), 'launch-vanilla');
    assert.equal(actionForEvent(keyEvent('KeyR', {}), DEFAULT_KEYBINDS), null, 'unmodified R is just typing');
});

test('a switched-off shortcut fires nothing', () => {
    const off: KeybindMap = { ...DEFAULT_KEYBINDS, 'stop-game': '' };
    assert.equal(actionForEvent(keyEvent('Period', { meta: true }), off), null);
});

// ── Fuzzy find ───────────────────────────────────────────────────────────────

test('initials find a profile', () => {
    assert.notEqual(fuzzyScore('bl', 'Best Lethal'), null);
    assert.equal(fuzzyScore('zq', 'Best Lethal'), null);
});

test('characters have to appear in order', () => {
    assert.equal(fuzzyScore('lb', 'Best Lethal'), null);
});

test('word starts beat letters buried mid-word', () => {
    const atWordStart = fuzzyScore('bl', 'Best Lethal')!;
    const buried = fuzzyScore('bl', 'Bumbling')!;
    assert.ok(atWordStart > buried, `${atWordStart} should beat ${buried}`);
});

test('an adjacent run beats the same letters scattered', () => {
    const adjacent = fuzzyScore('mod', 'Modded')!;
    const scattered = fuzzyScore('mod', 'My Old Default')!;
    assert.ok(adjacent > scattered, `${adjacent} should beat ${scattered}`);
});

test('an empty query matches everything so the list starts whole', () => {
    assert.equal(fuzzyScore('', 'anything'), 0);
});

test('matching ignores case', () => {
    assert.notEqual(fuzzyScore('LETHAL', 'best lethal'), null);
});
