export type ProfilePlatform = 'windows' | 'mac';
export type ProfileDistribution = 'steam' | 'manual';
export type ProfileLaunchMode = 'auto' | 'steam' | 'direct';
/** Which architecture modded BepInEx games run in on Apple Silicon; absent means automatic. */
export type ProfileArchitecture = 'auto' | 'native' | 'rosetta';
export type InstalledModSource = 'thunderstore' | 'hexium' | 'outerwilds' | 'local';
export type PendingSyncKind = 'add' | 'update' | 'enable' | 'disable';
export type PendingSyncStatus = 'queued' | 'syncing' | 'ready' | 'failed';

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
    pending_sync_status?: PendingSyncStatus;
    pending_sync_error?: string;
    sync_baseline?: InstalledModSnapshot | null;
    /** The sidebar folder the mod sits in; folders exist only in the sidebar. */
    folderId?: string;
}

/**
 * `normal` folders only group mods in the sidebar. The `ignore` folder also
 * changes how Apply treats its mods: a custom mod imported from a folder is
 * linked to that folder, everything else is left exactly as it is in the game.
 */
export type ModFolderKind = 'normal' | 'ignore';

export interface ModFolder {
    id: string;
    name: string;
    kind: ModFolderKind;
    collapsed?: boolean;
    /**
     * Where the folder sits among the mods outside any folder: just before
     * the mod with this uuid4, at the end with `null`, at the top when unset.
     */
    anchor?: string | null;
}

export type InstalledModSnapshot = Omit<InstalledMod,
    'pending_sync' | 'synced_enabled' | 'pending_sync_kind' | 'pending_sync_status' | 'pending_sync_error' | 'sync_baseline'>;

export interface PendingModRemoval {
    id: string;
    mod: InstalledModSnapshot;
    sync_status?: PendingSyncStatus;
    sync_error?: string;
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
    /** macOS only; absent is `auto`, so an old profile needs no migration. */
    architecture?: ProfileArchitecture;
    /** Older profiles inherit the setting they were created with. */
    bepinexIsolation?: boolean;
    modFolders?: ModFolder[];
}
