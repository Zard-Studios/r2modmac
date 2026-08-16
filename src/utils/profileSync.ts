import type {
    InstalledMod,
    InstalledModSnapshot,
    PendingSyncKind,
    Profile,
} from '../types/profile.ts';
import { parsePackageReference } from './modVersioning.ts';
import { isLoaderPackage } from './loaderPackages.ts';
import type { RuntimeHealth } from '../types/electron';

export interface ProfileSyncInspectionMod {
    packageKey: string;
    fullName: string;
    versionNumber?: string;
    enabled?: boolean;
}

export interface ProfileSyncInspection {
    status: 'ready' | 'unconfigured' | 'unsupported';
    runtime: RuntimeHealth['runtime'];
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
        return isLoaderPackage(runtime, mod.fullName);
    });
};

export const migratePendingSyncBaselines = (
    profile: Profile,
    inspection: ProfileSyncInspection,
): Pick<Profile, 'mods' | 'pending_removals' | 'needs_sync'> => {
    const inspectedByKey = new Map(inspection.mods.map(mod => [mod.packageKey.toLowerCase(), mod]));
    const desiredKeys = new Set(profile.mods.map(mod => getProfileModKey(mod.fullName)));

    const mods = profile.mods.map(mod => {
        const inspected = inspectedByKey.get(getProfileModKey(mod.fullName));

        // `needs_sync` can also mean that the game directory changed outside
        // the normal staging flow (for example, deleting another profile
        // removes the shared runtime and its mods). In that case there are no
        // per-mod flags yet: build them from the inspection so the Sync tab
        // reflects the real difference instead of showing an empty banner.
        if (!mod.pending_sync) {
            if (!profile.needs_sync) return mod;
            if (!inspected) {
                if (!mod.enabled) return mod;
                return {
                    ...mod,
                    pending_sync: true,
                    sync_baseline: null,
                    pending_sync_kind: 'add' as const,
                    pending_sync_status: 'queued' as const,
                    pending_sync_error: undefined,
                };
            }

            const baseline: InstalledModSnapshot = {
                ...snapshotInstalledMod(mod),
                fullName: inspected.fullName,
                versionNumber: inspected.versionNumber || mod.versionNumber,
                enabled: inspected.enabled ?? mod.synced_enabled ?? mod.enabled,
            };
            const versionChanged = baseline.fullName !== mod.fullName
                || baseline.versionNumber !== mod.versionNumber;
            const enabledChanged = baseline.enabled !== mod.enabled;
            if (!versionChanged && !enabledChanged) return mod;

            return {
                ...mod,
                pending_sync: true,
                sync_baseline: baseline,
                pending_sync_kind: inferPendingSyncKind(mod, baseline),
                pending_sync_status: 'queued' as const,
                pending_sync_error: undefined,
            };
        }

        if (mod.sync_baseline !== undefined) return mod;
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
