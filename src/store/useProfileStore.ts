import { create } from 'zustand';
import type { Profile, InstalledMod } from '../types/profile';

interface ProfileState {
    profiles: Profile[];
    activeProfileId: string | null;

    // Actions
    createProfile: (name: string, gameIdentifier: string) => string;
    selectProfile: (profileId: string) => void;
    deleteProfile: (profileId: string) => void;
    setProfiles: (profiles: Profile[]) => void;
    addMod: (profileId: string, mod: InstalledMod) => void;
    removeMod: (profileId: string, modId: string) => void;
    toggleMod: (profileId: string, modId: string) => void;
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
            window.ipcRenderer.saveProfiles(updatedProfiles);
            return {
                profiles: updatedProfiles,
                activeProfileId: newProfile.id
            };
        });

        return newProfile.id;
    },

    selectProfile: (profileId) => set({ activeProfileId: profileId }),

    deleteProfile: (profileId) => {
        set((state) => {
            const updatedProfiles = state.profiles.filter(p => p.id !== profileId);
            window.ipcRenderer.saveProfiles(updatedProfiles);

            // Delete the profile folder from disk
            window.ipcRenderer.deleteProfileFolder(profileId);

            return {
                profiles: updatedProfiles,
                activeProfileId: state.activeProfileId === profileId ? null : state.activeProfileId
            };
        });
    },

    setProfiles: (profiles) => {
        set({ profiles });
        window.ipcRenderer.saveProfiles(profiles);
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
                window.ipcRenderer.saveProfiles(updatedProfiles);
            }

            return { profiles: updatedProfiles };
        });
    },

    removeMod: (profileId, modId) => {
        set((state) => {
            const profileIndex = state.profiles.findIndex(p => p.id === profileId);
            if (profileIndex === -1) return state;

            const updatedProfiles = [...state.profiles];
            const profile = { ...updatedProfiles[profileIndex] };

            profile.mods = profile.mods.filter(m => m.uuid4 !== modId);
            updatedProfiles[profileIndex] = profile;
            window.ipcRenderer.saveProfiles(updatedProfiles);

            return { profiles: updatedProfiles };
        });
    },

    toggleMod: (profileId, modId) => {
        set((state) => {
            const profileIndex = state.profiles.findIndex(p => p.id === profileId);
            if (profileIndex === -1) return state;

            const updatedProfiles = [...state.profiles];
            const profile = { ...updatedProfiles[profileIndex] };

            // Create a new array for mods to ensure immutability
            profile.mods = profile.mods.map(m => {
                if (m.uuid4 === modId) {
                    return { ...m, enabled: !m.enabled };
                }
                return m;
            });

            updatedProfiles[profileIndex] = profile;
            window.ipcRenderer.saveProfiles(updatedProfiles);

            return { profiles: updatedProfiles };
        });
    },

    loadProfiles: async () => {
        const profiles = await window.ipcRenderer.getProfiles();
        set({ profiles });
    }
}));
