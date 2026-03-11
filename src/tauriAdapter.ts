import { invoke } from '@tauri-apps/api/core';
import type { IElectronAPI } from './types/electron';
import type { Profile } from './types/profile';
import type { Community, Package, CommunityPlatformInfo } from './types/thunderstore';

export const tauriAPI: IElectronAPI = {
    getProfiles: () => invoke<Profile[]>('get_profiles'),
    saveProfiles: (profiles) => invoke('save_profiles', { profiles }),

    // Placeholder implementations for now
    selectFolder: async () => invoke<string | null>('select_folder'),
    selectFile: async (filters) => invoke<string | null>('select_file', { filters }),
    installMod: async (profileId, downloadUrl, modName, gamePath, useProfileCache) => {
        try {
            await invoke('install_mod', { profileId, downloadUrl, modName, gamePath, useProfileCache: useProfileCache ?? false });
            return { success: true };
        } catch (e) {
            return { success: false, error: String(e) };
        }
    },
    checkDirectoryExists: async (path) => invoke<boolean>('check_directory_exists', { path }),
    fetchCommunities: () => invoke<Community[]>('fetch_communities'),
    fetchCommunityImages: () => invoke<Record<string, string>>('fetch_community_images'),
    resolveCommunityPlatforms: (games: { identifier: string; name: string }[]) =>
        invoke<Record<string, CommunityPlatformInfo>>('resolve_community_platforms', { games }),
    async fetchPackages(gameId: string) {
        return await invoke('fetch_packages', { gameId });
    },
    async getAvailableCategories(gameId: string): Promise<string[]> {
        return await invoke('get_available_categories', { gameId });
    },
    async getPackages(
        gameId: string,
        page: number,
        pageSize: number,
        search: string,
        sort?: string,
        nsfw?: boolean,
        deprecated?: boolean,
        sortDirection?: string,
        categories?: string[],
        mods?: boolean,
        modpacks?: boolean
    ) {
        return await invoke('get_packages', {
            gameId,
            page,
            pageSize,
            search,
            sort,
            nsfw,
            deprecated,
            sortDirection,
            categories,
            mods,
            modpacks
        });
    },
    async lookupPackagesByNames(gameId: string, names: string[]) {
        return await invoke('lookup_packages_by_names', { gameId, names });
    },
    fetchPackageByName: async (name: string, gameId?: string | null) => invoke<Package | null>('fetch_package_by_name', { name, gameId }),
    importProfile: async (code) => invoke<any>('import_profile', { code }),
    importProfileFromFile: async (path) => invoke<any>('import_profile_from_file', { path }),
    shareProfile: async (profileId) => invoke<string>('share_profile', { profileId }),
    openModFolder: async (profileId, modName, gameIdentifier, platform?) =>
        invoke('open_mod_folder', { profileId, modName, gameIdentifier, platform }),
    exportProfile: async (profileId) => {
        try {
            return await invoke<any>('export_profile', { profileId });
        } catch (e) {
            console.error("Export failed", e);
            throw e;
        }
    },
    deleteProfileFolder: async (profileId, gameIdentifier?, platform?) => invoke<boolean>('delete_profile_folder', { profileId, gameIdentifier, platform }),
    getSettings: async () => invoke('get_settings'),
    saveSettings: async (settings) => invoke('save_settings', { settings }),
    getGamePath: async (gameIdentifier, platform?) => invoke('get_game_path', { gameIdentifier, platform }),
    getGameSource: async (gameIdentifier, platform?) => invoke<'steam' | 'manual' | 'unknown'>('get_game_source', { gameIdentifier, platform }),
    setGamePath: async (gameIdentifier, path, platform?) => invoke('set_game_path', { gameIdentifier, path, platform }),
    openGameFolder: async (gameIdentifier, platform?) => invoke('open_game_folder', { gameIdentifier, platform }),
    removeMod: async (profileId: string, modName: string) => {
        await invoke('remove_mod', { profileId, modName });
    },
    toggleMod: async (profileId: string, modName: string, enabled: boolean, gameIdentifier?: string, platform?: 'windows' | 'mac') => {
        await invoke('toggle_mod', { profileId, modName, enabled, gameIdentifier, platform });
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
    installToGame: async (gameIdentifier: string, profileId: string, disabledMods: string[], isVanillaOverride?: boolean) => {
        console.log('Installing profile to game:', { gameIdentifier, profileId, disabledMods, isVanillaOverride });
        return await invoke('install_to_game', { gameIdentifier, profileId, disabledMods, isVanillaOverride });
    },
    fetchTextContent: async (url: string) => {
        return await invoke<string>('fetch_text_content', { url });
    },
    checkUpdate: async (currentVersion: string) => {
        return await invoke('check_update', { currentVersion });
    },
    installUpdate: async (downloadUrl: string) => {
        return await invoke('install_update', { downloadUrl });
    },
    syncProfileToGame: async (profileId: string, gameIdentifier: string, useLegacyCache?: boolean) => {
        return await invoke<{ removed: number; to_install: string[]; already_installed: number; cached: number }>('sync_profile_to_game', { profileId, gameIdentifier, useLegacyCache: useLegacyCache ?? false });
    },
    copyModFromCache: async (profileId: string, modName: string, gamePath: string) => {
        return await invoke<{ success: boolean; copied: boolean }>('copy_mod_from_cache', { profileId, modName, gamePath });
    },
    clearProfileCache: async () => {
        return await invoke<{ cleared: number; chunks_cleared?: number; bytes_freed: number }>('clear_profile_cache', {});
    },
    openProfileFolder: async (profileId: string) => {
        return await invoke('open_profile_folder', { profileId });
    },
    launchGameWithMods: async (gameIdentifier: string, platform?: 'windows' | 'mac') => {
        return await invoke('launch_game_with_mods', { gameIdentifier, platform });
    }
};
