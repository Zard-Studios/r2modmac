export interface InstalledMod {
    uuid4: string;
    fullName: string; // e.g. "ebkr-r2modman-3.1.0"
    versionNumber: string;
    enabled: boolean;
}

export interface Profile {
    id: string;
    name: string;
    gameIdentifier: string;
    mods: InstalledMod[];
    dateCreated: number;
    lastUsed: number;
}
