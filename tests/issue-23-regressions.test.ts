import assert from 'node:assert/strict';
import test from 'node:test';

import {
    findPinnedVersion,
    latestVersionNumber,
    packageIdentityKey,
    satisfiesMinimumVersion,
} from '../src/utils/modVersioning.ts';
import type { Package, PackageVersion } from '../src/types/thunderstore.ts';

const version = (versionNumber: string): PackageVersion => ({
    name: 'BepInExPack',
    full_name: `bbepis-BepInExPack-${versionNumber}`,
    description: '',
    icon: '',
    version_number: versionNumber,
    dependencies: [],
    download_url: `https://example.invalid/${versionNumber}.zip`,
    downloads: 0,
    date_created: '',
    website_url: '',
    is_active: true,
    uuid4: `bepinex-${versionNumber}`,
    file_size: 1,
});

const bepinexPackage = (versions: PackageVersion[]): Package => ({
    name: 'BepInExPack',
    full_name: 'bbepis-BepInExPack',
    owner: 'bbepis',
    package_url: '',
    date_created: '',
    date_updated: '',
    uuid4: 'bepinex-package',
    rating_score: 0,
    is_pinned: false,
    is_deprecated: false,
    has_nsfw_content: false,
    categories: [],
    versions,
});

test('StageRecap cannot downgrade an already newer BepInEx dependency', () => {
    const installed = 'bbepis-BepInExPack-5.4.2108';
    const historicalPin = 'bbepis-BepInExPack-5.4.1905';

    assert.equal(packageIdentityKey(installed), packageIdentityKey(historicalPin));
    assert.equal(satisfiesMinimumVersion('5.4.2108', '5.4.1905'), true);
    assert.equal(satisfiesMinimumVersion('5.4.1905', '5.4.2108'), false);
});

test('different BepInEx versions resolve to one package identity', () => {
    const identities = new Set([
        'bbepis-BepInExPack-5.4.1905',
        'bbepis-BepInExPack-5.4.2108',
        'bbepis-BepInExPack-5.4.2121',
    ].map(packageIdentityKey));

    assert.deepEqual([...identities], ['bbepis-bepinexpack']);
});

test('an unavailable dependency pin is rejected instead of silently using latest', () => {
    const pkg = bepinexPackage([version('5.4.2121')]);
    assert.throws(
        () => findPinnedVersion(pkg, '5.4.1905', 'bbepis-BepInExPack'),
        /pinned version 5\.4\.1905 is unavailable; refusing to use latest/,
    );
});

test('Update All selects the semantic latest version, not array order', () => {
    const pkg = bepinexPackage([
        version('5.4.2108'),
        version('5.4.2121-beta.1'),
        version('5.4.2121'),
        version('5.4.1905'),
    ]);

    assert.equal(latestVersionNumber(pkg), '5.4.2121');
});
