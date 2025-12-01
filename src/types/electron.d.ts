import type { Profile } from './profile';
import type { Community, Package } from './thunderstore';

export interface IElectronAPI {
    getProfiles: () => Promise<Profile[]>;
    saveProfiles: (profiles: Profile[]) => Promise<boolean>;
    selectFolder: () => Promise<string | null>;
    selectFile: () => Promise<string | null>;
    installMod: (profileId: string, downloadUrl: string, modName: string) => Promise<{ success: boolean; error?: string }>;
    checkDirectoryExists: (dirPath: string) => Promise<boolean>;
    fetchCommunities: () => Promise<Community[]>;
    fetchCommunityImages: () => Promise<Record<string, string>>;
    fetchPackages: (gameId: string) => Promise<number>;
    getPackages: (gameId: string, page: number, pageSize: number, search: string) => Promise<any[]>;
    fetchPackageByName: (name: string) => Promise<any>;
    importProfile: (code: string) => Promise<any>;
    importProfileFromFile: (path: string) => Promise<any>;
    openModFolder: (profileId: string, modName: string) => Promise<void>;
    exportProfile: (profileId: string) => Promise<any>;
    deleteProfileFolder: (profileId: string) => Promise<boolean>;
    removeMod: (profileId: string, modName: string) => Promise<void>;
    confirm: (title: string, message: string) => Promise<boolean>;
}

declare global {
    interface Window {
        ipcRenderer: IElectronAPI;
    }
}
