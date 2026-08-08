import assert from 'node:assert/strict';
import test from 'node:test';

import {
    DEFAULT_KEYBINDS,
    KEYBIND_ACTIONS,
    acceleratorFromEvent,
    actionForEvent,
    findKeybindConflicts,
    formatAccelerator,
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
 *
 * Platform is a parameter rather than something sniffed inside each function,
 * which is what lets the Windows and Linux behaviour be checked from a Mac —
 * the project ships all three and there is no machine here to try them on.
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
    assert.equal(acceleratorFromEvent(keyEvent('KeyR', { meta: true }), 'apple'), 'Mod+R');
    assert.equal(acceleratorFromEvent(keyEvent('KeyR', { meta: true, shift: true }), 'apple'), 'Mod+Shift+R');
    assert.equal(acceleratorFromEvent(keyEvent('Period', { meta: true }), 'apple'), 'Mod+.');
    assert.equal(acceleratorFromEvent(keyEvent('Enter', { meta: true }), 'apple'), 'Mod+Enter');
});

test('holding a modifier on its own is not yet a shortcut', () => {
    // Otherwise the recorder would capture the instant the user pressed Command.
    assert.equal(acceleratorFromEvent(keyEvent('MetaLeft', { meta: true }), 'apple'), null);
    assert.equal(acceleratorFromEvent(keyEvent('ShiftLeft', { shift: true }), 'apple'), null);
});

test('the physical key decides, not the character it types', () => {
    // Shift+/ arrives as "?" in `key`, and a non-QWERTY layout reports another
    // letter entirely; `code` is what keeps a saved shortcut meaning one key.
    assert.equal(acceleratorFromEvent(keyEvent('Slash', { meta: true, shift: true }), 'apple'), 'Mod+Shift+/');
});

// ── Canonical form ───────────────────────────────────────────────────────────

test('the spellings people write by hand all land on one form', () => {
    for (const written of ['Cmd+Shift+R', 'command+shift+r', 'Shift+Mod+R', 'meta+SHIFT+R']) {
        assert.equal(normalizeAccelerator(written, 'apple'), 'Mod+Shift+R', written);
    }
});

test('stored modifiers read the way accelerators are normally written', () => {
    assert.equal(normalizeAccelerator('Shift+Alt+Ctrl+Mod+K', 'apple'), 'Mod+Ctrl+Alt+Shift+K');
});

test('but they are printed in the order macOS uses', () => {
    assert.equal(formatAccelerator('Mod+Ctrl+Alt+Shift+K', 'apple'), '⌃⌥⇧⌘K');
});

test('nonsense in the settings file is refused rather than half-read', () => {
    assert.equal(normalizeAccelerator('', 'apple'), null);
    assert.equal(normalizeAccelerator('Hyper+R', 'apple'), null);
    assert.equal(normalizeAccelerator('Mod+NotAKey', 'apple'), null);
});

test('a shortcut has to hold something beyond an ordinary keystroke', () => {
    assert.equal(isUsableAccelerator('R', 'apple'), false, 'a bare letter would fire while typing');
    assert.equal(isUsableAccelerator('Mod+R', 'apple'), true);
    assert.equal(isUsableAccelerator('F5', 'apple'), true, 'function keys type nothing');
});

test('combinations are shown as symbols', () => {
    assert.equal(formatAccelerator('Mod+Shift+R', 'apple'), '⇧⌘R');
    assert.equal(formatAccelerator('Mod+Enter', 'apple'), '⌘↩');
    assert.equal(formatAccelerator('Mod+.', 'apple'), '⌘.');
});

// ── Overrides on top of defaults ─────────────────────────────────────────────

test('settings carry only what the user changed', () => {
    const keybinds: KeybindMap = { ...DEFAULT_KEYBINDS, 'launch-modded': 'Mod+Shift+L' };
    assert.deepEqual(overridesFromKeybinds(keybinds), { 'launch-modded': 'Mod+Shift+L' });
});

test('an action added in a later version arrives at its default', () => {
    // The point of storing overrides: settings written before an action existed
    // must not leave it bound to nothing.
    const resolved = resolveKeybinds({ 'launch-modded': 'Mod+Shift+L' }, 'apple');
    assert.equal(resolved['launch-modded'], 'Mod+Shift+L');
    assert.equal(resolved['find-profile'], DEFAULT_KEYBINDS['find-profile']);
});

test('an empty override means the user switched the shortcut off', () => {
    assert.equal(resolveKeybinds({ 'stop-game': '' }, 'apple')['stop-game'], '');
});

test('a stale action id or unusable combination falls back to the default', () => {
    const resolved = resolveKeybinds(
        {
            'launched-modded': 'Mod+Z', // renamed away in some older version
            'launch-vanilla': 'R', // no modifier, would fire while typing
        },
        'apple'
    );
    assert.equal(resolved['launch-vanilla'], DEFAULT_KEYBINDS['launch-vanilla']);
    assert.equal(Object.keys(resolved).length, KEYBIND_ACTIONS.length);
});

test('overrides are accepted however they were spelled by hand', () => {
    assert.equal(resolveKeybinds({ 'new-profile': 'cmd+shift+p' }, 'apple')['new-profile'], 'Mod+Shift+P');
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
    assert.equal(actionForEvent(keyEvent('KeyR', { meta: true }), DEFAULT_KEYBINDS, 'apple'), 'launch-modded');
    assert.equal(
        actionForEvent(keyEvent('KeyR', { meta: true, shift: true }), DEFAULT_KEYBINDS, 'apple'),
        'launch-vanilla'
    );
    assert.equal(actionForEvent(keyEvent('KeyR', {}), DEFAULT_KEYBINDS, 'apple'), null, 'unmodified R is just typing');
});

test('a switched-off shortcut fires nothing', () => {
    const off: KeybindMap = { ...DEFAULT_KEYBINDS, 'stop-game': '' };
    assert.equal(actionForEvent(keyEvent('Period', { meta: true }), off, 'apple'), null);
});

// ── Windows and Linux ────────────────────────────────────────────────────────

test('off macOS the command modifier is Control, not the Windows key', () => {
    // The whole point of storing `Mod` rather than `Cmd`: pressing Control on a
    // PC has to produce the same shortcut a Mac produces with Command, or every
    // shipped default would need the Windows key and none would ever fire.
    assert.equal(acceleratorFromEvent(keyEvent('KeyR', { ctrl: true }), 'other'), 'Mod+R');
    assert.equal(actionForEvent(keyEvent('KeyR', { ctrl: true }), DEFAULT_KEYBINDS, 'other'), 'launch-modded');
});

test('the Windows key does not stand in for Command', () => {
    assert.equal(acceleratorFromEvent(keyEvent('KeyR', { meta: true }), 'other'), 'R');
    assert.equal(actionForEvent(keyEvent('KeyR', { meta: true }), DEFAULT_KEYBINDS, 'other'), null);
});

test('every shipped default is reachable on both platforms', () => {
    // A default nobody can press is the failure this guards; the two keyboards
    // are checked from whichever machine runs the tests.
    for (const action of KEYBIND_ACTIONS) {
        const parts = action.defaultAccelerator.split('+');
        const key = parts[parts.length - 1];
        const held = new Set(parts.slice(0, -1));
        const code =
            key === 'Enter' ? 'Enter' : key === '.' ? 'Period' : `Key${key}`;

        const apple = keyEvent(code, { meta: held.has('Mod'), shift: held.has('Shift') });
        const other = keyEvent(code, { ctrl: held.has('Mod'), shift: held.has('Shift') });

        assert.equal(actionForEvent(apple, DEFAULT_KEYBINDS, 'apple'), action.id, `${action.id} on macOS`);
        assert.equal(actionForEvent(other, DEFAULT_KEYBINDS, 'other'), action.id, `${action.id} on Windows/Linux`);
    }
});

test('shortcuts are printed the way each platform writes them', () => {
    // Glyphs for keys a PC keyboard does not carry would describe a machine the
    // user is not sitting at.
    assert.equal(formatAccelerator('Mod+Shift+R', 'other'), 'Ctrl+Shift+R');
    assert.equal(formatAccelerator('Mod+Enter', 'other'), 'Ctrl+Enter');
    assert.equal(formatAccelerator('Mod+.', 'other'), 'Ctrl+.');
});

test('off macOS Control and the command modifier are one key, not two', () => {
    // `Ctrl+Mod+K` would otherwise ask for the same key twice and never match.
    assert.equal(normalizeAccelerator('Ctrl+K', 'other'), 'Mod+K');
    assert.equal(normalizeAccelerator('Ctrl+Mod+K', 'other'), 'Mod+K');
    assert.equal(normalizeAccelerator('Ctrl+K', 'apple'), 'Ctrl+K', 'on a Mac they really are two keys');
});

test('a settings file written on one platform still means something on the other', () => {
    // Someone syncing settings between a Mac and a PC keeps their shortcuts.
    const overrides = { 'launch-modded': 'Mod+Shift+L' };
    assert.equal(resolveKeybinds(overrides, 'apple')['launch-modded'], 'Mod+Shift+L');
    assert.equal(resolveKeybinds(overrides, 'other')['launch-modded'], 'Mod+Shift+L');
});

test('a shortcut rebound under an old action name is not lost to a rename', () => {
    // The profile finder became a search across the whole app. Someone who had
    // moved it off Cmd+F should still find it where they put it.
    const resolved = resolveKeybinds({ 'find-profile': 'Mod+Shift+K' }, 'apple');
    assert.equal(resolved['open-search'], 'Mod+Shift+K');
});

test('the current name wins when a settings file carries both', () => {
    // Whichever order the keys sit in: object iteration order must not decide
    // which of the two spellings the user ends up with.
    for (const overrides of [
        { 'find-profile': 'Mod+Shift+K', 'open-search': 'Mod+Shift+J' },
        { 'open-search': 'Mod+Shift+J', 'find-profile': 'Mod+Shift+K' },
    ]) {
        assert.equal(resolveKeybinds(overrides, 'apple')['open-search'], 'Mod+Shift+J');
    }
});
