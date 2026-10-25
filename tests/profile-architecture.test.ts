import assert from 'node:assert/strict';
import test from 'node:test';

import type { ArchitectureStatus } from '../src/types/electron.d.ts';
import type { Profile } from '../src/types/profile.ts';
import {
    architectureChoices,
    architectureOf,
    architectureStatusLine,
    canTryNativeAgain,
    isArchitectureSettingAvailable,
    shouldOpenArchitectureSection,
} from '../src/utils/profileArchitecture.ts';

/**
 * The profile's Architecture setting.
 *
 * The backend decides what is possible; these pin what the player is shown
 * from its answer: nothing for a profile on the default path, and a plain
 * reason (plus a way back) the moment a crash moved it to Rosetta.
 */

function status(overrides: Partial<ArchitectureStatus> = {}): ArchitectureStatus {
    return {
        applicable: true,
        hostAppleSilicon: true,
        rosettaInstalled: true,
        gameHasArm64: true,
        gameHasX64: true,
        native: { available: true, reason: null },
        rosetta: { available: true, reason: null },
        shimHint: null,
        autoWouldUse: 'native',
        pinned: null,
        mode: 'auto',
        effective: 'native',
        ...overrides,
    };
}

const date = () => '12 Oct';

test('a profile without the field is automatic', () => {
    assert.equal(architectureOf({}), 'auto');
    assert.equal(architectureOf(undefined), 'auto');
    assert.equal(architectureOf({ architecture: 'auto' }), 'auto');
    assert.equal(architectureOf({ architecture: 'native' }), 'native');
    assert.equal(architectureOf({ architecture: 'rosetta' }), 'rosetta');
    assert.equal(architectureOf({ architecture: 'bogus' as never }), 'auto');
});

test('the setting is not offered where there is nothing to choose or this is not Apple Silicon', () => {
    assert.equal(isArchitectureSettingAvailable(null), false);
    assert.equal(isArchitectureSettingAvailable(status({ applicable: false })), false);
    assert.equal(isArchitectureSettingAvailable(status({ hostAppleSilicon: false })), false);
    assert.equal(isArchitectureSettingAvailable(status()), true);
});

test('an untouched profile keeps the section closed and says nothing', () => {
    assert.equal(shouldOpenArchitectureSection(status(), 'auto'), false);
    assert.equal(architectureStatusLine(status(), 'auto', date), null);
    assert.equal(canTryNativeAgain(status(), 'auto'), false);
});

test('a choice, or a crash that moved the profile to Rosetta, opens the section', () => {
    assert.equal(shouldOpenArchitectureSection(status(), 'native'), true);
    assert.equal(shouldOpenArchitectureSection(status(), 'rosetta'), true);
    assert.equal(shouldOpenArchitectureSection(status({ pinned: { since: 1 } }), 'auto'), true);
    assert.equal(shouldOpenArchitectureSection(status({ applicable: false, pinned: { since: 1 } }), 'auto'), false);
});

test('after a crash the profile says why it uses Rosetta and offers to try again', () => {
    const crashed = status({ pinned: { since: 1_700_000_000 }, effective: 'rosetta', autoWouldUse: 'rosetta' });
    assert.equal(
        architectureStatusLine(crashed, 'auto', date),
        'Using Intel (Rosetta) since the game closed unexpectedly on Apple Silicon on 12 Oct.',
    );
    assert.equal(canTryNativeAgain(crashed, 'auto'), true);
    // A choice made by hand has nothing to retry.
    assert.equal(canTryNativeAgain(crashed, 'rosetta'), false);
});

test('a pin without Rosetta to go back to is explained instead of promising Rosetta', () => {
    const line = architectureStatusLine(
        status({ pinned: { since: 1 }, effective: 'native', rosettaInstalled: false }),
        'auto',
        date,
    );
    assert.match(line ?? '', /keeps using Apple Silicon/);
    assert.doesNotMatch(line ?? '', /Using Intel/);
});

test('a forced mode says what it does and, for Apple Silicon, how to leave it', () => {
    assert.match(architectureStatusLine(status(), 'native', date) ?? '', /switch back to Automatic/);
    assert.equal(architectureStatusLine(status(), 'rosetta', date), 'Always uses Intel (Rosetta).');
});

test('impossible choices are disabled with the reason, Automatic never is', () => {
    const intelOnly = status({
        gameHasArm64: false,
        native: { available: false, reason: 'This game has no Apple Silicon version.' },
    });
    const choices = architectureChoices(intelOnly);
    assert.deepEqual(choices.map((c) => c.id), ['auto', 'native', 'rosetta']);
    assert.equal(choices[0].disabledReason, null);
    assert.equal(choices[1].disabledReason, 'This game has no Apple Silicon version.');
    assert.equal(choices[2].disabledReason, null);

    const noRosetta = architectureChoices(
        status({ rosetta: { available: false, reason: "Rosetta isn't installed on this Mac." } }),
    );
    assert.equal(noRosetta[2].disabledReason, "Rosetta isn't installed on this Mac.");
    assert.deepEqual(architectureChoices(status()).map((c) => c.label), [
        'Automatic (recommended)',
        'Apple Silicon',
        'Intel (Rosetta)',
    ]);
});

test('only an explicit choice is kept on the profile; automatic is the field being absent', async () => {
    (globalThis as Record<string, unknown>).window = {
        ipcRenderer: { saveProfiles: async () => true },
    };
    const mod = await import(`../src/store/useProfileStore.ts?bust=${Math.random()}`);
    const store = mod.useProfileStore;
    const base: Profile = {
        id: 'p1',
        name: 'Modded',
        gameIdentifier: 'inscryption',
        mods: [],
        dateCreated: 1,
        lastUsed: 0,
        platform: 'mac',
    };
    store.setState({ profiles: [base], activeProfileId: 'p1' });

    store.getState().updateProfile('p1', { architecture: 'rosetta' });
    assert.equal(store.getState().profiles[0].architecture, 'rosetta');

    store.getState().updateProfile('p1', { architecture: 'auto' });
    assert.equal(store.getState().profiles[0].architecture, undefined);

    store.getState().updateProfile('p1', { architecture: 'sideways' as never });
    assert.equal(store.getState().profiles[0].architecture, undefined);
});
