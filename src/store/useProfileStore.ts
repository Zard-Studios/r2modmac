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

interface ProfileState {
    profiles: Profile[];
    activeProfileId: string | null;

    // Actions
    createProfile: (name: string, gameIdentifier: string) => string;
    selectProfile: (profileId: string) => void;
    deleteProfile: (profileId: string, gameIdentifier?: string) => Promise<void>;
    updateProfile: (profileId: string, updates: Partial<Profile>) => void;
    setProfiles: (profiles: Profile[]) => void;
    addMod: (profileId: string, mod: InstalledMod) => void;
    removeMod: (profileId: string, modId: string) => Promise<void>;
    toggleMod: (profileId: string, modId: string) => Promise<void>;
    loadProfiles: () => Promise<void>;
}

export const useProfileStore = create<ProfileState>((set) => ({
    profiles: [],
    activeProfileId: null,

    createProfile: (name, gameIdentifier) => {
        const newProfile: Profile = {
            id: crypto.randomUUID(),
            name,
            gameIdentifier,
            mods: [],
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
        // First delete from disk, THEN update state
        // This ensures if there's an error, we don't lose state
        try {
            await window.ipcRenderer.deleteProfileFolder(profileId, gameIdentifier);
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

            // Check if mod is already installed
            if (!profile.mods.some(m => m.uuid4 === mod.uuid4)) {
                profile.mods = [...profile.mods, mod];
                updatedProfiles[profileIndex] = profile;
                debouncedSaveProfiles(updatedProfiles);
            }

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
            updatedProfiles[profileIndex] = profile;
            debouncedSaveProfiles(updatedProfiles);

            return { profiles: updatedProfiles };
        });
    },

    toggleMod: async (profileId, modId) => {
        // Get current state to find the mod
        const state = useProfileStore.getState();
        const profile = state.profiles.find(p => p.id === profileId);
        if (!profile) return;

        const mod = profile.mods.find(m => m.uuid4 === modId);
        if (!mod) return;

        const newEnabled = !mod.enabled;

        // Extract mod name from fullName (format: "Author-ModName-Version")
        const parts = mod.fullName.split('-');
        const modName = parts.length >= 2 ? parts[1] : mod.fullName;

        try {
            // Call backend to actually rename the DLL files and sync to game if applicable
            await window.ipcRenderer.toggleMod(profileId, modName, newEnabled, profile.gameIdentifier);

            // Update store state after successful backend call
            set((state) => {
                const profileIndex = state.profiles.findIndex(p => p.id === profileId);
                if (profileIndex === -1) return state;

                const updatedProfiles = [...state.profiles];
                const profile = { ...updatedProfiles[profileIndex] };

                profile.mods = profile.mods.map(m => {
                    if (m.uuid4 === modId) {
                        return { ...m, enabled: newEnabled };
                    }
                    return m;
                });

                updatedProfiles[profileIndex] = profile;
                debouncedSaveProfiles(updatedProfiles);

                return { profiles: updatedProfiles };
            });
        } catch (e) {
            console.error('Failed to toggle mod:', e);
        }
    },

    loadProfiles: async () => {
        const profiles = await window.ipcRenderer.getProfiles();
        set({ profiles });
    }
}));
