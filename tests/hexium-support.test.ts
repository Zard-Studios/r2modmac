import test from 'node:test';
import assert from 'node:assert/strict';
import type { Package, PackageVersion } from '../src/types/thunderstore.ts';
import type { InstalledMod } from '../src/types/profile.ts';
import { findPinnedVersionForSource } from '../src/utils/modVersioning.ts';
import { inferPendingSyncKind, snapshotInstalledMod } from '../src/utils/profileSync.ts';

const version = (source: 'thunderstore' | 'hexium'): PackageVersion => ({
    name: 'SharedMod',
    full_name: 'Author-SharedMod-1.0.0',
    description: '',
    icon: '',
    version_number: '1.0.0',
    dependencies: [],
    download_url: `https://example.invalid/${source}.zip`,
    downloads: 0,
    date_created: '',
    website_url: '',
    is_active: true,
    uuid4: `${source}-version`,
    file_size: 0,
    source,
});

const pkg: Package = {
    name: 'SharedMod',
    full_name: 'Author-SharedMod',
    owner: 'Author',
    package_url: '',
    date_created: '',
    date_updated: '',
    uuid4: 'shared-package',
    rating_score: 0,
    is_pinned: false,
    is_deprecated: false,
    has_nsfw_content: false,
    categories: [],
    versions: [version('thunderstore'), version('hexium')],
};

test('pinned profile entries retain their selected mod store', () => {
    assert.equal(findPinnedVersionForSource(pkg, '1.0.0', 'hexium').source, 'hexium');
    assert.equal(findPinnedVersionForSource(pkg, '1.0.0', 'thunderstore').source, 'thunderstore');
});

test('profiles without source metadata remain backward compatible', () => {
    assert.equal(findPinnedVersionForSource(pkg, '1.0.0').source, 'thunderstore');
});

test('switching store at the same version is still a sync update', () => {
    const installed: InstalledMod = {
        uuid4: 'thunderstore-version',
        fullName: 'Author-SharedMod-1.0.0',
        versionNumber: '1.0.0',
        enabled: true,
        source: 'thunderstore',
    };
    const replacement: InstalledMod = {
        ...installed,
        uuid4: 'hexium-version',
        source: 'hexium',
    };

    assert.equal(inferPendingSyncKind(replacement, snapshotInstalledMod(installed)), 'update');
});

test('a requested store never silently falls back to the other store', () => {
    const thunderstoreOnly = { ...pkg, versions: [version('thunderstore')] };

    assert.throws(
        () => findPinnedVersionForSource(thunderstoreOnly, '1.0.0', 'hexium'),
        /from hexium is unavailable/,
    );
});
