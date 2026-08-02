export type ProfilePlatform = 'windows' | 'mac';
export type ProfileDistribution = 'steam' | 'manual';
export type ProfileLaunchMode = 'auto' | 'steam' | 'direct';
export type InstalledModSource = 'thunderstore' | 'local';
export type PendingSyncKind = 'add' | 'update' | 'enable' | 'disable';

export interface CustomModSecurityReport {
    riskLevel: 'low' | 'medium' | 'high';
    warnings: string[];
    executableFiles: string[];
    totalFiles: number;
    totalUncompressedBytes: number;
}

export interface InstalledMod {
    uuid4: string;
    fullName: string; // e.g. "ebkr-r2modman-3.1.0"
    versionNumber: string;
    iconUrl?: string;
    enabled: boolean;
    source?: InstalledModSource;
    localId?: string;
    displayName?: string;
    author?: string;
    description?: string;
    readme?: string;
    fileName?: string;
    fileSize?: number;
    sha256?: string;
    manifestSha256?: string;
    contentFingerprint?: string;
    sourcePath?: string;
    platforms?: Array<'windows' | 'mac' | 'linux'>;
    securityReport?: CustomModSecurityReport;
    pending_sync?: boolean;
    synced_enabled?: boolean;
    pending_sync_kind?: PendingSyncKind;
    sync_baseline?: InstalledModSnapshot | null;
}

export type InstalledModSnapshot = Omit<InstalledMod,
    'pending_sync' | 'synced_enabled' | 'pending_sync_kind' | 'sync_baseline'>;

export interface PendingModRemoval {
    id: string;
    mod: InstalledModSnapshot;
}

export interface SelectiveSyncRestore {
    mods: InstalledMod[];
    pending_removals: PendingModRemoval[];
    needs_sync: boolean;
}

export interface Profile {
    id: string;
    name: string;
    gameIdentifier: string;
    mods: InstalledMod[];
    needs_sync?: boolean;
    apply_interrupted?: boolean;
    pending_removals?: PendingModRemoval[];
    selective_sync_restore?: SelectiveSyncRestore;
    dateCreated: number;
    lastUsed: number;
    profileImageUrl?: string;
    is_vanilla?: boolean;
    platform?: ProfilePlatform;
    distribution?: ProfileDistribution;
    launchMode?: ProfileLaunchMode;
}
