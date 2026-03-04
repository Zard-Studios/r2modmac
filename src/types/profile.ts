export interface InstalledMod {
    uuid4: string;
    fullName: string; // e.g. "ebkr-r2modman-3.1.0"
    versionNumber: string;
    iconUrl?: string;
    enabled: boolean;
    pending_sync?: boolean;
    synced_enabled?: boolean;
}

export interface Profile {
    id: string;
    name: string;
    gameIdentifier: string;
    mods: InstalledMod[];
    needs_sync?: boolean;
    dateCreated: number;
    lastUsed: number;
    profileImageUrl?: string;
    is_vanilla?: boolean;
    platform?: 'windows' | 'mac';
}
