import type { Profile } from './profile';
import type { Community, Package } from './thunderstore';

export interface IElectronAPI {
    getProfiles: () => Promise<Profile[]>;
    saveProfiles: (profiles: Profile[]) => Promise<boolean>;
    selectFolder: () => Promise<string | null>;
    installMod: (downloadUrl: string, modName: string, gameDir: string) => Promise<{ success: boolean; error?: string }>;
    checkDirectoryExists: (dirPath: string) => Promise<boolean>;
    fetchCommunities: () => Promise<Community[]>;
    fetchPackages: (communityIdentifier: string) => Promise<Package[]>;
}

declare global {
    interface Window {
        ipcRenderer: IElectronAPI;
    }
}
