import type {
    InstalledMod,
    InstalledModSnapshot,
    PendingSyncKind,
    Profile,
} from '../types/profile';
import { parsePackageReference } from './modVersioning';
import type { RuntimeHealth } from '../types/electron';

export interface ProfileSyncInspectionMod {
    packageKey: string;
    fullName: string;
    versionNumber?: string;
    enabled?: boolean;
}

export interface ProfileSyncInspection {
    status: 'ready' | 'unconfigured' | 'unsupported';
    runtime: 'bepinex' | 'owml' | 'lovely' | 'returnofmodding';
    mods: ProfileSyncInspectionMod[];
    unresolvedKeys: string[];
}

export const getProfileModKey = (fullName: string) => (
    parsePackageReference(fullName).packageName.toLowerCase()
);

export const snapshotInstalledMod = (mod: InstalledMod): InstalledModSnapshot => {
    const snapshot = { ...mod } as InstalledMod;
    delete snapshot.pending_sync;
    delete snapshot.synced_enabled;
    delete snapshot.pending_sync_kind;
    delete snapshot.pending_sync_status;
    delete snapshot.pending_sync_error;
    delete snapshot.sync_baseline;
    return snapshot;
};

export const restoreInstalledMod = (snapshot: InstalledModSnapshot): InstalledMod => ({
    ...snapshot,
    pending_sync: false,
    synced_enabled: snapshot.enabled,
    pending_sync_status: undefined,
    pending_sync_error: undefined,
});

export const inferPendingSyncKind = (
    mod: InstalledMod,
    baseline: InstalledModSnapshot | null | undefined = mod.sync_baseline,
): PendingSyncKind => {
    if (baseline === null || baseline === undefined) return mod.pending_sync_kind || 'add';
    if (baseline.versionNumber !== mod.versionNumber || baseline.fullName !== mod.fullName) return 'update';
    return mod.enabled ? 'enable' : 'disable';
};

export const hasPendingProfileSync = (profile: Profile | null | undefined) => !!profile && (
    !!profile.needs_sync
    || !!profile.apply_interrupted
    || profile.mods.some(mod => mod.pending_sync)
    || (profile.pending_removals?.length ?? 0) > 0
);

export const hasPendingRuntimeInstall = (
    profile: Profile | null | undefined,
    runtime: RuntimeHealth['runtime'] | undefined,
) => {
    if (!profile || !runtime) return false;
    return profile.mods.some(mod => {
        if (!mod.pending_sync || !mod.enabled || mod.pending_sync_kind === 'disable') return false;
        const packageName = parsePackageReference(mod.fullName).packageName.toLowerCase();
        if (runtime === 'bepinex') return packageName.includes('bepinexpack');
        if (runtime === 'owml') return packageName === 'owml' || packageName.endsWith('-owml');
        if (runtime === 'lovely') return packageName === 'lovely' || packageName.includes('thunderstore-lovely');
        return packageName === 'returnofmodding' || packageName.endsWith('-returnofmodding');
    });
};

export const migratePendingSyncBaselines = (
    profile: Profile,
    inspection: ProfileSyncInspection,
): Pick<Profile, 'mods' | 'pending_removals' | 'needs_sync'> => {
    const inspectedByKey = new Map(inspection.mods.map(mod => [mod.packageKey.toLowerCase(), mod]));
    const desiredKeys = new Set(profile.mods.map(mod => getProfileModKey(mod.fullName)));

    const mods = profile.mods.map(mod => {
        if (!mod.pending_sync || mod.sync_baseline !== undefined) return mod;
        const inspected = inspectedByKey.get(getProfileModKey(mod.fullName));
        if (!inspected) {
            return { ...mod, sync_baseline: null, pending_sync_kind: 'add' as const, pending_sync_status: mod.pending_sync_status || 'queued' as const };
        }

        const baseline: InstalledModSnapshot = {
            ...snapshotInstalledMod(mod),
            fullName: inspected.fullName,
            versionNumber: inspected.versionNumber || mod.versionNumber,
            enabled: inspected.enabled ?? mod.synced_enabled ?? mod.enabled,
        };
        return {
            ...mod,
            sync_baseline: baseline,
            pending_sync_kind: inferPendingSyncKind(mod, baseline),
            pending_sync_status: mod.pending_sync_status || 'queued',
        };
    });

    const existingRemovalKeys = new Set((profile.pending_removals || []).map(removal => getProfileModKey(removal.mod.fullName)));
    const discoveredRemovals = inspection.mods
        .filter(mod => !desiredKeys.has(mod.packageKey.toLowerCase()) && !existingRemovalKeys.has(mod.packageKey.toLowerCase()))
        .map(mod => ({
            id: `remove:${mod.packageKey.toLowerCase()}`,
            mod: {
                uuid4: `synced:${mod.packageKey.toLowerCase()}`,
                fullName: mod.fullName,
                versionNumber: mod.versionNumber || 'unknown',
                enabled: mod.enabled ?? true,
            } as InstalledModSnapshot,
        }));

    const pendingRemovals = [...(profile.pending_removals || []), ...discoveredRemovals];
    return {
        mods,
        pending_removals: pendingRemovals,
        needs_sync: mods.some(mod => mod.pending_sync) || pendingRemovals.length > 0 || !!profile.apply_interrupted,
    };
};
