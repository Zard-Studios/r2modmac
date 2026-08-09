import { invoke } from '@tauri-apps/api/core';
import type { IElectronAPI, ThemeSummary } from './types/electron';
import type { Profile } from './types/profile';
import type { Community, Package, CommunityPlatformInfo } from './types/thunderstore';

const PACKAGE_FETCH_DEDUP_WINDOW_MS = 1_000;
const packageFetchesInFlight = new Map<string, Promise<number>>();
const recentPackageFetches = new Map<string, { result: number; completedAt: number }>();

function fetchPackagesDeduped(gameId: string): Promise<number> {
    const key = gameId.trim().toLowerCase();
    const inFlight = packageFetchesInFlight.get(key);
    if (inFlight) return inFlight;

    const now = Date.now();
    for (const [cachedKey, cached] of recentPackageFetches) {
        if (now - cached.completedAt >= PACKAGE_FETCH_DEDUP_WINDOW_MS) {
            recentPackageFetches.delete(cachedKey);
        }
    }

    const recent = recentPackageFetches.get(key);
    if (recent) {
        return Promise.resolve(recent.result);
    }

    const request = invoke<number>('fetch_packages', { gameId })
        .then((result) => {
            recentPackageFetches.clear();
            recentPackageFetches.set(key, {
                result,
                completedAt: Date.now(),
            });
            return result;
        })
        .finally(() => {
            if (packageFetchesInFlight.get(key) === request) {
                packageFetchesInFlight.delete(key);
            }
        });

    packageFetchesInFlight.set(key, request);
    return request;
}

export const tauriAPI: IElectronAPI = {
    getProfiles: () => invoke<Profile[]>('get_profiles'),
    saveProfiles: (profiles) => invoke('save_profiles', { profiles }),

    // Placeholder implementations for now
    selectFolder: async () => invoke<string | null>('select_folder'),
    getUsername: async () => invoke<string>('get_username'),
    selectFile: async (filters) => invoke<string | null>('select_file', { filters }),
    selectImportPath: async () => invoke<string | null>('select_import_path'),
    installMod: async (profileId, downloadUrl, modName, gamePath, useProfileCache) => {
        try {
            const res = await invoke<any>('install_mod', { profileId, downloadUrl, modName, gamePath, useProfileCache: useProfileCache ?? false });
            return res;
        } catch (e) {
            return { success: false, error: String(e) };
        }
    },
    beginModOperations: async () => invoke<boolean>('begin_mod_operations'),
    cancelModOperations: async () => invoke<boolean>('cancel_mod_operations'),
    modOperationsCancelled: async () => invoke<boolean>('mod_operations_cancelled'),
    inspectCustomMod: async (path) => invoke('inspect_custom_mod', { path }),
    cancelCustomModImport: async () => invoke('cancel_custom_mod_import'),
    importCustomMod: async (profileId, path, options) => invoke('import_custom_mod', {
        profileId,
        path,
        name: options.name,
        author: options.author,
        version: options.version,
        platforms: options.platforms,
    }),
    refreshLocalModMetadata: async (profileId, localId, sourcePath, enabled) => invoke('refresh_local_mod_metadata', {
        profileId,
        localId,
        sourcePath,
        enabled,
    }),
    importEmbeddedCustomMod: async (profileId, archivePath, payloadPath, options) => invoke('import_embedded_custom_mod', {
        profileId,
        archivePath,
        payloadPath,
        name: options.name,
        author: options.author,
        version: options.version,
        enabled: options.enabled,
        platforms: options.platforms,
        expectedSha256: options.expectedSha256,
    }),
    installLocalMod: async (profileId, localId, modName, gamePath, useProfileCache) => {
        try {
            await invoke('install_local_mod', { profileId, localId, modName, gamePath, useProfileCache: useProfileCache ?? false });
            return { success: true };
        } catch (e) {
            return { success: false, error: String(e) };
        }
    },
    deleteLocalModPayload: async (profileId, localId) => invoke<boolean>('delete_local_mod_payload', { profileId, localId }),
    checkDirectoryExists: async (path) => invoke<boolean>('check_directory_exists', { path }),
    fetchCommunities: (refresh?: boolean) => invoke<Community[]>('fetch_communities', { refresh }),
    fetchCommunityImages: (refresh?: boolean) => invoke<Record<string, string>>('fetch_community_images', { refresh }),
    resolveCommunityPlatforms: (games: { identifier: string; name: string }[]) =>
        invoke<Record<string, CommunityPlatformInfo>>('resolve_community_platforms', { games }),
    fetchPackages: (gameId: string) => fetchPackagesDeduped(gameId),
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
    ): Promise<{ items: Package[], total: number }> {
        return await invoke<{ items: Package[], total: number }>('get_packages', {
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
    duplicateProfileFolder: async (sourceProfileId, newProfileId) => invoke<boolean>('duplicate_profile_folder', { sourceProfileId, newProfileId }),
    getSettings: async () => invoke('get_settings'),
    saveSettings: async (settings) => invoke('save_settings', { settings }),
    requestSponsor: async (placement) => invoke('request_sponsor', { placement }),
    acknowledgeSponsorDisplay: async (sponsorId) => invoke('acknowledge_sponsor_display', { sponsorId }),
    dismissSponsor: async (sponsorId) => invoke('dismiss_sponsor', { sponsorId }),
    updateSponsorPreferences: async (enabled) => invoke('update_sponsor_preferences', { enabled }),
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
        return await invoke('check_update_secure', { currentVersion });
    },
    installUpdate: async (downloadUrl: string) => {
        return await invoke('install_update', { downloadUrl });
    },
    syncProfileToGame: async (profileId: string, gameIdentifier: string, useLegacyCache?: boolean, finalize?: boolean) => {
        return await invoke('sync_profile_to_game', { profileId, gameIdentifier, useLegacyCache: useLegacyCache ?? false, finalize: finalize ?? false });
    },
    checkProfileRuntimeHealth: async (profileId, gameIdentifier, platform?) =>
        invoke('check_profile_runtime_health', { profileId, gameIdentifier, platform }),
    inspectProfileSyncState: async (profileId, gameIdentifier, platform?) =>
        invoke('inspect_profile_sync_state', { profileId, gameIdentifier, platform }),
    beginProfileApplyTransaction: async (profileId, gameIdentifier) =>
        invoke<boolean>('begin_profile_apply_transaction', { profileId, gameIdentifier }),
    rollbackProfileApplyTransaction: async (profileId, gameIdentifier) =>
        invoke<boolean>('rollback_profile_apply_transaction', { profileId, gameIdentifier }),
    commitProfileApplyTransaction: async (profileId, gameIdentifier) =>
        invoke<boolean>('commit_profile_apply_transaction', { profileId, gameIdentifier }),
    copyModFromCache: async (profileId: string, modName: string, gamePath: string) => {
        return await invoke<{ success: boolean; copied: boolean }>('copy_mod_from_cache', { profileId, modName, gamePath });
    },
    clearProfileCache: async () => {
        return await invoke<{ cleared: number; chunks_cleared?: number; bytes_freed: number }>('clear_profile_cache', {});
    },
    openProfileFolder: async (profileId: string) => {
        return await invoke('open_profile_folder', { profileId });
    },
    isGameRunning: async (gameIdentifier: string, platform?: 'windows' | 'mac') => {
        return await invoke<boolean>('is_game_running', { gameIdentifier, platform });
    },
    launchGameWithMods: async (gameIdentifier: string, profileId: string, platform?: 'windows' | 'mac') => {
        return await invoke('launch_game_with_mods', { gameIdentifier, profileId, platform });
    },
    launchGameVanilla: async (gameIdentifier: string, profileId: string, platform?: 'windows' | 'mac') => {
        return await invoke('launch_game_vanilla', { gameIdentifier, profileId, platform });
    },
    stopGame: async (gameIdentifier: string, platform?: 'windows' | 'mac') => {
        return await invoke('stop_game', { gameIdentifier, platform });
    },
    listProfileConfigFiles: async (profileId: string, gameIdentifier?: string, platform?: string): Promise<ConfigFileInfo[]> => {
        return await invoke('list_profile_config_files', { profileId, gameIdentifier: gameIdentifier ?? null, platform: platform ?? null });
    },
    readProfileConfigFile: async (profileId: string, relativePath: string, root?: string): Promise<string> => {
        return await invoke('read_profile_config_file', { profileId, relativePath, root: root ?? null });
    },
    writeProfileConfigFile: async (profileId: string, relativePath: string, content: string, root?: string): Promise<boolean> => {
        return await invoke('write_profile_config_file', { profileId, relativePath, content, root: root ?? null });
    },
    revealProfileConfigFile: async (profileId: string, relativePath: string, root?: string): Promise<void> => {
        return await invoke('reveal_profile_config_file', { profileId, relativePath, root: root ?? null });
    },
    openProfileConfigFile: async (profileId: string, relativePath: string, root?: string): Promise<void> => {
        return await invoke('open_profile_config_file', { profileId, relativePath, root: root ?? null });
    },
    openAppLogsFolder: async () => invoke('open_app_logs_folder'),
    getAppLogsSize: async () => invoke<number>('get_app_logs_size'),
    clearAppLogs: async () => invoke<number>('clear_app_logs'),
    setVerboseLogging: async (enabled: boolean) => invoke('set_verbose_logging', { enabled }),

    listThemes: async () => invoke<ThemeSummary[]>('list_themes'),
    readThemeSource: async (fileName: string) => invoke<string>('read_theme_source', { fileName }),
    writeTheme: async (fileName: string, content: string) => invoke('write_theme', { fileName, content }),
    deleteTheme: async (fileName: string) => invoke('delete_theme', { fileName }),
    openThemesFolder: async () => invoke('open_themes_folder'),
    suggestThemeFileName: async (name: string) => invoke<string>('suggest_theme_file_name', { name }),
    setActiveTheme: async (fileName: string | null) => invoke('set_active_theme', { fileName }),
    importThemeImage: async (sourcePath: string) => invoke<string>('import_theme_image', { sourcePath }),
    readThemeImage: async (relativePath: string) => invoke<string | null>('read_theme_image', { relativePath }),
};

/** Metadata about a config file inside a profile directory. */
export interface ConfigFileInfo {
    /** Display name of the file (e.g. "BepInEx.cfg") */
    name: string;
    /** Path relative to the root (e.g. "BepInEx/config/BepInEx.cfg") */
    relative_path: string;
    /** Absolute path to the root directory. Used by read/write commands. */
    root: string;
}
