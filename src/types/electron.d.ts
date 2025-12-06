import type { Profile } from './profile';
import type { Community, Package } from './thunderstore';

export interface IElectronAPI {
    getProfiles: () => Promise<Profile[]>;
    saveProfiles: (profiles: Profile[]) => Promise<boolean>;
    selectFolder: () => Promise<string | null>;
    selectFile: (filters?: { name: string; extensions: string[] }[]) => Promise<string | null>;
    installMod: (profileId: string, downloadUrl: string, modName: string) => Promise<{ success: boolean; error?: string }>;
    checkDirectoryExists: (dirPath: string) => Promise<boolean>;
    fetchCommunities: () => Promise<Community[]>;
    fetchCommunityImages: () => Promise<Record<string, string>>;
    fetchPackages: (gameId: string) => Promise<number>;
    getPackages: (gameId: string, page: number, pageSize: number, search: string, sort?: string) => Promise<any[]>;
    lookupPackagesByNames: (gameId: string, names: string[]) => Promise<{ found: Package[]; unknown: string[] }>;
    fetchPackageByName: (name: string, gameId?: string | null) => Promise<Package | null>;
    importProfile: (code: string) => Promise<any>;
    importProfileFromFile: (path: string) => Promise<any>;
    shareProfile: (profileId: string) => Promise<string>;
    openModFolder: (profileId: string, modName: string) => Promise<void>;
    exportProfile: (profileId: string) => Promise<any>;
    deleteProfileFolder: (profileId: string, gameIdentifier?: string) => Promise<boolean>;
    getSettings: () => Promise<{ steam_path: string | null; favorite_games: string[]; game_paths: Record<string, string> }>;
    saveSettings: (settings: { steam_path: string | null; favorite_games: string[]; game_paths: Record<string, string> }) => Promise<void>;
    getGamePath: (gameIdentifier: string) => Promise<string | null>;
    setGamePath: (gameIdentifier: string, path: string) => Promise<void>;
    openGameFolder: (gameIdentifier: string) => Promise<void>;
    removeMod: (profileId: string, modName: string) => Promise<void>;
    toggleMod: (profileId: string, modName: string, enabled: boolean, gameIdentifier?: string) => Promise<void>;
    confirm: (title: string, message: string) => Promise<boolean>;
    alert: (title: string, message: string) => Promise<void>;
    readImage: (path: string) => Promise<string | null>;
    installToGame: (gameIdentifier: string, profileId: string, disabledMods: string[]) => Promise<void>;
    fetchTextContent: (url: string) => Promise<string>;
    lookupPackagesByNames: (gameId: string, names: string[]) => Promise<any>;
}

declare global {
    interface Window {
        ipcRenderer: IElectronAPI;
    }
}
