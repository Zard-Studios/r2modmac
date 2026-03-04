import { create } from 'zustand';
import type { Profile, InstalledMod } from '../types/profile';

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

const dedupeModsByKey = (mods: InstalledMod[]): InstalledMod[] => {
    const byKey = new Map<string, InstalledMod>();
    for (const mod of mods) {
        byKey.set(getModKey(mod.fullName), mod);
    }
    return Array.from(byKey.values());
};

interface ProfileState {
    profiles: Profile[];
    activeProfileId: string | null;

    // Actions
    createProfile: (name: string, gameIdentifier: string, platform?: 'windows' | 'mac') => string;
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

    createProfile: (name, gameIdentifier, platform) => {
        const newProfile: Profile = {
            id: crypto.randomUUID(),
            name,
            gameIdentifier,
            platform: platform || 'windows',
            mods: [],
            needs_sync: false,
            dateCreated: Date.now(),
            lastUsed: Date.now(),
        };

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
                p.id === profileId ? { ...p, ...updates } : p
            );
            debouncedSaveProfiles(updatedProfiles);
            return { profiles: updatedProfiles };
        });
    },

    setProfiles: (profiles) => {
        set({ profiles });
        debouncedSaveProfiles(profiles);
    },

    addMod: (profileId, mod) => {
        set((state) => {
            const profileIndex = state.profiles.findIndex(p => p.id === profileId);
            if (profileIndex === -1) return state;

            const updatedProfiles = [...state.profiles];
            const profile = { ...updatedProfiles[profileIndex] };

            const normalizedMod: InstalledMod = {
                ...mod,
                pending_sync: mod.pending_sync ?? false,
                synced_enabled: mod.synced_enabled ?? (mod.pending_sync ? undefined : mod.enabled),
            };

            const incomingKey = getModKey(normalizedMod.fullName);
            const existingIndex = profile.mods.findIndex((m) =>
                m.uuid4 === normalizedMod.uuid4 || getModKey(m.fullName) === incomingKey
            );

            if (existingIndex >= 0) {
                const existing = profile.mods[existingIndex];
                const merged: InstalledMod = {
                    ...existing,
                    ...normalizedMod,
                };
                profile.mods = profile.mods.map((m, idx) => (idx === existingIndex ? merged : m));
            } else {
                profile.mods = [...profile.mods, normalizedMod];
            }

            profile.mods = dedupeModsByKey(profile.mods);
            profile.needs_sync = !!profile.needs_sync || profile.mods.some(m => m.pending_sync);
            updatedProfiles[profileIndex] = profile;
            debouncedSaveProfiles(updatedProfiles);

            return { profiles: updatedProfiles };
        });
    },

    removeMod: async (profileId, modId) => {
        // First get the mod info and delete files, THEN update state
        const state = useProfileStore.getState();
        const profileIndex = state.profiles.findIndex(p => p.id === profileId);
        if (profileIndex === -1) return;

        const profile = state.profiles[profileIndex];
        const mod = profile.mods.find(m => m.uuid4 === modId);

        if (mod) {
            try {
                const modName = mod.fullName.split('-').slice(0, 2).join('-');
                await window.ipcRenderer.removeMod(profileId, modName);
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
            // Removal cannot be represented by per-mod pending markers after deletion,
            // so keep profile-level pending sync true.
            profile.needs_sync = true;
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
            };
        });
        set({ profiles });
    }
}));
