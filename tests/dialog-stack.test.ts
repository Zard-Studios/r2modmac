import assert from 'node:assert/strict';
import test from 'node:test';

import { createDialogStack, isPlainEscape } from '../src/utils/dialogStack.ts';

test('only the last opened dialog is topmost', () => {
    const stack = createDialogStack();
    const first = Symbol('first');
    const second = Symbol('second');
    stack.register(first);
    stack.register(second);

    assert.equal(stack.isTop(first), false);
    assert.equal(stack.isTop(second), true);
    stack.unregister(second);
    assert.equal(stack.isTop(first), true);
});

test('removing a stale registration cannot disturb the active dialog', () => {
    const stack = createDialogStack();
    const active = Symbol('active');
    stack.register(active);
    stack.unregister(Symbol('unknown'));
    assert.equal(stack.isTop(active), true);
    assert.equal(stack.size(), 1);
});

test('only an unmodified, unhandled Escape dismisses a dialog', () => {
    assert.equal(isPlainEscape({ key: 'Escape' }), true);
    assert.equal(isPlainEscape({ key: 'Enter' }), false);
    assert.equal(isPlainEscape({ key: 'Escape', defaultPrevented: true }), false);
    assert.equal(isPlainEscape({ key: 'Escape', isComposing: true }), false);
    assert.equal(isPlainEscape({ key: 'Escape', keyCode: 229 }), false);
    assert.equal(isPlainEscape({ key: 'Escape', metaKey: true }), false);
    assert.equal(isPlainEscape({ key: 'Escape', shiftKey: true }), false);
});
