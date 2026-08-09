import assert from 'node:assert/strict';
import test from 'node:test';

import type { InstalledMod, Profile } from '../src/types/profile.ts';
import { migratePendingSyncBaselines } from '../src/utils/profileSync.ts';

const mod = (overrides: Partial<InstalledMod> = {}): InstalledMod => ({
    uuid4: 'mod-a',
    fullName: 'Someone-ModA-1.0.0',
    versionNumber: '1.0.0',
    enabled: true,
    synced_enabled: true,
    ...overrides,
});

const profile = (mods: InstalledMod[], needsSync = true): Profile => ({
    id: 'survivor',
    name: 'Original copy',
    gameIdentifier: 'lethal-company',
    mods,
    needs_sync: needsSync,
    dateCreated: 1,
    lastUsed: 0,
});

const emptyInspection = {
    status: 'ready' as const,
    runtime: 'bepinex' as const,
    mods: [],
    unresolvedKeys: [],
};

test('a surviving duplicate exposes missing game mods in the Sync tab', () => {
    const result = migratePendingSyncBaselines(profile([mod()]), emptyInspection);

    assert.equal(result.needs_sync, true);
    assert.equal(result.mods[0].pending_sync, true);
    assert.equal(result.mods[0].pending_sync_kind, 'add');
    assert.equal(result.mods[0].pending_sync_status, 'queued');
    assert.equal(result.mods[0].sync_baseline, null);
});

test('a disabled mod missing from the game is already in its desired state', () => {
    const result = migratePendingSyncBaselines(profile([mod({ enabled: false, synced_enabled: false })]), emptyInspection);

    assert.equal(result.mods[0].pending_sync, undefined);
    assert.equal(result.needs_sync, false);
});

test('an identical installed mod clears a generic sync marker', () => {
    const installed = mod();
    const result = migratePendingSyncBaselines(profile([installed]), {
        ...emptyInspection,
        mods: [{
            packageKey: 'someone-moda',
            fullName: installed.fullName,
            versionNumber: installed.versionNumber,
            enabled: true,
        }],
    });

    assert.equal(result.mods[0].pending_sync, undefined);
    assert.equal(result.needs_sync, false);
});

test('a changed installed version becomes an update in Sync', () => {
    const result = migratePendingSyncBaselines(profile([mod()]), {
        ...emptyInspection,
        mods: [{
            packageKey: 'someone-moda',
            fullName: 'Someone-ModA-0.9.0',
            versionNumber: '0.9.0',
            enabled: true,
        }],
    });

    assert.equal(result.mods[0].pending_sync, true);
    assert.equal(result.mods[0].pending_sync_kind, 'update');
    assert.equal(result.needs_sync, true);
});
