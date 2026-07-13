import { create } from 'zustand';
import type {
    Profile,
    InstalledMod,
    ProfileDistribution,
    ProfileLaunchMode,
    ProfilePlatform
} from '../types/profile';

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
    removeMod: (profileId: string, modId: string) => Promise<void>;
    toggleMod: (profileId: string, modId: string, syncFiles?: boolean) => Promise<void>;
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

    removeMod: async (profileId, modId) => {
        // First get the mod info and delete files, THEN update state
        const state = useProfileStore.getState();
        const profileIndex = state.profiles.findIndex(p => p.id === profileId);
        if (profileIndex === -1) return;

        const profile = state.profiles[profileIndex];
        const mod = profile.mods.find(m => m.uuid4 === modId);

        const wasSynced = mod ? mod.synced_enabled !== undefined : false;

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

    loadProfiles: async () => {
        const rawProfiles = await window.ipcRenderer.getProfiles();
        const profiles = rawProfiles.map((profile) => {
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
                needs_sync: !!profile.needs_sync || mods.some((m: InstalledMod) => m.pending_sync),
            } as Profile;
        });
        set({ profiles: profiles.map((profile) => normalizeProfile(profile)) });
    }
}));
