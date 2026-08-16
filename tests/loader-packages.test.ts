import assert from 'node:assert/strict';
import test from 'node:test';

import type { InstalledMod, Profile } from '../src/types/profile.ts';
import { isLoaderPackage, loaderDisplayName, loaderPackageIds } from '../src/utils/loaderPackages.ts';
import { hasPendingRuntimeInstall } from '../src/utils/profileSync.ts';

const profileWith = (mod: InstalledMod): Profile => ({
    id: 'hades',
    name: 'Hades II',
    gameIdentifier: 'hades-ii',
    mods: [mod],
    dateCreated: 1,
    lastUsed: 0,
});

test('the ReturnOfModding loader is more than one package', () => {
    const ids = loaderPackageIds('returnofmodding');
    // Hades II is served by Hell2Modding; Risk of Rain Returns by
    // ReturnOfModding. Matching the literal name "returnofmodding" recognised
    // only the second, so Hades II repairs found no loader at all (issue #38).
    assert.ok(ids.includes('Hell2Modding-Hell2Modding'));
    assert.ok(ids.includes('ReturnOfModding-ReturnOfModding'));

    assert.ok(isLoaderPackage('returnofmodding', 'Hell2Modding-Hell2Modding-1.0.110'));
    assert.ok(isLoaderPackage('returnofmodding', 'ReturnOfModding-ReturnOfModding-1.1.30'));
    assert.ok(!isLoaderPackage('returnofmodding', 'LuaENVY-ENVY-1.0.0'));
});

test('a community BepInEx fork still counts as the BepInEx runtime', () => {
    // Communities publish their own packs, and one published after this build
    // is still the runtime a profile is installing.
    assert.ok(isLoaderPackage('bepinex', 'BepInEx-BepInExPack_GTFO-1.0.0'));
    assert.ok(isLoaderPackage('bepinex', 'Someone-BepInExPack_BrandNewGame-1.0.0'));
    assert.ok(!isLoaderPackage('bepinex', 'Someone-SomeMod-1.0.0'));
});

test('a staged Hell2Modding install counts as the pending runtime', () => {
    const staged = profileWith({
        uuid4: 'loader',
        fullName: 'Hell2Modding-Hell2Modding-1.0.110',
        versionNumber: '1.0.110',
        enabled: true,
        pending_sync: true,
    });
    assert.equal(hasPendingRuntimeInstall(staged, 'returnofmodding'), true);
});

test('an unsupported loader is shown under its own name', () => {
    assert.equal(loaderDisplayName('returnofmodding'), 'ReturnOfModding');
    assert.equal(loaderDisplayName('melonloader'), 'melonloader');
});
