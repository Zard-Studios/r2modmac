import { create } from 'zustand';
import type {
    Profile,
    InstalledMod,
    ProfileDistribution,
    ProfileLaunchMode,
    ProfilePlatform
} from '../types/profile';
import { getProfileModKey, inferPendingSyncKind, restoreInstalledMod, snapshotInstalledMod } from '../utils/profileSync';

// Debounced save to prevent rapid-fire file writes causing race conditions
let saveTimeout: ReturnType<typeof setTimeout> | null = null;
const debouncedSaveProfiles = (profiles: Profile[]) => {
    if (saveTimeout) clearTimeout(saveTimeout);
    saveTimeout = setTimeout(() => {
        window.ipcRenderer.saveProfiles(profiles);
    }, 100); // 100ms debounce window
};

const getModKey = (fullName: string): string => {
    const parts = fullName.split('-');
    if (parts.length >= 2) {
        return `${parts[0]}-${parts[1]}`.toLowerCase();
    }
    return fullName.toLowerCase();
};

const normalizeLocalKeyPart = (value?: string | null): string => (
    value
        ?.trim()
        .toLowerCase()
        .replace(/[^a-z0-9]+/g, '_')
        .replace(/^_+|_+$/g, '')
        || ''
);

const getLocalModDisplayKey = (mod: InstalledMod): string => {
    const parts = mod.fullName.split('-');
    const fallbackName = parts.length >= 2 ? parts[1] : mod.fullName;
    const author = normalizeLocalKeyPart(mod.author || (parts.length >= 2 ? parts[0] : 'local')) || 'local';
    const name = normalizeLocalKeyPart(mod.displayName || fallbackName);
    if (name) return `${author}:${name}`;
    if (mod.sha256) return `sha:${mod.sha256.toLowerCase()}`;
    return normalizeLocalKeyPart(mod.fullName) || mod.uuid4;
};

const getInstalledModKey = (mod: InstalledMod): string => {
    if (mod.source === 'local') {
        return `local:${getLocalModDisplayKey(mod)}`;
    }
    return getModKey(mod.fullName);
};

const ensureModIdentity = (mod: InstalledMod): InstalledMod => {
    if (mod.uuid4?.trim()) return mod;
    const identity = mod.source === 'local' && mod.localId
        ? `local:${mod.localId}`
        : `thunderstore:${mod.fullName.toLowerCase()}`;
    return { ...mod, uuid4: identity };
};

const dedupeModsByKey = (mods: InstalledMod[]): InstalledMod[] => {
    const byKey = new Map<string, InstalledMod>();
    for (const mod of mods) {
        byKey.set(getInstalledModKey(mod), mod);
    }
    return Array.from(byKey.values());
};

const normalizeProfile = (profile: Profile): Profile => {
    const platform: ProfilePlatform = profile.platform === 'mac' ? 'mac' : 'windows';
    const distribution: ProfileDistribution = profile.distribution === 'manual' ? 'manual' : 'steam';
    const launchMode: ProfileLaunchMode = distribution === 'manual'
        ? 'direct'
        : (profile.launchMode === 'steam' || profile.launchMode === 'direct'
            ? profile.launchMode
            : 'auto');
    const dateCreated = typeof profile.dateCreated === 'number' ? profile.dateCreated : Date.now();
    const lastUsed = typeof profile.lastUsed === 'number' && profile.lastUsed > dateCreated
        ? profile.lastUsed
        : 0;

    return {
        ...profile,
        mods: dedupeModsByKey((profile.mods || []).map(ensureModIdentity)),
        pending_removals: profile.pending_removals || [],
        platform,
        distribution,
        launchMode,
        dateCreated,
        lastUsed,
    };
};

interface ProfileState {
    profiles: Profile[];
    activeProfileId: string | null;

    // Actions
    createProfile: (
        name: string,
        gameIdentifier: string,
        platform?: ProfilePlatform,
        distribution?: ProfileDistribution
    ) => string;
    selectProfile: (profileId: string) => void;
    deleteProfile: (profileId: string, gameIdentifier?: string) => Promise<void>;
    updateProfile: (profileId: string, updates: Partial<Profile>) => void;
    setProfiles: (profiles: Profile[]) => void;
    addMod: (profileId: string, mod: InstalledMod) => void;
    removeMod: (profileId: string, modId: string, stageOnly?: boolean) => Promise<void>;
    toggleMod: (profileId: string, modId: string, syncFiles?: boolean) => Promise<void>;
    revertPendingMods: (profileId: string, modIds: string[]) => void;
    revertAllPending: (profileId: string) => void;
    loadProfiles: () => Promise<void>;
}

export const useProfileStore = create<ProfileState>((set) => ({
    profiles: [],
    activeProfileId: null,

    createProfile: (name, gameIdentifier, platform, distribution) => {
        const newProfile = normalizeProfile({
            id: crypto.randomUUID(),
            name,
            gameIdentifier,
            platform: platform || 'windows',
            distribution: distribution === 'manual' ? 'manual' : 'steam',
            launchMode: distribution === 'manual' ? 'direct' : 'auto',
            mods: [],
            needs_sync: false,
            dateCreated: Date.now(),
            lastUsed: 0,
        });

        set((state) => {
            const updatedProfiles = [...state.profiles, newProfile];
            debouncedSaveProfiles(updatedProfiles);
            return {
                profiles: updatedProfiles,
                activeProfileId: newProfile.id
            };
        });

        return newProfile.id;
    },

    selectProfile: (profileId) => set({ activeProfileId: profileId }),

    deleteProfile: async (profileId, gameIdentifier?) => {
        const existingProfile = useProfileStore.getState().profiles.find((p) => p.id === profileId);
        // First delete from disk, THEN update state
        // This ensures if there's an error, we don't lose state
        try {
            await window.ipcRenderer.deleteProfileFolder(profileId, gameIdentifier, existingProfile?.platform);
        } catch (e) {
            console.error("Failed to delete profile folder:", e);
            // Continue anyway to clean up state
        }

        set((state) => {
            const updatedProfiles = state.profiles.filter(p => p.id !== profileId);
            debouncedSaveProfiles(updatedProfiles);

            return {
                profiles: updatedProfiles,
                activeProfileId: state.activeProfileId === profileId ? null : state.activeProfileId
            };
        });
    },

    updateProfile: (profileId, updates) => {
        set((state) => {
            const updatedProfiles = state.profiles.map(p =>
                p.id === profileId ? normalizeProfile({ ...p, ...updates }) : p
            );
            debouncedSaveProfiles(updatedProfiles);
            return { profiles: updatedProfiles };
        });
    },

    setProfiles: (profiles) => {
        const normalized = profiles.map((profile) => normalizeProfile(profile));
        set({ profiles: normalized });
        debouncedSaveProfiles(normalized);
    },

    addMod: (profileId, mod) => {
        const obsoleteLocalPayloads: string[] = [];
        set((state) => {
            const profileIndex = state.profiles.findIndex(p => p.id === profileId);
            if (profileIndex === -1) return state;

            const updatedProfiles = [...state.profiles];
            const profile = { ...updatedProfiles[profileIndex] };

            const normalizedMod: InstalledMod = ensureModIdentity({
                ...mod,
                pending_sync: mod.pending_sync ?? false,
                synced_enabled: mod.synced_enabled ?? (mod.pending_sync ? undefined : mod.enabled),
            });

            const incomingKey = getInstalledModKey(normalizedMod);
            const matchingMods = profile.mods.filter((m) =>
                (!!m.uuid4 && !!normalizedMod.uuid4 && m.uuid4 === normalizedMod.uuid4)
                || getInstalledModKey(m) === incomingKey
            );
            const existing = matchingMods[matchingMods.length - 1];

            if (normalizedMod.source === 'local') {
                obsoleteLocalPayloads.push(
                    ...matchingMods
                        .map((m) => m.localId)
                        .filter((localId): localId is string => !!localId && localId !== normalizedMod.localId)
                );
            }

            const merged: InstalledMod = existing ? { ...existing, ...normalizedMod } : normalizedMod;
            if (merged.pending_sync) {
                merged.sync_baseline = existing?.pending_sync
                    ? existing.sync_baseline
                    : existing
                        ? snapshotInstalledMod(existing)
                        : null;
                merged.pending_sync_kind = inferPendingSyncKind(merged, merged.sync_baseline);
                merged.pending_sync_status = merged.pending_sync_status || 'queued';
                merged.pending_sync_error = undefined;
            }
            profile.mods = [
                ...profile.mods.filter((m) => {
                    const sameUuid = !!m.uuid4 && !!normalizedMod.uuid4 && m.uuid4 === normalizedMod.uuid4;
                    return !sameUuid && getInstalledModKey(m) !== incomingKey;
                }),
                merged,
            ];
            profile.needs_sync = !!profile.needs_sync || profile.mods.some(m => m.pending_sync);
            updatedProfiles[profileIndex] = profile;
            debouncedSaveProfiles(updatedProfiles);

            return { profiles: updatedProfiles };
        });
        for (const localId of obsoleteLocalPayloads) {
            void window.ipcRenderer.deleteLocalModPayload(profileId, localId).catch((err) => {
                console.error("Failed to delete replaced custom mod payload:", err);
            });
        }
    },

    removeMod: async (profileId, modId, stageOnly = true) => {
        // First get the mod info and delete files, THEN update state
        const state = useProfileStore.getState();
        const profileIndex = state.profiles.findIndex(p => p.id === profileId);
        if (profileIndex === -1) return;

        const profile = state.profiles[profileIndex];
        const mod = profile.mods.find(m => m.uuid4 === modId);

        const wasSynced = mod ? mod.synced_enabled !== undefined || mod.sync_baseline !== null : false;

        // Default mode stages a reversible removal. Legacy callers remove files
        // immediately and therefore never have pending_sync metadata.
        if (stageOnly && mod?.pending_sync && mod.sync_baseline === null) {
            set((currentState) => {
                const updatedProfiles = currentState.profiles.map(candidate => candidate.id === profileId
                    ? {
                        ...candidate,
                        mods: candidate.mods.filter(item => item.uuid4 !== modId),
                        needs_sync: candidate.mods.some(item => item.uuid4 !== modId && item.pending_sync)
                            || (candidate.pending_removals?.length ?? 0) > 0,
                    }
                    : candidate);
                debouncedSaveProfiles(updatedProfiles);
                return { profiles: updatedProfiles };
            });
            return;
        }

        if (stageOnly && mod && wasSynced) {
            const baseline = mod.sync_baseline || snapshotInstalledMod(mod);
            set((currentState) => {
                const updatedProfiles = currentState.profiles.map(candidate => candidate.id === profileId
                    ? {
                        ...candidate,
                        mods: candidate.mods.filter(item => item.uuid4 !== modId),
                        pending_removals: [
                            ...(candidate.pending_removals || []).filter(removal => getProfileModKey(removal.mod.fullName) !== getProfileModKey(baseline.fullName)),
                            { id: `remove:${getProfileModKey(baseline.fullName)}`, mod: baseline, sync_status: 'queued' as const },
                        ],
                        needs_sync: true,
                    }
                    : candidate);
                debouncedSaveProfiles(updatedProfiles);
                return { profiles: updatedProfiles };
            });
            return;
        }

        if (mod) {
            try {
                const modName = mod.fullName.split('-').slice(0, 2).join('-');
                await window.ipcRenderer.removeMod(profileId, modName);
                if (mod.source === 'local' && mod.localId) {
                    await window.ipcRenderer.deleteLocalModPayload(profileId, mod.localId);
                }
            } catch (e) {
                console.error("Failed to remove mod files:", e);
                // Continue to update state anyway
            }
        }

        set((state) => {
            const profileIndex = state.profiles.findIndex(p => p.id === profileId);
            if (profileIndex === -1) return state;

            const updatedProfiles = [...state.profiles];
            const profile = { ...updatedProfiles[profileIndex] };

            profile.mods = profile.mods.filter(m => m.uuid4 !== modId);
            
            // Check if there are other mods in the profile that still need sync
            const otherModsNeedSync = profile.mods.some(m => m.pending_sync);
            
            // Only set needs_sync to true if the deleted mod was previously synced, 
            // or if other remaining mods in the profile need to be synced.
            profile.needs_sync = wasSynced || otherModsNeedSync;

            updatedProfiles[profileIndex] = profile;
            debouncedSaveProfiles(updatedProfiles);

            return { profiles: updatedProfiles };
        });
    },

    toggleMod: async (profileId, modId, syncFiles = true) => {
        // Get current state to find the mod
        const state = useProfileStore.getState();
        const profile = state.profiles.find(p => p.id === profileId);
        if (!profile) return;

        const mod = profile.mods.find(m => m.uuid4 === modId);
        if (!mod) return;

        const newEnabled = !mod.enabled;

        // Use "Author-ModName" for reliable backend matching.
        const parts = mod.fullName.split('-');
        const modName = parts.length >= 2 ? `${parts[0]}-${parts[1]}` : mod.fullName;

        try {
            if (syncFiles) {
                // Sync profile change to filesystem/game only when explicitly requested
                await window.ipcRenderer.toggleMod(profileId, modName, newEnabled, profile.gameIdentifier, profile.platform);
            }

            // Update store state after successful operation
            set((state) => {
                const profileIndex = state.profiles.findIndex(p => p.id === profileId);
                if (profileIndex === -1) return state;

                const updatedProfiles = [...state.profiles];
                const profile = { ...updatedProfiles[profileIndex] };

                profile.mods = profile.mods.map(m => {
                    if (m.uuid4 === modId) {
                        const pendingSync =
                            syncFiles
                                ? false
                                : (m.synced_enabled === undefined
                                    ? true
                                    : newEnabled !== m.synced_enabled);
                        return {
                            ...m,
                            enabled: newEnabled,
                            pending_sync: pendingSync,
                            synced_enabled: syncFiles ? newEnabled : m.synced_enabled,
                            sync_baseline: syncFiles
                                ? undefined
                                : pendingSync
                                    ? (m.pending_sync ? m.sync_baseline : snapshotInstalledMod(m))
                                    : undefined,
                            pending_sync_kind: syncFiles || !pendingSync
                                ? undefined
                                : inferPendingSyncKind(
                                    { ...m, enabled: newEnabled },
                                    m.pending_sync ? m.sync_baseline : snapshotInstalledMod(m)
                                ),
                            pending_sync_status: syncFiles || !pendingSync ? undefined : 'queued',
                            pending_sync_error: undefined,
                        };
                    }
                    return m;
                });
                profile.needs_sync = profile.mods.some(m => m.pending_sync);

                updatedProfiles[profileIndex] = profile;
                debouncedSaveProfiles(updatedProfiles);

                return { profiles: updatedProfiles };
            });
        } catch (e) {
            console.error('Failed to toggle mod:', e);
        }
    },

    revertPendingMods: (profileId, modIds) => {
        const ids = new Set(modIds);
        set((state) => {
            const updatedProfiles = state.profiles.map(profile => {
                if (profile.id !== profileId) return profile;
                const restoredMods: InstalledMod[] = [];
                for (const mod of profile.mods) {
                    if (!ids.has(mod.uuid4) || !mod.pending_sync) {
                        restoredMods.push(mod);
                    } else if (mod.sync_baseline) {
                        restoredMods.push(restoreInstalledMod(mod.sync_baseline));
                    }
                }
                const selectedRemovalKeys = new Set(
                    (profile.pending_removals || [])
                        .filter(removal => ids.has(removal.id))
                        .map(removal => getProfileModKey(removal.mod.fullName))
                );
                const restoredRemovalMods = (profile.pending_removals || [])
                    .filter(removal => ids.has(removal.id))
                    .map(removal => restoreInstalledMod(removal.mod));
                const byKey = new Map<string, InstalledMod>();
                for (const mod of [...restoredMods, ...restoredRemovalMods]) byKey.set(getProfileModKey(mod.fullName), mod);
                const pendingRemovals = (profile.pending_removals || []).filter(removal => !selectedRemovalKeys.has(getProfileModKey(removal.mod.fullName)));
                const mods = Array.from(byKey.values());
                return {
                    ...profile,
                    mods,
                    pending_removals: pendingRemovals,
                    needs_sync: mods.some(mod => mod.pending_sync) || pendingRemovals.length > 0 || !!profile.apply_interrupted,
                };
            });
            debouncedSaveProfiles(updatedProfiles);
            return { profiles: updatedProfiles };
        });
    },

    revertAllPending: (profileId) => {
        const profile = useProfileStore.getState().profiles.find(candidate => candidate.id === profileId);
        if (!profile) return;
        const ids = [
            ...profile.mods.filter(mod => mod.pending_sync).map(mod => mod.uuid4),
            ...(profile.pending_removals || []).map(removal => removal.id),
        ];
        useProfileStore.getState().revertPendingMods(profileId, ids);
    },

    loadProfiles: async () => {
        const rawProfiles = await window.ipcRenderer.getProfiles();
        const profiles = rawProfiles.map((storedProfile) => {
            const profile = storedProfile.selective_sync_restore ? {
                ...storedProfile,
                mods: storedProfile.selective_sync_restore.mods,
                pending_removals: storedProfile.selective_sync_restore.pending_removals,
                needs_sync: true,
                apply_interrupted: true,
                selective_sync_restore: undefined,
            } : storedProfile;
            const normalizedMods = (profile.mods || []).map((mod) => {
                const pendingSync = !!mod.pending_sync;
                return {
                    ...mod,
                    pending_sync: pendingSync,
                    synced_enabled: mod.synced_enabled ?? (pendingSync ? undefined : mod.enabled),
                } as InstalledMod;
            });
            const mods = dedupeModsByKey(normalizedMods);
            return {
                ...profile,
                mods,
                needs_sync: !!profile.needs_sync
                    || mods.some((m: InstalledMod) => m.pending_sync)
                    || (profile.pending_removals?.length ?? 0) > 0,
            } as Profile;
        });
        set({ profiles: profiles.map((profile) => normalizeProfile(profile)) });
    }
}));
