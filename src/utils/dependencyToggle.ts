import type { Package } from '../types/thunderstore';
import type { InstalledMod } from '../types/profile';
import { getProfileModKey } from './profileSync.ts';

const isLoaderKey = (key: string) => key.toLowerCase().includes('bepinexpack');

/** Installed mods the given mod depends on, keyed by profile key. */
function installedDependencies(
    mod: InstalledMod,
    byKey: Map<string, InstalledMod>,
    index: Record<string, Package>,
): InstalledMod[] {
    const pkg = index[getProfileModKey(mod.fullName)];
    const version = pkg?.versions.find((v) => v.version_number === mod.versionNumber) ?? pkg?.versions[0];
    const own = getProfileModKey(mod.fullName);
    const found: InstalledMod[] = [];
    for (const dependency of version?.dependencies ?? []) {
        const key = getProfileModKey(dependency);
        if (key === own || isLoaderKey(key)) continue;
        const installed = byKey.get(key);
        if (installed) found.push(installed);
    }
    return found;
}

/**
 * The dependencies that should follow `target` when it is switched.
 * Enabling: every installed dependency (transitively) that is currently off.
 * Disabling: every enabled dependency (transitively) that no other enabled mod still needs.
 */
export function planDependencyToggle(
    mods: InstalledMod[],
    target: InstalledMod,
    index: Record<string, Package>,
): InstalledMod[] {
    const byKey = new Map(mods.map((mod) => [getProfileModKey(mod.fullName), mod]));
    const enabling = !target.enabled;

    const closure = new Map<string, InstalledMod>();
    const walk = (mod: InstalledMod) => {
        for (const dep of installedDependencies(mod, byKey, index)) {
            const key = getProfileModKey(dep.fullName);
            if (dep.uuid4 === target.uuid4 || closure.has(key)) continue;
            if (dep.enabled === enabling) continue;
            closure.set(key, dep);
            walk(dep);
        }
    };
    walk(target);
    if (enabling) return [...closure.values()];

    // Drop candidates another enabled mod outside the group still relies on.
    let changed = true;
    while (changed) {
        changed = false;
        const leaving = new Set([target.uuid4, ...[...closure.values()].map((m) => m.uuid4)]);
        for (const [key] of closure) {
            const needed = mods.some((other) =>
                other.enabled
                && !leaving.has(other.uuid4)
                && installedDependencies(other, byKey, index).some((dep) => getProfileModKey(dep.fullName) === key));
            if (needed) {
                closure.delete(key);
                changed = true;
                break;
            }
        }
    }
    return [...closure.values()];
}
