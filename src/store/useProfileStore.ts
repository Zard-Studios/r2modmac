import { create } from 'zustand';
import type { Profile } from '../types/profile';

interface ProfileState {
    profiles: Profile[];
    activeProfileId: string | null;

    // Actions
    createProfile: (name: string, gameIdentifier: string) => void;
    selectProfile: (profileId: string) => void;
    deleteProfile: (profileId: string) => void;
    setProfiles: (profiles: Profile[]) => void;
    loadProfiles: () => Promise<void>;
}

export const useProfileStore = create<ProfileState>((set) => ({
    profiles: [],
    activeProfileId: null,

    createProfile: (name, gameIdentifier) => {
        set((state) => {
            const newProfile: Profile = {
                id: crypto.randomUUID(),
                name,
                gameIdentifier,
                mods: [],
                dateCreated: Date.now(),
                lastUsed: Date.now(),
            };
            const updatedProfiles = [...state.profiles, newProfile];
            window.ipcRenderer.saveProfiles(updatedProfiles);
            return {
                profiles: updatedProfiles,
                activeProfileId: newProfile.id
            };
        });
    },

    selectProfile: (profileId) => set({ activeProfileId: profileId }),

    deleteProfile: (profileId) => {
        set((state) => {
            const updatedProfiles = state.profiles.filter(p => p.id !== profileId);
            window.ipcRenderer.saveProfiles(updatedProfiles);
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

    loadProfiles: async () => {
        const profiles = await window.ipcRenderer.getProfiles();
        set({ profiles });
    }
}));
