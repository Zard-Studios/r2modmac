import type { InstalledMod, Profile } from './profile';
import type { Community, Package, CommunityPlatformInfo } from './thunderstore';
import type { ConfigFileInfo } from '../tauriAdapter';
import type { ProfileSyncInspection } from '../utils/profileSync';


export interface AppSettings {
    steam_path: string | null;
    windows_steam_path?: string | null;
    mac_steam_path?: string | null;
    favorite_games: string[];
    default_game?: string | null;
    default_profile?: string | null;
    active_theme?: string | null;
    game_paths: Record<string, string>;
    legacy_install_mode?: boolean;
    ask_version_before_install?: boolean;
    install_in_parallel?: boolean;
    confirm_before_apply_to_game?: boolean;
    write_debug_logs_to_game?: boolean;
    verbose_logging?: boolean;
    default_mod_view_mode?: 'grid' | 'list';
    show_deprecated_warnings?: boolean;
    hide_crossover_guide?: boolean;
    hide_macos_guide?: boolean;
    stream_mode?: boolean;
    sponsored_messages_enabled?: boolean;
    sponsored_messages_scale?: number;
    sponsored_messages_background_opacity?: number;
}

export interface SponsorMessage {
    id: string;
    sponsorName?: string | null;
    message: string;
    url?: string | null;
}

export type SponsorPlacement = 'preferences-support' | 'home-support' | 'profile-selector-support' | 'catalog-support';

/** A theme file on disk, as parsed by the backend. */
export interface ThemeSummary {
    /** Identifies the theme; also what `AppSettings.active_theme` stores. */
    file_name: string;
    name: string;
    author?: string | null;
    /** Only the colours the file actually defines; the rest fall back. */
    colors: Partial<Record<
        'background' | 'surface' | 'surface_hover' | 'border' | 'text' | 'text_muted' | 'accent' | 'accent_hover'
        | 'danger' | 'warning' | 'success'
        | 'on_accent' | 'on_surface' | 'on_danger' | 'on_warning' | 'on_success' | 'icon'
        | 'media_scrim' | 'media_ink',
        string
    >>;
    background_image?: {
        path?: string | null;
        opacity?: number | null;
        blur?: number | null;
        fit?: 'cover' | 'contain' | 'fill' | 'tile' | 'center' | null;
        offset_x?: number | null;
        offset_y?: number | null;
        tile_scale?: number | null;
    } | null;
    options?: { auto_contrast?: boolean | null } | null;
    /** Set when the file could not be parsed, so the UI can say why. */
    error?: string | null;
}

export interface RuntimeHealth {
    runtime: 'bepinex' | 'owml' | 'lovely' | 'returnofmodding';
    status: 'healthy' | 'missing' | 'incomplete' | 'unconfigured' | 'unsupported';
    missingComponents: string[];
    repairable: boolean;
}

export interface ProfileSyncResult {
    removed: number;
    to_install: string[];
    already_installed: number;
    cached: number;
    pending_removals: number;
}

export interface IElectronAPI {
    getProfiles: () => Promise<Profile[]>;
    saveProfiles: (profiles: Profile[]) => Promise<boolean>;
    selectFolder: () => Promise<string | null>;
    getUsername: () => Promise<string>;
    selectFile: (filters?: { name: string; extensions: string[] }[]) => Promise<string | null>;
    selectImportPath: () => Promise<string | null>;
    installMod: (profileId: string, downloadUrl: string, modName: string, gamePath: string, useProfileCache?: boolean) => Promise<{ success: boolean; uniqueName?: string; dependencies?: string[]; error?: string }>;
    beginModOperations: () => Promise<boolean>;
    cancelModOperations: () => Promise<boolean>;
    modOperationsCancelled: () => Promise<boolean>;
    inspectCustomMod: (path: string) => Promise<any>;
    cancelCustomModImport: () => Promise<boolean>;
    importCustomMod: (
        profileId: string,
        path: string,
        options: { name?: string; author?: string; version?: string; platforms?: string[] }
    ) => Promise<{ mod: InstalledMod; inspection: any }>;
    refreshLocalModMetadata: (
        profileId: string,
        localId: string,
        sourcePath?: string,
        enabled?: boolean
    ) => Promise<{ changed: boolean; mod: InstalledMod; inspection: any }>;
    importEmbeddedCustomMod: (
        profileId: string,
        archivePath: string,
        payloadPath: string,
        options: { name?: string; author?: string; version?: string; enabled?: boolean; platforms?: string[]; expectedSha256?: string }
    ) => Promise<{ mod: InstalledMod; inspection: any }>;
    installLocalMod: (profileId: string, localId: string, modName: string, gamePath: string, useProfileCache?: boolean) => Promise<{ success: boolean; error?: string }>;
    deleteLocalModPayload: (profileId: string, localId: string) => Promise<boolean>;
    checkDirectoryExists: (dirPath: string) => Promise<boolean>;
    fetchCommunities: (refresh?: boolean) => Promise<Community[]>;
    fetchCommunityImages: (refresh?: boolean) => Promise<Record<string, string>>;
    resolveCommunityPlatforms: (games: { identifier: string; name: string }[]) => Promise<Record<string, CommunityPlatformInfo>>;
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
    ): Promise<{ items: Package[]; total: number; }>;
    lookupPackagesByNames: (gameId: string, names: string[]) => Promise<{ found: Package[]; unknown: string[] }>;
    fetchPackageByName: (name: string, gameId?: string | null) => Promise<Package | null>;
    importProfile: (code: string) => Promise<any>;
    importProfileFromFile: (path: string) => Promise<any>;
    shareProfile: (profileId: string) => Promise<string>;
    openModFolder: (profileId: string, modName: string, gameIdentifier: string, platform?: 'windows' | 'mac') => Promise<void>;
    exportProfile: (profileId: string) => Promise<any>;
    deleteProfileFolder: (profileId: string, gameIdentifier?: string, platform?: 'windows' | 'mac') => Promise<boolean>;
    getSettings: () => Promise<AppSettings>;
    saveSettings: (settings: AppSettings) => Promise<void>;
    requestSponsor: (placement?: SponsorPlacement) => Promise<SponsorMessage | null>;
    acknowledgeSponsorDisplay: (sponsorId: string) => Promise<void>;
    dismissSponsor: (sponsorId: string) => Promise<void>;
    updateSponsorPreferences: (enabled: boolean) => Promise<void>;
    getGamePath: (gameIdentifier: string, platform?: 'windows' | 'mac') => Promise<string | null>;
    getGameSource: (gameIdentifier: string, platform?: 'windows' | 'mac') => Promise<'steam' | 'manual' | 'unknown'>;
    setGamePath: (gameIdentifier: string, path: string, platform?: 'windows' | 'mac') => Promise<void>;
    openGameFolder: (gameIdentifier: string, platform?: 'windows' | 'mac') => Promise<void>;
    removeMod: (profileId: string, modName: string) => Promise<void>;
    toggleMod: (profileId: string, modName: string, enabled: boolean, gameIdentifier?: string, platform?: 'windows' | 'mac') => Promise<void>;
    confirm: (title: string, message: string) => Promise<boolean>;
    alert: (title: string, message: string) => Promise<void>;
    readImage: (path: string) => Promise<string | null>;
    installToGame: (gameIdentifier: string, profileId: string, disabledMods: string[], isVanillaOverride?: boolean) => Promise<void>;
    fetchTextContent: (url: string) => Promise<string>;
    checkUpdate: (currentVersion: string) => Promise<UpdateInfo>;
    installUpdate: (downloadUrl: string) => Promise<void>;
    lookupPackagesByNames: (gameId: string, names: string[]) => Promise<any>;
    syncProfileToGame: (profileId: string, gameIdentifier: string, useLegacyCache?: boolean, finalize?: boolean) => Promise<ProfileSyncResult>;
    checkProfileRuntimeHealth: (profileId: string, gameIdentifier: string, platform?: 'windows' | 'mac') => Promise<RuntimeHealth>;
    inspectProfileSyncState: (profileId: string, gameIdentifier: string, platform?: 'windows' | 'mac') => Promise<ProfileSyncInspection>;
    beginProfileApplyTransaction: (profileId: string, gameIdentifier: string) => Promise<boolean>;
    rollbackProfileApplyTransaction: (profileId: string, gameIdentifier: string) => Promise<boolean>;
    commitProfileApplyTransaction: (profileId: string, gameIdentifier: string) => Promise<boolean>;
    copyModFromCache: (profileId: string, modName: string, gamePath: string) => Promise<{ success: boolean; copied: boolean }>;
    clearProfileCache: () => Promise<{ cleared: number; chunks_cleared?: number; bytes_freed: number }>;
    openProfileFolder: (profileId: string) => Promise<void>;
    isGameRunning: (gameIdentifier: string, platform?: 'windows' | 'mac') => Promise<boolean>;
    launchGameWithMods: (gameIdentifier: string, profileId: string, platform?: 'windows' | 'mac') => Promise<void>;
    launchGameVanilla: (gameIdentifier: string, profileId: string, platform?: 'windows' | 'mac') => Promise<void>;
    stopGame: (gameIdentifier: string, platform?: 'windows' | 'mac') => Promise<void>;
    listProfileConfigFiles: (profileId: string, gameIdentifier?: string, platform?: string) => Promise<ConfigFileInfo[]>;
    readProfileConfigFile: (profileId: string, relativePath: string, root?: string) => Promise<string>;
    writeProfileConfigFile: (profileId: string, relativePath: string, content: string, root?: string) => Promise<boolean>;
    revealProfileConfigFile: (profileId: string, relativePath: string, root?: string) => Promise<void>;
    openProfileConfigFile: (profileId: string, relativePath: string, root?: string) => Promise<void>;
    openAppLogsFolder: () => Promise<void>;
    setVerboseLogging: (enabled: boolean) => Promise<void>;
    listThemes: () => Promise<ThemeSummary[]>;
    readThemeSource: (fileName: string) => Promise<string>;
    writeTheme: (fileName: string, content: string) => Promise<void>;
    deleteTheme: (fileName: string) => Promise<void>;
    openThemesFolder: () => Promise<void>;
    suggestThemeFileName: (name: string) => Promise<string>;
    setActiveTheme: (fileName: string | null) => Promise<void>;
    importThemeImage: (sourcePath: string) => Promise<string>;
    readThemeImage: (relativePath: string) => Promise<string | null>;
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
