import type { Profile } from './profile';
import type { Community, Package } from './thunderstore';

export interface IElectronAPI {
    getProfiles: () => Promise<Profile[]>;
    saveProfiles: (profiles: Profile[]) => Promise<boolean>;
    selectFolder: () => Promise<string | null>;
    selectFile: (filters?: { name: string; extensions: string[] }[]) => Promise<string | null>;
    installMod: (profileId: string, downloadUrl: string, modName: string, gamePath: string, useProfileCache?: boolean) => Promise<{ success: boolean; error?: string }>;
    checkDirectoryExists: (dirPath: string) => Promise<boolean>;
    fetchCommunities: () => Promise<Community[]>;
    fetchCommunityImages: () => Promise<Record<string, string>>;
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
    openModFolder: (profileId: string, modName: string, gameIdentifier: string) => Promise<void>;
    exportProfile: (profileId: string) => Promise<any>;
    deleteProfileFolder: (profileId: string, gameIdentifier?: string) => Promise<boolean>;
    getSettings: () => Promise<{ steam_path: string | null; favorite_games: string[]; game_paths: Record<string, string>; legacy_install_mode?: boolean }>;
    saveSettings: (settings: { steam_path: string | null; favorite_games: string[]; game_paths: Record<string, string>; legacy_install_mode?: boolean }) => Promise<void>;
    getGamePath: (gameIdentifier: string) => Promise<string | null>;
    setGamePath: (gameIdentifier: string, path: string) => Promise<void>;
    openGameFolder: (gameIdentifier: string) => Promise<void>;
    removeMod: (profileId: string, modName: string) => Promise<void>;
    toggleMod: (profileId: string, modName: string, enabled: boolean, gameIdentifier?: string) => Promise<void>;
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
    clearProfileCache: () => Promise<{ cleared: number; bytes_freed: number }>;
    openProfileFolder: (profileId: string) => Promise<void>;
    launchGameWithMods: (gameIdentifier: string) => Promise<void>;
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
