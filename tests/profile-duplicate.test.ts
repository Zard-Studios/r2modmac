import assert from 'node:assert/strict';
import test from 'node:test';

import type { Profile } from '../src/types/profile.ts';

/**
 * Duplicating a profile.
 *
 * The copy has to be a genuinely separate profile: its own id, its own name,
 * and its own mod objects. Sharing any of those with the original is the kind
 * of thing that only shows up later, as an edit to one profile appearing in
 * another.
 */

interface FakeBackend {
    copies: { source: string; destination: string }[];
    saved: Profile[][];
}

function installFakeEnvironment(): FakeBackend {
    const backend: FakeBackend = { copies: [], saved: [] };
    (globalThis as Record<string, unknown>).window = {
        ipcRenderer: {
            duplicateProfileFolder: async (source: string, destination: string) => {
                backend.copies.push({ source, destination });
                return true;
            },
            saveProfiles: async (profiles: Profile[]) => {
                backend.saved.push(profiles);
                return true;
            },
        },
    };
    if (!(globalThis as { crypto?: unknown }).crypto) {
        (globalThis as Record<string, unknown>).crypto = {};
    }
    let counter = 0;
    (globalThis.crypto as { randomUUID: () => string }).randomUUID = () =>
        `generated-id-${(counter += 1)}`;
    return backend;
}

/** Imported lazily so the fake globals exist before the module initialises. */
async function freshStore() {
    const mod = await import(`../src/store/useProfileStore.ts?bust=${Math.random()}`);
    return mod.useProfileStore;
}

function profile(overrides: Partial<Profile> = {}): Profile {
    return {
        id: 'source-profile',
        name: 'Modded',
        gameIdentifier: 'lethal-company',
        mods: [
            { uuid4: 'mod-a', fullName: 'Someone-ModA-1.0.0', versionNumber: '1.0.0', enabled: true },
            { uuid4: 'mod-b', fullName: 'Someone-ModB-2.1.0', versionNumber: '2.1.0', enabled: false },
        ],
        dateCreated: 1_000,
        lastUsed: 2_000,
        ...overrides,
    } as Profile;
}

test('the copy is a separate profile carrying the same mods', async () => {
    const backend = installFakeEnvironment();
    const store = await freshStore();
    store.getState().setProfiles([profile()]);

    const newId = await store.getState().duplicateProfile('source-profile');

    const copy = store.getState().profiles.find((p: Profile) => p.id === newId)!;
    assert.ok(copy, 'the duplicate was added');
    assert.notEqual(copy.id, 'source-profile');
    assert.equal(copy.name, 'Modded copy');
    assert.deepEqual(
        copy.mods.map((m) => m.fullName),
        ['Someone-ModA-1.0.0', 'Someone-ModB-2.1.0']
    );
    assert.equal(copy.gameIdentifier, 'lethal-company');
    assert.equal(backend.copies.length, 1, 'the folder on disk was copied too');
    assert.deepEqual(backend.copies[0], { source: 'source-profile', destination: newId });
});

test('editing the copy does not reach back into the original', async () => {
    installFakeEnvironment();
    const store = await freshStore();
    const original = profile();
    store.getState().setProfiles([original]);

    const newId = await store.getState().duplicateProfile('source-profile');
    const copy = store.getState().profiles.find((p: Profile) => p.id === newId)!;

    copy.mods[0].enabled = false;
    assert.equal(original.mods[0].enabled, true, 'mods were copied, not shared');
});

test('a second duplicate does not reuse the first copy name', async () => {
    installFakeEnvironment();
    const store = await freshStore();
    store.getState().setProfiles([profile()]);

    await store.getState().duplicateProfile('source-profile');
    const secondId = await store.getState().duplicateProfile('source-profile');

    const names = store.getState().profiles.map((p: Profile) => p.name);
    assert.deepEqual(names, ['Modded', 'Modded copy', 'Modded copy 2']);
    assert.ok(secondId);
});

test('the copy has not been applied to the game yet', async () => {
    // The duplicate exists only in r2modmac's own folder; the game directory
    // still holds whatever the original left there.
    installFakeEnvironment();
    const store = await freshStore();
    store.getState().setProfiles([profile({ needs_sync: false, apply_interrupted: true })]);

    const newId = await store.getState().duplicateProfile('source-profile');
    const copy = store.getState().profiles.find((p: Profile) => p.id === newId)!;

    assert.equal(copy.needs_sync, true);
    assert.equal(copy.apply_interrupted, false, 'an interrupted apply does not carry over');
    assert.equal(copy.lastUsed, 0, 'the copy has never been played');
});

test('an empty profile duplicates without claiming it needs applying', async () => {
    installFakeEnvironment();
    const store = await freshStore();
    store.getState().setProfiles([profile({ mods: [] })]);

    const newId = await store.getState().duplicateProfile('source-profile');
    const copy = store.getState().profiles.find((p: Profile) => p.id === newId)!;

    assert.equal(copy.needs_sync, false);
});

test('duplicating a profile that is gone changes nothing', async () => {
    const backend = installFakeEnvironment();
    const store = await freshStore();
    store.getState().setProfiles([profile()]);

    const result = await store.getState().duplicateProfile('never-existed');

    assert.equal(result, null);
    assert.equal(store.getState().profiles.length, 1);
    assert.equal(backend.copies.length, 0, 'nothing was copied on disk');
});
