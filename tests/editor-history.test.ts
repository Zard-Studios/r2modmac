import assert from 'node:assert/strict';
import test from 'node:test';

import {
    EMPTY_HISTORY,
    pushEdit,
    stepBack,
    stepForward,
} from '../src/utils/editorHistory.ts';

/**
 * The theme file editor's undo stack.
 *
 * Written after the editor shipped with edits that never landed: the state
 * update was nested inside another update's updater, so typing changed nothing
 * and Save — which compares against the saved text — stayed inert. Extracting
 * the logic is what makes it checkable at all.
 */

test('an edit moves the text forward and remembers what it replaced', () => {
    const first = pushEdit(EMPTY_HISTORY, 'one', 'two');
    assert.equal(first.source, 'two');
    assert.deepEqual(first.history.undo, ['one']);
    assert.deepEqual(first.history.redo, []);
});

test('typing the same text again changes nothing', () => {
    // Otherwise every keystroke that landed on the same value would push a
    // no-op onto the stack and undo would appear to do nothing.
    const state = pushEdit(EMPTY_HISTORY, 'same', 'same');
    assert.equal(state.source, 'same');
    assert.deepEqual(state.history, EMPTY_HISTORY);
});

test('undo returns the previous text and offers it back to redo', () => {
    const edited = pushEdit(EMPTY_HISTORY, 'one', 'two');
    const back = stepBack(edited.history, edited.source);
    assert.equal(back.source, 'one');
    assert.deepEqual(back.history.undo, []);
    assert.deepEqual(back.history.redo, ['two']);

    const forward = stepForward(back.history, back.source);
    assert.equal(forward.source, 'two');
    assert.deepEqual(forward.history.undo, ['one']);
    assert.deepEqual(forward.history.redo, []);
});

test('undo and redo at the ends of the stack are no-ops', () => {
    assert.deepEqual(stepBack(EMPTY_HISTORY, 'x'), { history: EMPTY_HISTORY, source: 'x' });
    assert.deepEqual(stepForward(EMPTY_HISTORY, 'x'), { history: EMPTY_HISTORY, source: 'x' });
});

test('a new edit after undoing discards the redo branch', () => {
    // Standard editor behaviour: once you type over an undone change, there is
    // nothing sensible left to redo to.
    let state = pushEdit(EMPTY_HISTORY, 'a', 'b');
    state = stepBack(state.history, state.source);
    assert.deepEqual(state.history.redo, ['b']);

    state = pushEdit(state.history, state.source, 'c');
    assert.deepEqual(state.history.redo, []);
    assert.equal(state.source, 'c');
});

test('a long session walks all the way back through its edits', () => {
    let state = { history: EMPTY_HISTORY, source: 'v0' };
    for (let i = 1; i <= 10; i++) state = pushEdit(state.history, state.source, `v${i}`);
    assert.equal(state.source, 'v10');

    for (let i = 9; i >= 0; i--) {
        state = stepBack(state.history, state.source);
        assert.equal(state.source, `v${i}`);
    }
    assert.deepEqual(state.history.undo, []);
});

test('the stack is bounded, keeping the most recent steps', () => {
    let state = { history: EMPTY_HISTORY, source: 'v0' };
    for (let i = 1; i <= 260; i++) state = pushEdit(state.history, state.source, `v${i}`);

    assert.equal(state.history.undo.length, 200, 'stack should stop growing');
    // The oldest steps are the ones dropped, not the newest.
    assert.equal(state.history.undo[state.history.undo.length - 1], 'v259');
});

test('history operations never mutate what they were given', () => {
    const original = { undo: ['a'], redo: ['b'] };
    const snapshot = JSON.parse(JSON.stringify(original));
    pushEdit(original, 'c', 'd');
    stepBack(original, 'c');
    stepForward(original, 'c');
    assert.deepEqual(original, snapshot);
});
