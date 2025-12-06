import { invoke } from '@tauri-apps/api/core';
import type { IElectronAPI } from './types/electron';
import type { Profile } from './types/profile';
import type { Community, Package } from './types/thunderstore';

export const tauriAPI: IElectronAPI = {
    getProfiles: () => invoke<Profile[]>('get_profiles'),
    saveProfiles: (profiles) => invoke('save_profiles', { profiles }),

    // Placeholder implementations for now
    selectFolder: async () => invoke<string | null>('select_folder'),
    selectFile: async (filters) => invoke<string | null>('select_file', { filters }),
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
    async getPackages(gameId: string, page: number, pageSize: number, search: string, sort?: string) {
        return await invoke('get_packages', { gameId, page, pageSize, search, sort });
    },
    async lookupPackagesByNames(gameId: string, names: string[]) {
        return await invoke('lookup_packages_by_names', { gameId, names });
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
            console.error("Export failed", e);
            throw e;
        }
    },
    deleteProfileFolder: async (profileId, gameIdentifier?) => invoke<boolean>('delete_profile_folder', { profileId, gameIdentifier }),
    getSettings: async () => invoke('get_settings'),
    saveSettings: async (settings) => invoke('save_settings', { settings }),
    getGamePath: async (gameIdentifier) => invoke('get_game_path', { gameIdentifier }),
    setGamePath: async (gameIdentifier, path) => invoke('set_game_path', { gameIdentifier, path }),
    openGameFolder: async (gameIdentifier) => invoke('open_game_folder', { gameIdentifier }),
    removeMod: async (profileId: string, modName: string) => {
        await invoke('remove_mod', { profileId, modName });
    },
    toggleMod: async (profileId: string, modName: string, enabled: boolean, gameIdentifier?: string) => {
        await invoke('toggle_mod', { profileId, modName, enabled, gameIdentifier });
    },
    confirm: async (title: string, message: string) => {
        return await invoke('confirm_dialog', { title, message });
    },
    alert: async (title: string, message: string) => {
        return await invoke('alert_dialog', { title, message });
    },
    readImage: async (path: string) => {
        return await invoke('read_image', { path });
    },
    installToGame: async (gameIdentifier: string, profileId: string, disabledMods: string[]) => {
        console.log('Installing profile to game:', { gameIdentifier, profileId, disabledMods });
        return await invoke('install_to_game', { gameIdentifier, profileId, disabledMods });
    },
    fetchTextContent: async (url: string) => {
        return await invoke<string>('fetch_text_content', { url });
    },
    checkUpdate: async (currentVersion: string) => {
        return await invoke('check_update', { currentVersion });
    },
    installUpdate: async (downloadUrl: string) => {
        return await invoke('install_update', { downloadUrl });
    }
};
