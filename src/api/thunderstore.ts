import type { Community, Package } from '../types/thunderstore';

export async function fetchCommunities(): Promise<Community[]> {
    return window.ipcRenderer.fetchCommunities();
}

export async function fetchPackages(communityIdentifier: string): Promise<number> {
    return window.ipcRenderer.fetchPackages(communityIdentifier);
}

export async function getPackages(communityIdentifier: string, page: number, pageSize: number, search: string): Promise<Package[]> {
    return window.ipcRenderer.getPackages(communityIdentifier, page, pageSize, search);
}

export async function fetchPackage(communityIdentifier: string, packageName: string): Promise<Package> {
    // Fetch all packages and search locally (backend already has them cached)
    const packages = await getPackages(communityIdentifier, 0, 10000, '');
    const pkg = packages.find(p => p.full_name === packageName);
    if (!pkg) throw new Error(`Package ${packageName} not found`);
    return pkg;
}

export async function fetchPackageByUuid(uuid: string): Promise<Package> {
    // Direct lookup via Thunderstore API v1
    // The endpoint is https://thunderstore.io/api/v1/package/{uuid}/
    // Note: This endpoint returns a single package object
    const response = await fetch(`https://thunderstore.io/api/v1/package/${uuid}/`);
    if (!response.ok) {
        throw new Error(`Failed to fetch package by UUID: ${response.statusText}`);
    }
    return response.json();
}
