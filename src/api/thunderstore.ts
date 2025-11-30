import type { Community, Package } from '../types/thunderstore';

export const ThunderstoreAPI = {
    async getCommunities(): Promise<Community[]> {
        return window.ipcRenderer.fetchCommunities();
    },

    async getPackages(communityIdentifier: string): Promise<Package[]> {
        return window.ipcRenderer.fetchPackages(communityIdentifier);
    },

    async getPackage(communityIdentifier: string, packageName: string): Promise<Package> {
        const packages = await this.getPackages(communityIdentifier);
        const pkg = packages.find(p => p.full_name === packageName);
        if (!pkg) throw new Error(`Package ${packageName} not found`);
        return pkg;
    }
};
