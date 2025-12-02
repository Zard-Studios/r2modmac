import { invoke } from '@tauri-apps/api/core';
import type { IElectronAPI } from './types/electron';
import type { Profile } from './types/profile';
import type { Community, Package } from './types/thunderstore';

export const tauriAPI: IElectronAPI = {
    getProfiles: () => invoke<Profile[]>('get_profiles'),
    saveProfiles: (profiles) => invoke('save_profiles', { profiles }),

    // Placeholder implementations for now
    selectFolder: async () => invoke<string | null>('select_folder'),
    selectFile: async () => invoke<string | null>('select_file'),
    installMod: async (profileId, downloadUrl, modName) => {
        try {
            await invoke('install_mod', { profileId, downloadUrl, modName });
            return { success: true };
        } catch (e) {
            return { success: false, error: String(e) };
        }
    },
    checkDirectoryExists: async (path) => invoke<boolean>('check_directory_exists', { path }),
    fetchCommunities: () => invoke<Community[]>('fetch_communities'),
    fetchCommunityImages: () => invoke<Record<string, string>>('fetch_community_images'),
    async fetchPackages(gameId: string) {
        return await invoke('fetch_packages', { gameId });
    },
    async getPackages(gameId: string, page: number, pageSize: number, search: string) {
        return await invoke('get_packages', { gameId, page, pageSize, search });
    },
    fetchPackageByName: async (name: string, gameId?: string | null) => invoke<Package | null>('fetch_package_by_name', { name, gameId }),
    importProfile: async (code) => invoke<any>('import_profile', { code }),
    importProfileFromFile: async (path) => invoke<any>('import_profile_from_file', { path }),
    shareProfile: async (profileId) => invoke<string>('share_profile', { profileId }),
    openModFolder: async (profileId, modName) => invoke('open_mod_folder', { profileId, modName }),
    exportProfile: async (profileId) => {
        try {
            return await invoke<any>('export_profile', { profileId });
        } catch (e) {
            console.warn('Export failed:', e);
            return { success: false, error: e };
        }
    },
    deleteProfileFolder: async (profileId) => invoke<boolean>('delete_profile_folder', { profileId }),
    removeMod: async (profileId: string, modName: string) => {
        await invoke('remove_mod', { profileId, modName });
    },
    confirm: async (title: string, message: string) => {
        return await invoke('confirm_dialog', { title, message });
    }
};
