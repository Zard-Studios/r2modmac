import assert from 'node:assert/strict';
import test from 'node:test';
import {
    ALL_PROVIDERS,
    isProviderFilterActive,
    sourceName,
    toggleProvider,
} from '../src/utils/storeProviders.ts';

test('both stores are on by default', () => {
    assert.deepEqual(ALL_PROVIDERS, ['thunderstore', 'hexium']);
    assert.equal(isProviderFilterActive(ALL_PROVIDERS), false);
});

test('switching a store off leaves the other one on', () => {
    assert.deepEqual(toggleProvider(['thunderstore', 'hexium'], 'hexium'), ['thunderstore']);
    assert.deepEqual(toggleProvider(['thunderstore', 'hexium'], 'thunderstore'), ['hexium']);
    assert.equal(isProviderFilterActive(['thunderstore']), true);
});

test('the last store cannot be switched off', () => {
    const only: ('thunderstore' | 'hexium')[] = ['hexium'];
    assert.equal(toggleProvider(only, 'hexium'), only);
});

test('switching a store back on restores the stable order', () => {
    assert.deepEqual(toggleProvider(['hexium'], 'thunderstore'), ['thunderstore', 'hexium']);
    assert.deepEqual(toggleProvider(['thunderstore'], 'hexium'), ['thunderstore', 'hexium']);
});

test('stores are named for the people reading the list', () => {
    assert.equal(sourceName('hexium'), 'Hexium');
    assert.equal(sourceName('thunderstore'), 'Thunderstore');
    assert.equal(sourceName(undefined), 'Thunderstore');
});
