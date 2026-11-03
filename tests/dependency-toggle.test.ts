import assert from 'node:assert/strict';
import test from 'node:test';

import type { InstalledMod } from '../src/types/profile.ts';
import type { Package } from '../src/types/thunderstore.ts';
import { planDependencyToggle } from '../src/utils/dependencyToggle.ts';

const mod = (name: string, enabled: boolean): InstalledMod => ({
    uuid4: name, name, fullName: `A-${name}-1.0.0`, versionNumber: '1.0.0', enabled,
} as InstalledMod);

const pkg = (name: string, deps: string[]): Package => ({
    full_name: `A-${name}`, versions: [{ version_number: '1.0.0', dependencies: deps.map((d) => `A-${d}-1.0.0`) }],
} as unknown as Package);

const index = {
    'a-top': pkg('Top', ['Lib']),
    'a-other': pkg('Other', ['Lib']),
    'a-lib': pkg('Lib', ['Core']),
} as Record<string, Package>;

test('disabling skips a dependency another enabled mod still needs', () => {
    const mods = [mod('Top', true), mod('Other', true), mod('Lib', true), mod('Core', true)];
    assert.deepEqual(planDependencyToggle(mods, mods[0], index), []);
});

test('disabling collects the unused chain', () => {
    const mods = [mod('Top', true), mod('Lib', true), mod('Core', true)];
    assert.deepEqual(planDependencyToggle(mods, mods[0], index).map((m) => m.name).sort(), ['Core', 'Lib']);
});

test('enabling collects disabled dependencies only', () => {
    const mods = [mod('Top', false), mod('Lib', false), mod('Core', true)];
    assert.deepEqual(planDependencyToggle(mods, mods[0], index).map((m) => m.name), ['Lib']);
});
