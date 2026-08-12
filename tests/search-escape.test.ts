import assert from 'node:assert/strict';
import test from 'node:test';

import { isTextEntryTarget, shouldReleaseSearchFocus, type SearchEscapeEvent } from '../src/utils/searchField.ts';

function keyEvent(overrides: Partial<SearchEscapeEvent> = {}): SearchEscapeEvent {
    return { key: 'Escape', ...overrides };
}

test('escape in a search field releases the focus instead of leaving the profile', () => {
    assert.equal(shouldReleaseSearchFocus(keyEvent()), true);
});

test('typing does not release the focus', () => {
    assert.equal(shouldReleaseSearchFocus(keyEvent({ key: 'a' })), false);
    assert.equal(shouldReleaseSearchFocus(keyEvent({ key: 'Enter' })), false);
});

test('an escape somebody else already handled is left alone', () => {
    assert.equal(shouldReleaseSearchFocus(keyEvent({ defaultPrevented: true })), false);
});

test('escape with a modifier belongs to whatever shortcut owns it', () => {
    assert.equal(shouldReleaseSearchFocus(keyEvent({ metaKey: true })), false);
    assert.equal(shouldReleaseSearchFocus(keyEvent({ ctrlKey: true })), false);
    assert.equal(shouldReleaseSearchFocus(keyEvent({ altKey: true })), false);
    assert.equal(shouldReleaseSearchFocus(keyEvent({ shiftKey: true })), false);
});

test('escape cancels an in-flight IME composition first', () => {
    assert.equal(shouldReleaseSearchFocus(keyEvent({ isComposing: true })), false);
    assert.equal(shouldReleaseSearchFocus(keyEvent({ keyCode: 229 })), false);
});

test('every field text is typed into counts, not just the mod searches', () => {
    assert.equal(isTextEntryTarget({ tagName: 'INPUT' }), true);
    assert.equal(isTextEntryTarget({ tagName: 'input', type: 'search' }), true);
    assert.equal(isTextEntryTarget({ tagName: 'INPUT', type: 'number' }), true);
    assert.equal(isTextEntryTarget({ tagName: 'TEXTAREA' }), true);
    assert.equal(isTextEntryTarget({ tagName: 'DIV', isContentEditable: true }), true);
});

test('controls that hold no text keep their own escape', () => {
    assert.equal(isTextEntryTarget({ tagName: 'INPUT', type: 'checkbox' }), false);
    assert.equal(isTextEntryTarget({ tagName: 'INPUT', type: 'range' }), false);
    assert.equal(isTextEntryTarget({ tagName: 'BUTTON' }), false);
    assert.equal(isTextEntryTarget({ tagName: 'DIV' }), false);
    assert.equal(isTextEntryTarget(null), false);
});
