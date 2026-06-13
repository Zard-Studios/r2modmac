import type { InstalledMod, Profile } from './profile';
import type { Community, Package, CommunityPlatformInfo } from './thunderstore';
import type { ConfigFileInfo } from '../tauriAdapter';


export interface AppSettings {
    steam_path: string | null;
    windows_steam_path?: string | null;
    mac_steam_path?: string | null;
    favorite_games: string[];
    game_paths: Record<string, string>;
    legacy_install_mode?: boolean;
    ask_version_before_install?: boolean;
    install_in_parallel?: boolean;
    confirm_before_apply_to_game?: boolean;
    write_debug_logs_to_game?: boolean;
    default_mod_view_mode?: 'grid' | 'list';
    hide_crossover_guide?: boolean;
    hide_macos_guide?: boolean;
}

export interface IElectronAPI {
    getProfiles: () => Promise<Profile[]>;
    saveProfiles: (profiles: Profile[]) => Promise<boolean>;
    selectFolder: () => Promise<string | null>;
    selectFile: (filters?: { name: string; extensions: string[] }[]) => Promise<string | null>;
    selectImportPath: () => Promise<string | null>;
    installMod: (profileId: string, downloadUrl: string, modName: string, gamePath: string, useProfileCache?: boolean) => Promise<{ success: boolean; error?: string }>;
    inspectCustomMod: (path: string) => Promise<any>;
    cancelCustomModImport: () => Promise<boolean>;
    importCustomMod: (
        profileId: string,
        path: string,
        options: { name?: string; author?: string; version?: string; platforms?: string[] }
    ) => Promise<{ mod: InstalledMod; inspection: any }>;
    refreshLocalModMetadata: (
        profileId: string,
        localId: string,
        sourcePath?: string,
        enabled?: boolean
    ) => Promise<{ changed: boolean; mod: InstalledMod; inspection: any }>;
    importEmbeddedCustomMod: (
        profileId: string,
        archivePath: string,
        payloadPath: string,
        options: { name?: string; author?: string; version?: string; enabled?: boolean; platforms?: string[]; expectedSha256?: string }
    ) => Promise<{ mod: InstalledMod; inspection: any }>;
    installLocalMod: (profileId: string, localId: string, modName: string, gamePath: string, useProfileCache?: boolean) => Promise<{ success: boolean; error?: string }>;
    deleteLocalModPayload: (profileId: string, localId: string) => Promise<boolean>;
    checkDirectoryExists: (dirPath: string) => Promise<boolean>;
    fetchCommunities: () => Promise<Community[]>;
    fetchCommunityImages: () => Promise<Record<string, string>>;
    resolveCommunityPlatforms: (games: { identifier: string; name: string }[]) => Promise<Record<string, CommunityPlatformInfo>>;
    fetchPackages: (gameId: string) => Promise<number>;
    getAvailableCategories: (gameId: string) => Promise<string[]>;
    getPackages(
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
    ): Promise<any[]>;
    lookupPackagesByNames: (gameId: string, names: string[]) => Promise<{ found: Package[]; unknown: string[] }>;
    fetchPackageByName: (name: string, gameId?: string | null) => Promise<Package | null>;
    importProfile: (code: string) => Promise<any>;
    importProfileFromFile: (path: string) => Promise<any>;
    shareProfile: (profileId: string) => Promise<string>;
    openModFolder: (profileId: string, modName: string, gameIdentifier: string, platform?: 'windows' | 'mac') => Promise<void>;
    exportProfile: (profileId: string) => Promise<any>;
    deleteProfileFolder: (profileId: string, gameIdentifier?: string, platform?: 'windows' | 'mac') => Promise<boolean>;
    getSettings: () => Promise<AppSettings>;
    saveSettings: (settings: AppSettings) => Promise<void>;
    getGamePath: (gameIdentifier: string, platform?: 'windows' | 'mac') => Promise<string | null>;
    getGameSource: (gameIdentifier: string, platform?: 'windows' | 'mac') => Promise<'steam' | 'manual' | 'unknown'>;
    setGamePath: (gameIdentifier: string, path: string, platform?: 'windows' | 'mac') => Promise<void>;
    openGameFolder: (gameIdentifier: string, platform?: 'windows' | 'mac') => Promise<void>;
    removeMod: (profileId: string, modName: string) => Promise<void>;
    toggleMod: (profileId: string, modName: string, enabled: boolean, gameIdentifier?: string, platform?: 'windows' | 'mac') => Promise<void>;
    confirm: (title: string, message: string) => Promise<boolean>;
    alert: (title: string, message: string) => Promise<void>;
    readImage: (path: string) => Promise<string | null>;
    installToGame: (gameIdentifier: string, profileId: string, disabledMods: string[], isVanillaOverride?: boolean) => Promise<void>;
    fetchTextContent: (url: string) => Promise<string>;
    checkUpdate: (currentVersion: string) => Promise<UpdateInfo>;
    installUpdate: (downloadUrl: string) => Promise<void>;
    lookupPackagesByNames: (gameId: string, names: string[]) => Promise<any>;
    syncProfileToGame: (profileId: string, gameIdentifier: string, useLegacyCache?: boolean) => Promise<{ removed: number; to_install: string[]; already_installed: number; cached: number }>;
    copyModFromCache: (profileId: string, modName: string, gamePath: string) => Promise<{ success: boolean; copied: boolean }>;
    clearProfileCache: () => Promise<{ cleared: number; chunks_cleared?: number; bytes_freed: number }>;
    openProfileFolder: (profileId: string) => Promise<void>;
    isGameRunning: (gameIdentifier: string, platform?: 'windows' | 'mac') => Promise<boolean>;
    launchGameWithMods: (gameIdentifier: string, profileId: string, platform?: 'windows' | 'mac') => Promise<void>;
    launchGameVanilla: (gameIdentifier: string, profileId: string, platform?: 'windows' | 'mac') => Promise<void>;
    stopGame: (gameIdentifier: string, platform?: 'windows' | 'mac') => Promise<void>;
    listProfileConfigFiles: (profileId: string, gameIdentifier?: string, platform?: string) => Promise<ConfigFileInfo[]>;
    readProfileConfigFile: (profileId: string, relativePath: string, root?: string) => Promise<string>;
    writeProfileConfigFile: (profileId: string, relativePath: string, content: string, root?: string) => Promise<boolean>;
    revealProfileConfigFile: (profileId: string, relativePath: string, root?: string) => Promise<void>;
    openProfileConfigFile: (profileId: string, relativePath: string, root?: string) => Promise<void>;
}

export interface UpdateInfo {
    available: boolean;
    version: string;
    notes: string;
    download_url?: string;
}

declare global {
    interface Window {
        ipcRenderer: IElectronAPI;
    }
}
