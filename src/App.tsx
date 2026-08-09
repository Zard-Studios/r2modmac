import { useState, useEffect, useRef, useCallback, useMemo } from 'react'
import { Button } from './components/ui'
import { Layout } from './components/Layout'
import type { FilterOptions } from './components/FilterPopover'
import { FilterPopover } from './components/FilterPopover'
import { GameSelectionScreen } from './components/screens/GameSelectionScreen'
import { SearchBar } from './components/SearchBar'
import { VirtualizedModGrid } from './components/VirtualizedModGrid'
import { ProfileList } from './components/profiles/ProfileList'
import { CommandPalette } from './components/CommandPalette'
import { CommandSource } from './components/CommandSource'
import { KeyboardShortcuts } from './components/KeyboardShortcuts'
import { useCommandSource, useCommandStore } from './store/useCommandStore'
import { getProfileAvatarGradient, getProfileInitial } from './utils/profileAvatar'
import type { CommandItem } from './utils/commandPalette'
import { useKeybindStore } from './store/useKeybindStore';
import { formatAccelerator, overridesFromKeybinds } from './utils/keybinds';
import { ProfileSidebar } from './components/profiles/ProfileSidebar';
import { useProfileStore } from './store/useProfileStore';
import { useAppStore } from './store/useAppStore';
import { useThemeStore } from './store/useThemeStore';
import type { CommunityPlatformInfo, Package, PackageVersion } from './types/thunderstore';
import { getVersion } from '@tauri-apps/api/app';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { flushSync } from 'react-dom';
import { AppModals } from './components/screens/AppModals';
import { UpdateAllModal } from './components/modals/UpdateAllModal';
import { LaunchIssueModal } from './components/modals/LaunchIssueModal';
import { describeLaunchIssue, type LaunchIssue } from './utils/launchIssue';
import type { AppSettings, RuntimeHealth, UpdateInfo } from './types/electron';
import type { InstalledMod } from './types/profile';
import { MAC_IMAGE_CACHE_KEY, MAC_PLATFORM_CACHE_KEY } from './constants/cacheKeys';
import type { PreferencesSettings, PreferencesTarget } from './components/modals/PreferencesModal';
import type { ProgressState } from './types/progress';

import { useModActions } from './hooks/useModActions';
import type { ProfileModUpdate } from './hooks/useModActions';
import { useProfileActions } from './hooks/useProfileActions';
import { useGameSync } from './hooks/useGameSync';
import { compareVersions, findPinnedVersion, parsePackageReference } from './utils/modVersioning';
import { getProfileModKey, hasPendingRuntimeInstall, migratePendingSyncBaselines, restoreInstalledMod } from './utils/profileSync';

const QUICK_MAC_HINTS = new Set([
  'btd6',
  'valheim',
  'slimerancher',
  'stardewvalley',
  'subnautica',
  'subnauticabelowzero',
  'hollowknight',
  'celeste',
  'kerbalspaceprogram',
  'outward',
  'inscryption',
  'cities_skylines',
  '20minutes-till-dawn',
  'stacklands',
  'timberborn',
  'dontstarvetogether',
  'factorio',
  'garrysmod',
  'oxygennotincluded',
  'projectzomboid',
  'rimworld',
  'terraria',
  'hytale',
  'outerwilds',
]);

const VERBOSE_LOG_WARNING_BYTES = 5 * 1024 * 1024;
const VERBOSE_LOG_SIZE_CHECK_INTERVAL_MS = 10 * 60 * 1000;

interface StoredMacPlatformCache {
  version: number;
  known_games: string[];
  mac_platforms: Record<string, CommunityPlatformInfo>;
  updated_at: number;
}

interface StoredMacImageCache {
  version: number;
  mac_images: Record<string, string>;
  missing_ids: string[];
  updated_at: number;
}

interface ImportedProfileMod {
  name?: string;
  version?: string;
  enabled?: boolean;
  source?: string;
  payload?: string;
  displayName?: string;
  author?: string;
  platforms?: string[];
  sha256?: string;
}

type ProfileCommand = 'apply' | 'launch' | 'launch-vanilla' | 'stop' | 'duplicate' | 'export';

interface PendingProfileCommand {
  command: ProfileCommand;
}

function ProfileCommandBridge({
  request,
  handlers,
  onHandled,
}: {
  request: PendingProfileCommand | null;
  handlers: Record<ProfileCommand, () => void>;
  onHandled: () => void;
}) {
  useEffect(() => {
    if (!request) return;
    let cancelled = false;
    queueMicrotask(() => {
      if (cancelled) return;
      handlers[request.command]();
      onHandled();
    });
    return () => { cancelled = true; };
  }, [handlers, onHandled, request]);

  return null;
}

interface ProfileArchiveMergeSummary {
  handled: boolean;
  cancelled?: boolean;
  profileName?: string;
  importedCount: number;
  failedMods: string[];
}

const ARCHIVE_IMPORT_PATTERN = /\.(r2z|zip)$/i;
const SHOW_DEVTOOLS_CONTEXT_MENU_ITEM = import.meta.env.DEV;

const isArchiveImportPath = (path: string) => ARCHIVE_IMPORT_PATTERN.test(path.trim());

const getProfileModName = (mod: ImportedProfileMod) => (
  typeof mod.name === 'string' ? mod.name.trim() : ''
);

const emptyPlatformCache = (): StoredMacPlatformCache => ({
  version: 1,
  known_games: [],
  mac_platforms: {},
  updated_at: 0,
});

const emptyImageCache = (): StoredMacImageCache => ({
  version: 1,
  mac_images: {},
  missing_ids: [],
  updated_at: 0,
});

const normalizePlatformInfo = (value: Partial<CommunityPlatformInfo> | undefined): CommunityPlatformInfo => ({
  windows: value?.windows ?? true,
  mac: value?.mac ?? false,
  linux: value?.linux ?? false,
  confidence: typeof value?.confidence === 'number' ? value.confidence : 0,
  source: typeof value?.source === 'string' ? value.source : 'bootstrap:unknown',
});

const readMacPlatformCache = (): StoredMacPlatformCache => {
  try {
    const raw = localStorage.getItem(MAC_PLATFORM_CACHE_KEY);
    if (!raw) return emptyPlatformCache();
    const parsed = JSON.parse(raw) as Partial<StoredMacPlatformCache>;
    const knownGames = Array.isArray(parsed?.known_games)
      ? parsed.known_games.filter((id): id is string => typeof id === 'string')
      : [];
    const macPlatformsRaw = parsed?.mac_platforms ?? {};
    const macPlatforms: Record<string, CommunityPlatformInfo> = {};
    for (const [id, info] of Object.entries(macPlatformsRaw)) {
      const normalized = normalizePlatformInfo(info as Partial<CommunityPlatformInfo>);
      if (normalized.mac) {
        macPlatforms[id] = normalized;
      }
    }

    return {
      version: 1,
      known_games: knownGames,
      mac_platforms: macPlatforms,
      updated_at: typeof parsed?.updated_at === 'number' ? parsed.updated_at : 0,
    };
  } catch {
    return emptyPlatformCache();
  }
};

const writeMacPlatformCache = (cache: StoredMacPlatformCache) => {
  localStorage.setItem(MAC_PLATFORM_CACHE_KEY, JSON.stringify(cache));
};

const readMacImageCache = (): StoredMacImageCache => {
  try {
    const raw = localStorage.getItem(MAC_IMAGE_CACHE_KEY);
    if (!raw) return emptyImageCache();
    const parsed = JSON.parse(raw) as Partial<StoredMacImageCache>;
    const imagesRaw = parsed?.mac_images ?? {};
    const images: Record<string, string> = {};
    for (const [id, url] of Object.entries(imagesRaw)) {
      if (typeof url === 'string' && url.length > 0) {
        images[id] = url;
      }
    }
    const missingIds = Array.isArray(parsed?.missing_ids)
      ? parsed.missing_ids.filter((id): id is string => typeof id === 'string')
      : [];

    return {
      version: 1,
      mac_images: images,
      missing_ids: missingIds,
      updated_at: typeof parsed?.updated_at === 'number' ? parsed.updated_at : 0,
    };
  } catch {
    return emptyImageCache();
  }
};

const writeMacImageCache = (cache: StoredMacImageCache) => {
  localStorage.setItem(MAC_IMAGE_CACHE_KEY, JSON.stringify(cache));
};

const mergePlatformInfo = (existing: CommunityPlatformInfo, incoming: CommunityPlatformInfo): CommunityPlatformInfo => {
  const existingConfidence = typeof existing.confidence === 'number' ? existing.confidence : 0;
  const incomingConfidence = typeof incoming.confidence === 'number' ? incoming.confidence : 0;
  const incomingIsAuthoritativeSteam =
    typeof incoming.source === 'string' &&
    incoming.source.startsWith('steam_store:') &&
    incomingConfidence >= 0.85;
  const incomingAddsPlatformSignal =
    (!!incoming.windows && !existing.windows) ||
    (!!incoming.mac && !existing.mac) ||
    (!!incoming.linux && !existing.linux);

  if (incomingIsAuthoritativeSteam || incomingConfidence >= existingConfidence + 0.05) {
    return incoming;
  }

  if (!incomingAddsPlatformSignal) {
    return existing;
  }

  return {
    windows: !!existing.windows || !!incoming.windows,
    mac: !!existing.mac || !!incoming.mac,
    linux: !!existing.linux || !!incoming.linux,
    confidence: Math.max(existingConfidence, incomingConfidence),
    source:
      existing.source === incoming.source
        ? existing.source
        : `merge:${existing.source}|${incoming.source}`,
  };
};

const isValidPathForImport = (path: string): boolean => {
  const lower = path.toLowerCase();
  if (lower.endsWith('.zip') || lower.endsWith('.r2z')) {
    return true;
  }
  const lastSegment = path.split(/[/\\]/).pop() || '';
  const dotIndex = lastSegment.lastIndexOf('.');
  if (dotIndex !== -1) {
    const ext = lastSegment.slice(dotIndex).toLowerCase();
    if (ext === '.zip' || ext === '.r2z') return true;
    return false;
  }
  return true;
};

function App() {

  const [allPackages, setAllPackages] = useState<Package[]>([])
  const [totalPackages, setTotalPackages] = useState(0)
  const [currentPage, setCurrentPage] = useState(0)
  const [isFetchingNextPage, setIsFetchingNextPage] = useState(false)
  const [loading, setLoading] = useState(true)
  const [loadingMods, setLoadingMods] = useState(false)
  const [searchQuery, setSearchQuery] = useState('')
  const [gameSearchQuery, setGameSearchQuery] = useState('')
  const [filterOptions, setFilterOptions] = useState<FilterOptions>({
    sort: 'downloads',
    sortDirection: 'desc',
    nsfw: false,
    deprecated: false,
    mods: false,
    modpacks: false,
    categories: [],
  })
  const PAGE_SIZE = 50 
  const [availableCategories, setAvailableCategories] = useState<string[]>([])
  const [profilePackageIndex, setProfilePackageIndex] = useState<Record<string, Package>>({})
  const [isSidebarOpen, setIsSidebarOpen] = useState(true)

  const [selectedMod, setSelectedMod] = useState<Package | null>(null)
  // Game Selector state moved to component
  const [progressState, setProgressState] = useState<ProgressState>({
    isOpen: false,
    title: '',
    progress: 0,
    currentTask: ''
  })
  const [isProgressMinimized, setIsProgressMinimized] = useState(false)
  const [isCancellingCustomModImport, setIsCancellingCustomModImport] = useState(false)
  const [isCustomModDragActive, setIsCustomModDragActive] = useState(false)
  const [isCustomModDragValid, setIsCustomModDragValid] = useState(true)
  const [showSettings, setShowSettings] = useState(false)
  const [isLaunchingProfile, setIsLaunchingProfile] = useState(false)
  const [isStoppingProfile, setIsStoppingProfile] = useState(false)
  const [isGameRunning, setIsGameRunning] = useState(false)
  const [isSteamRestarting, setIsSteamRestarting] = useState(false)
  const applyInFlightRef = useRef(false)
  const profileActionLockRef = useRef(false)
  const steamRestartingRef = useRef(false)
  const customModImportCancelledRef = useRef(false)
  const launchGraceUntilRef = useRef(0)
  const [isApplyingToGame, setIsApplyingToGame] = useState(false)
  const [showExportModal, setShowExportModal] = useState(false)
  const [showCrossOverGuide, setShowCrossOverGuide] = useState(false)
  const [hideCrossOverGuide, setHideCrossOverGuide] = useState(false)
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null)
  const [uninstallModalState, setUninstallModalState] = useState<{
    isOpen: boolean;
    pkg: Package | null;
    orphanDeps: { name: string; icon?: string }[];
    allInstalledDepDetails: { name: string; icon?: string }[];
    allInstalledDeps: string[];
    profileId: string | null;
  }>({
    isOpen: false,
    pkg: null,
    orphanDeps: [],
    allInstalledDepDetails: [],
    allInstalledDeps: [],
    profileId: null
  })
  const [viewMode, setViewMode] = useState<'grid' | 'list'>('grid')
  const [showUpdateModal, setShowUpdateModal] = useState(false)
  const [launchIssue, setLaunchIssue] = useState<LaunchIssue | null>(null)
  const [showPreferences, setShowPreferences] = useState(false)
  const activeKeybinds = useKeybindStore((state) => state.keybinds)
  const openPalette = useCommandStore((state) => state.open)
  const togglePalette = useCommandStore((state) => state.toggle)
  // Which panel Preferences should land on, when a command names one.
  const [preferencesPanel, setPreferencesPanel] = useState<PreferencesTarget | null>(null)
  const [legacyInstallMode, setLegacyInstallMode] = useState(false)
  const [askVersionBeforeInstall, setAskVersionBeforeInstall] = useState(false)
  const [installInParallel, setInstallInParallel] = useState(true)
  const [confirmBeforeApplyToGame, setConfirmBeforeApplyToGame] = useState(false)
  const [writeDebugLogsToGame, setWriteDebugLogsToGame] = useState(false)
  const [verboseLogging, setVerboseLogging] = useState(false)
  const [hideVerboseLogsWarning, setHideVerboseLogsWarning] = useState(false)
  const [verboseLogsWarningBytes, setVerboseLogsWarningBytes] = useState<number | null>(null)
  const [verboseLogsWarningDismissed, setVerboseLogsWarningDismissed] = useState(false)
  const [settingsHydrated, setSettingsHydrated] = useState(false)
  const [defaultModViewMode, setDefaultModViewMode] = useState<'grid' | 'list'>('grid')
  const [showDeprecatedWarnings, setShowDeprecatedWarnings] = useState(true)
  const [sponsoredMessagesEnabled, setSponsoredMessagesEnabled] = useState(true)
  const [sponsoredMessagesScale, setSponsoredMessagesScale] = useState(80)
  const [sponsoredMessagesOpacity, setSponsoredMessagesOpacity] = useState(80)
  const [isBrowsingMode, setIsBrowsingMode] = useState(false)
  const [defaultGame, setDefaultGame] = useState<string | null>(null)
  const [defaultProfile, setDefaultProfile] = useState<string | null>(null)
  // Captured once from the settings loaded at launch. Deliberately not the
  // reactive `defaultGame` state above: changing the preference mid-session
  // (Preferences form) must only take effect on the *next* launch, not snap
  // the user to a different game immediately after saving.
  const startupDefaultGameRef = useRef<string | null | undefined>(undefined)
  const startupDefaultProfileRef = useRef<string | null | undefined>(undefined)
  const defaultGameAppliedRef = useRef(false)
  const defaultGameValidatedRef = useRef(false)
  const isInitialLoadRunningRef = useRef(false)
  // Distinct from `communities.length > 0`: the startup default-game skip
  // populates `communities` via a cache-only read (loadEssentialsForDefaultGame),
  // which must NOT count as "the full load already happened" — otherwise
  // loadData()'s own guard would block it from ever running when the user
  // later goes back to the home screen.
  const fullCommunitiesLoadDoneRef = useRef(false)
  const packagesLoadRequestRef = useRef(0)
  const profilePackageIndexRequestRef = useRef(0)
  const syncInspectionRequestRef = useRef<string | null>(null)
  const installToGameRequestRef = useRef<((isVanillaOverride?: boolean) => Promise<void>) | null>(null)
  const [pendingProfileCommand, setPendingProfileCommand] = useState<PendingProfileCommand | null>(null)

  const {
    profiles,
    createProfile,
    duplicateProfile,
    loadProfiles,
    activeProfileId,
    selectProfile,
    deleteProfile,
    updateProfile,
    addMod,
    removeMod,
    toggleMod,
    revertPendingMods,
  } = useProfileStore()
  // App State Store
  const { communities, communityImages, communityPlatforms, streamMode, setCommunities, setCommunityImages, setCommunityPlatforms, setStreamMode, setUsername } = useAppStore();
  const hydrateTheme = useThemeStore((s) => s.hydrate);
  const loadThemes = useThemeStore((s) => s.loadThemes);

  const [selectedCommunity, setSelectedCommunity] = useState<string | null>(null)
  const activeProfile = profiles.find((profile) => profile.id === activeProfileId) ?? null
  const activeProfileModSignature = useMemo(() => activeProfile?.mods
    .map(mod => `${mod.uuid4}:${mod.fullName}:${mod.versionNumber}:${mod.source ?? ''}`)
    .join('|') ?? '', [activeProfile?.mods])
  const [activeProfileGamePath, setActiveProfileGamePath] = useState<string | null>(null)
  const [isCheckingActiveProfileGamePath, setIsCheckingActiveProfileGamePath] = useState(false)
  const [runtimeHealth, setRuntimeHealth] = useState<RuntimeHealth | null>(null)
  const [isRepairingRuntime, setIsRepairingRuntime] = useState(false)
  const [pendingProfileUpdates, setPendingProfileUpdates] = useState<ProfileModUpdate[]>([])
  const [isUpdatingProfile, setIsUpdatingProfile] = useState(false)
  const [storageVolumeEventCount, setStorageVolumeEventCount] = useState(0)

  async function checkForUpdates() {
    const UPDATE_CHECK_INTERVAL_MS = 12 * 60 * 60 * 1000;
    const LAST_UPDATE_CHECK_KEY = 'r2modmac:lastUpdateCheck';
    try {
      const lastCheckRaw = localStorage.getItem(LAST_UPDATE_CHECK_KEY);
      if (lastCheckRaw) {
        const lastCheck = parseInt(lastCheckRaw, 10);
        if (!Number.isNaN(lastCheck) && Date.now() - lastCheck < UPDATE_CHECK_INTERVAL_MS) {
          return;
        }
      }

      const ver = await getVersion();
      const info = await window.ipcRenderer.checkUpdate(ver);
      localStorage.setItem(LAST_UPDATE_CHECK_KEY, Date.now().toString());
      if (info.available) {
        setUpdateInfo(info);
        setShowUpdateModal(true);
      }
    } catch (e) {
      console.warn("Update check skipped", e);
    }
  };

  async function forceCheckForUpdates() {
    try {
      const ver = await getVersion();
      const info = await window.ipcRenderer.checkUpdate(ver);
      if (info.available) {
        setUpdateInfo(info);
        setShowUpdateModal(true);
      } else {
        await window.ipcRenderer.alert("No Update Available", "You are on the latest version.");
      }
    } catch (e) {
      console.error("Update check failed", e);
      await window.ipcRenderer.alert("Error", `Failed to check for updates: ${e}`);
    }
  }

  async function loadData(refresh: boolean = true) {
    if (isInitialLoadRunningRef.current) return;
    if (fullCommunitiesLoadDoneRef.current) return;

    isInitialLoadRunningRef.current = true;
    fullCommunitiesLoadDoneRef.current = true;
    setLoading(true)
    try {
      // Both requests go out together: the game list and the cover map are
      // independent, and awaiting one before starting the other made startup
      // wait for the sum of two round trips instead of the longer of the two.
      // The images promise carries its own catch so a failure there still
      // leaves us with a usable game list — which is why this is not a plain
      // Promise.all. (The default-game path already did this correctly.)
      const communitiesPromise = window.ipcRenderer.fetchCommunities(refresh);
      const imagesPromise = window.ipcRenderer.fetchCommunityImages(refresh).catch((imgErr) => {
        console.warn('[community-images] failed to fetch image map, using cached mac images', imgErr);
        return null;
      });

      const data = await communitiesPromise;
      setCommunities(data)
      console.log(`[communities] loaded ${data.length} communities`);

      const storedPlatformCache = readMacPlatformCache();
      const storedImageCache = readMacImageCache();
      const knownGamesSet = new Set(storedPlatformCache.known_games);
      let sessionImages: Record<string, string> = {};

      const fetchedImages = await imagesPromise;
      if (fetchedImages) {
        sessionImages = fetchedImages;
        setCommunityImages(sessionImages);
      }

      const defaultPlatforms: Record<string, CommunityPlatformInfo> = Object.fromEntries(
        data.map((c: any) => {
          const cachedMacPlatform = storedPlatformCache.mac_platforms[c.identifier];
          const wasKnownBefore = knownGamesSet.has(c.identifier);
          const useQuickHint = !wasKnownBefore && QUICK_MAC_HINTS.has(c.identifier);

          if (cachedMacPlatform?.mac) {
            return [c.identifier, normalizePlatformInfo(cachedMacPlatform)];
          }

          return [c.identifier, {
            windows: true,
            mac: useQuickHint,
            linux: false,
            confidence: useQuickHint ? 0.55 : 0,
            source: useQuickHint ? 'bootstrap:quick_mac_hint' : 'bootstrap:unknown',
          }];
        })
      );

      let mergedPlatforms: Record<string, CommunityPlatformInfo> = { ...defaultPlatforms };
      setCommunityPlatforms(mergedPlatforms);

      const initialMacImages: Record<string, string> = { ...sessionImages };
      for (const [communityId, imageUrl] of Object.entries(storedImageCache.mac_images)) {
        if (!initialMacImages[communityId] && mergedPlatforms[communityId]?.mac) {
          initialMacImages[communityId] = imageUrl;
        }
      }
      setCommunityImages(initialMacImages);
      setLoading(false)

      const newCommunities = data.filter((c: any) => !knownGamesSet.has(c.identifier));
      if (newCommunities.length > 0) {
        console.log(`[platform-resolver] resolving ${newCommunities.length} new communities`);
      }

      const batchSize = 18;
      for (let i = 0; i < newCommunities.length; i += batchSize) {
        const batch = newCommunities.slice(i, i + batchSize).map((c: any) => ({
          identifier: c.identifier,
          name: c.name,
        }));
        try {
          const resolved = await window.ipcRenderer.resolveCommunityPlatforms(batch) as Record<string, CommunityPlatformInfo>;
          const nextMerged: Record<string, CommunityPlatformInfo> = { ...mergedPlatforms };
          for (const [communityId, incoming] of Object.entries(resolved)) {
            const normalizedIncoming = normalizePlatformInfo(incoming);
            const existing = nextMerged[communityId];
            nextMerged[communityId] = existing
              ? mergePlatformInfo(existing, normalizedIncoming)
              : normalizedIncoming;
            knownGamesSet.add(communityId);
          }

          mergedPlatforms = nextMerged;
          setCommunityPlatforms(mergedPlatforms);
          const macCount = Object.values(mergedPlatforms).filter((p: any) => p.mac).length;
          console.log(`[platform-resolver] processed ${Math.min(i + batchSize, newCommunities.length)}/${newCommunities.length} new, mac-compatible total: ${macCount}`);
        } catch (e) {
          console.warn('[platform-resolver] batch failed', e);
        }
      }

      const currentGameIds = data.map((c: any) => c.identifier as string);
      const macPlatformsToPersist: Record<string, CommunityPlatformInfo> = {};
      for (const gameId of currentGameIds) {
        const platform = mergedPlatforms[gameId];
        if (platform?.mac) {
          macPlatformsToPersist[gameId] = normalizePlatformInfo(platform);
        }
      }
      writeMacPlatformCache({
        version: 1,
        known_games: currentGameIds.filter((id) => knownGamesSet.has(id)),
        mac_platforms: macPlatformsToPersist,
        updated_at: Date.now(),
      });

      const macIds = currentGameIds.filter((id) => mergedPlatforms[id]?.mac);
      const missingSet = new Set(storedImageCache.missing_ids);
      const macImages: Record<string, string> = {};
      for (const id of macIds) {
        const cachedImage = storedImageCache.mac_images[id];
        if (cachedImage) {
          macImages[id] = cachedImage;
        }
      }

      for (const id of macIds) {
        const liveImage = sessionImages[id];
        if (liveImage) {
          macImages[id] = liveImage;
          missingSet.delete(id);
          continue;
        }
        if (!macImages[id]) {
          missingSet.add(id);
        }
      }
      console.log(`[community-images] stored ${Object.keys(macImages).length}/${macIds.length} mac images`);

      const persistedMissingIds = macIds.filter((id) => !macImages[id] && missingSet.has(id));
      writeMacImageCache({
        version: 1,
        mac_images: macImages,
        missing_ids: persistedMissingIds,
        updated_at: Date.now(),
      });
      if (Object.keys(sessionImages).length > 0) {
        setCommunityImages(sessionImages);
      } else {
        setCommunityImages(initialMacImages);
      }
      return;
    } catch (err) {
      console.error('Failed to load data', err)
    } finally {
      isInitialLoadRunningRef.current = false;
      setLoading(false)
    }
  }

  // Startup "default game" skip: populate only what that one game's Browse
  // Mods screen needs, with zero network activity. fetchCommunities(false)/
  // fetchCommunityImages(false) read the on-disk cache only (no live fetch,
  // no background refresh) — cheap even though the cache covers every game,
  // since it's a local read either way. resolveCommunityPlatforms is called
  // for just this one game rather than the full catalog. The full catalog
  // (with a live refresh) is deferred to the `else if (previous)` branch in
  // the selectedCommunity effect below, which only fires if the user
  // actually goes back to the home screen.
  async function loadEssentialsForDefaultGame(gameId: string) {
    try {
      const [comms, images] = await Promise.all([
        window.ipcRenderer.fetchCommunities(false),
        window.ipcRenderer.fetchCommunityImages(false),
      ]);
      setCommunities(comms);
      setCommunityImages(images);

      const target = comms.find((c) => c.identifier === gameId);
      if (!target) return;
      try {
        const resolved = await window.ipcRenderer.resolveCommunityPlatforms([
          { identifier: target.identifier, name: target.name },
        ]) as Record<string, CommunityPlatformInfo>;
        const info = resolved[target.identifier];
        if (info) {
          setCommunityPlatforms({ [target.identifier]: normalizePlatformInfo(info) });
        }
      } catch (err) {
        console.warn('[loadEssentialsForDefaultGame] platform resolution failed', err);
      }
    } catch (err) {
      console.error('[loadEssentialsForDefaultGame] failed', err);
    }
  }

  async function loadPackages(communityId: string, pageNum: number, reset: boolean = false, silent: boolean = false) {
    const requestId = ++packagesLoadRequestRef.current;
    const isStaleRequest = () => requestId !== packagesLoadRequestRef.current;

    if (reset) {
      if (!silent) {
        setLoadingMods(true);
        setAllPackages([]);
        setTotalPackages(0);
        setCurrentPage(0);
        setAvailableCategories([]);
      }
    }

    const withTimeout = async <T,>(promise: Promise<T>, ms: number, label: string): Promise<T> => {
      let timeoutId: ReturnType<typeof setTimeout> | undefined;
      try {
        return await Promise.race([
          promise,
          new Promise<T>((_, reject) => {
            timeoutId = setTimeout(() => reject(new Error(`${label} timed out after ${Math.round(ms / 1000)}s`)), ms);
          }),
        ]);
      } finally {
        if (timeoutId) clearTimeout(timeoutId);
      }
    };

    try {
      if (pageNum === 0 && reset) {
        await withTimeout(
          window.ipcRenderer.fetchPackages(communityId),
          45_000,
          'Package fetch'
        );
        if (isStaleRequest()) return;
        rebuildProfilePackageIndex(communityId);
        // Now that cache is populated, fetch available categories
        const cats = await withTimeout(
          window.ipcRenderer.getAvailableCategories(communityId),
          10_000,
          'Category fetch'
        );
        if (isStaleRequest()) return;
        setAvailableCategories(cats)
      }

      const response = await withTimeout(
        window.ipcRenderer.getPackages(
          communityId,
          pageNum,
          PAGE_SIZE,
          searchQuery,
          filterOptions.sort,
          filterOptions.nsfw,
          filterOptions.deprecated,
          filterOptions.sortDirection,
          filterOptions.categories,
          filterOptions.mods,
          filterOptions.modpacks
        ),
        20_000,
        'Package query'
      );
      if (isStaleRequest()) return;

      if (reset) {
        setAllPackages(response.items);
      } else {
        setAllPackages(prev => [...prev, ...response.items]);
      }
      setTotalPackages(response.total);
      setCurrentPage(pageNum);
    } catch (err) {
      if (isStaleRequest()) return;
      console.error('Failed to load packages', err)
      const msg = err instanceof Error ? err.message : String(err);
      if (msg.toLowerCase().includes('timed out')) {
        await window.ipcRenderer.alert(
          'Network Timeout',
          'Thunderstore took too long to respond. Please retry in a few seconds.'
        );
      }
    } finally {
      if (requestId === packagesLoadRequestRef.current && !silent) {
        setLoadingMods(false)
      }
    }
  }

  useEffect(() => {
    if (!activeProfile) {
      setTimeout(() => setIsGameRunning(false), 0)
      return
    }

    let cancelled = false
    const pollRunningState = async () => {
      try {
        const running = await window.ipcRenderer.isGameRunning(activeProfile.gameIdentifier, activeProfile.platform)
        if (!cancelled) {
          if (running) {
            launchGraceUntilRef.current = 0
            setIsGameRunning(true)
          } else if (Date.now() >= launchGraceUntilRef.current) {
            setIsGameRunning(false)
          }
        }
      } catch {
        if (!cancelled) {
          if (Date.now() >= launchGraceUntilRef.current) {
            setIsGameRunning(false)
          }
        }
      }
    }

    void pollRunningState()
    const intervalId = window.setInterval(() => {
      void pollRunningState()
    }, 1500)

    return () => {
      cancelled = true
      window.clearInterval(intervalId)
    }
  }, [activeProfile?.gameIdentifier, activeProfile?.platform])

  useEffect(() => {
    if (!activeProfile) {
      setTimeout(() => {
        setActiveProfileGamePath(null)
        setIsCheckingActiveProfileGamePath(false)
      }, 0)
      return
    }

    let cancelled = false
    const checkGamePath = async () => {
      setIsCheckingActiveProfileGamePath(true)
      try {
        const path = await window.ipcRenderer.getGamePath(activeProfile.gameIdentifier, activeProfile.platform)
        if (!cancelled) {
          setActiveProfileGamePath(typeof path === 'string' && path.trim().length > 0 ? path : null)
        }
      } catch {
        if (!cancelled) {
          setActiveProfileGamePath(null)
        }
      } finally {
        if (!cancelled) {
          setIsCheckingActiveProfileGamePath(false)
        }
      }
    }

    void checkGamePath()

    return () => {
      cancelled = true
    }
  }, [activeProfile?.gameIdentifier, activeProfile?.platform, showSettings, storageVolumeEventCount])

  useEffect(() => {
    setTimeout(() => {
      loadProfiles()
      checkForUpdates()
    }, 0);

    // Load app preferences
    window.ipcRenderer.getSettings().then((s: AppSettings) => {
      setLegacyInstallMode(!!s.legacy_install_mode);
      setAskVersionBeforeInstall(s.ask_version_before_install ?? false);
      setInstallInParallel(s.install_in_parallel ?? true);
      setConfirmBeforeApplyToGame(!!s.confirm_before_apply_to_game);
      setWriteDebugLogsToGame(s.write_debug_logs_to_game ?? false);
      setVerboseLogging(s.verbose_logging ?? false);
      // Apply the persisted level immediately so early-session logs honour it.
      void window.ipcRenderer.setVerboseLogging(s.verbose_logging ?? false);
      const storedViewMode = s.default_mod_view_mode === 'list' ? 'list' : 'grid';
      setDefaultModViewMode(storedViewMode);
      setViewMode(storedViewMode);
      setShowDeprecatedWarnings(s.show_deprecated_warnings ?? true);
      setSponsoredMessagesEnabled(s.sponsored_messages_enabled ?? true);
      setSponsoredMessagesScale(s.sponsored_messages_scale ?? 80);
      setSponsoredMessagesOpacity(s.sponsored_messages_background_opacity ?? 80);
      setHideCrossOverGuide(!!s.hide_crossover_guide);
      setHideVerboseLogsWarning(!!s.hide_verbose_logs_warning);
      setStreamMode(!!s.stream_mode);
      setDefaultGame(s.default_game ?? null);
      setDefaultProfile(s.default_profile ?? null);
      useKeybindStore.getState().hydrate(s.keybinds);
      startupDefaultGameRef.current = s.default_game ?? null;
      startupDefaultProfileRef.current = s.default_profile ?? null;
      // Paint the saved theme as early as the settings arrive, so the window
      // settles into its colours instead of changing them under the user.
      void hydrateTheme(s.active_theme ?? null);

      // Skipping straight to a game: fetch only that game's essentials
      // (cache-only, no network). Otherwise: full catalog load as before.
      if (s.default_game) {
        void loadEssentialsForDefaultGame(s.default_game);
      } else {
        void loadData(true);
      }
      setSettingsHydrated(true);
    });

    window.ipcRenderer.getUsername().then((u: string) => {
      setUsername(u);
    }).catch((err) => {
      console.error('Failed to get username', err);
    });

    // Listen for preferences menu event
    const unlistenPrefs = listen('show-preferences', () => {
      setPreferencesPanel(null);
      setShowPreferences(true);
    });

    const unlistenStorageVolumes = listen('storage-volumes-changed', () => {
      setStorageVolumeEventCount((count) => count + 1);
    });

    // A theme edited in an external editor repaints the app on save, which is
    // what makes hand-editing the TOML a first-class way to work.
    const unlistenThemes = listen('themes-changed', () => {
      void loadThemes();
    });

    const unlistenSteamLaunchOptionsRestart = listen('steam-launch-options-restart', () => {
      steamRestartingRef.current = true;
      setIsSteamRestarting(true);
      void window.ipcRenderer.alert(
        'Restarting Steam',
        'Steam is being restarted to apply the launch options for this profile.'
      );
    });

    return () => {
      unlistenPrefs.then(fn => fn());
      unlistenStorageVolumes.then(fn => fn());
      unlistenThemes.then(fn => fn());
      unlistenSteamLaunchOptionsRestart.then(fn => fn());
    };
  }, [])

  useEffect(() => {
    if (!settingsHydrated || !verboseLogging || hideVerboseLogsWarning || verboseLogsWarningDismissed) {
      return;
    }

    let cancelled = false;
    const checkLogSize = async () => {
      try {
        const bytes = await window.ipcRenderer.getAppLogsSize();
        if (!cancelled) {
          setVerboseLogsWarningBytes(bytes >= VERBOSE_LOG_WARNING_BYTES ? bytes : null);
        }
      } catch (error) {
        console.warn('Could not inspect application log storage:', error);
      }
    };

    void checkLogSize();
    const interval = window.setInterval(checkLogSize, VERBOSE_LOG_SIZE_CHECK_INTERVAL_MS);
    return () => {
      cancelled = true;
      window.clearInterval(interval);
    };
  }, [settingsHydrated, verboseLogging, hideVerboseLogsWarning, verboseLogsWarningDismissed]);

  // Jump straight to the default game's profile list on startup, skipping the
  // game selection screen. Runs once, using the value settings had at launch
  // (startupDefaultGameRef) rather than the live `defaultGame` state, so
  // saving a new default in Preferences mid-session never yanks the user
  // somewhere else — it only takes effect on the next launch.
  //
  // Gated on settings alone, deliberately NOT on `communities`: getSettings()
  // is a local disk read (a few ms), while communities is a real Thunderstore
  // fetch — waiting for it here was the entire reason the skip felt slow
  // instead of instant. Downstream code already tolerates an unrecognized
  // `selectedCommunity` (falls back to undefined via optional chaining), and
  // the validation effect below self-corrects if the stored identifier turns
  // out to be stale once communities actually arrives.
  useEffect(() => {
    if (defaultGameAppliedRef.current) return;
    if (startupDefaultGameRef.current === undefined) return;
    defaultGameAppliedRef.current = true;
    const target = startupDefaultGameRef.current;
    if (!target) return;

    const targetProfileName = startupDefaultProfileRef.current;
    if (!targetProfileName) {
      // No profile to also land on — go straight to this game's profile list.
      setSelectedCommunity(target);
      return;
    }

    // Resolve the profile BEFORE setting selectedCommunity, and set both in
    // the same tick, so the very first render already has both — otherwise
    // selectedCommunity alone would paint the profile-list screen for one
    // frame before activeProfileId caught up and swapped it to Browse Mods.
    // getProfiles() is a local disk read (same class as getSettings()), so
    // this costs no perceptible time.
    window.ipcRenderer.getProfiles()
      .then((allProfiles) => {
        const match = allProfiles.find(p => p.gameIdentifier === target && p.name === targetProfileName);
        setSelectedCommunity(target);
        if (match) selectProfile(match.id);
      })
      .catch((err) => {
        console.error('Failed to auto-select default profile', err);
        setSelectedCommunity(target);
      });
  }, [defaultGame, selectProfile])

  // Safety net for the effect above: once communities has actually loaded, if
  // the default game we jumped to on faith doesn't exist (renamed/removed
  // Thunderstore community), back out to the normal game selection screen
  // instead of leaving the user stranded on a nameless, image-less game page.
  useEffect(() => {
    if (defaultGameValidatedRef.current) return;
    if (!defaultGameAppliedRef.current || communities.length === 0) return;
    defaultGameValidatedRef.current = true;
    const target = startupDefaultGameRef.current;
    if (target && !communities.some(c => c.identifier === target)) {
      setSelectedCommunity(null);
    }
  }, [communities])

  const clearSteamRestartingState = () => {
    steamRestartingRef.current = false;
    setIsSteamRestarting(false);
  };

  const beginLaunchGraceWindow = (milliseconds: number = 20000) => {
    launchGraceUntilRef.current = Date.now() + milliseconds;
  };

  const clearLaunchGraceWindow = () => {
    launchGraceUntilRef.current = 0;
  };

  const waitForLaunchStateToSettle = async (
    gameIdentifier: string,
    platform?: 'windows' | 'mac'
  ) => {
    const shouldPollLonger = steamRestartingRef.current;
    const deadline = Date.now() + (shouldPollLonger ? 12000 : 7000);

    while (Date.now() < deadline) {
      try {
        const running = await window.ipcRenderer.isGameRunning(gameIdentifier, platform);
        if (running) {
          clearSteamRestartingState();
          clearLaunchGraceWindow();
          return true;
        }
      } catch {
        // Keep polling until deadline; transient query failures should not flip UI immediately.
      }

      await new Promise((resolve) => setTimeout(resolve, 300));
    }

    clearSteamRestartingState();
    return false;
  };

  // Keep a stable ref to selectedCommunity so the packages-loaded listener
  // doesn't need to re-register on every game change (avoids stale closures).
  const selectedCommunityRef = useRef<string | null>(null);
  useEffect(() => { selectedCommunityRef.current = selectedCommunity; }, [selectedCommunity]);

  useEffect(() => {
    const onContextMenu = async (event: MouseEvent) => {
      if (event.defaultPrevented) return;

      event.preventDefault();

      const { Menu: M, MenuItem: MI } = await import('@tauri-apps/api/menu');

      const items = [];
      items.push(await MI.new({
        text: 'Reload',
        action: () => {
          window.location.reload();
        }
      }));

      if (SHOW_DEVTOOLS_CONTEXT_MENU_ITEM) {
        items.push(await MI.new({
          text: 'Inspect Element',
          action: async () => {
            try {
              await invoke('open_devtools');
            } catch (error) {
              console.error('Failed to open devtools', error);
            }
          }
        }));
      }

      const menu = await M.new({ items });
      await menu.popup();
    };

    window.addEventListener('contextmenu', onContextMenu);

    return () => {
      window.removeEventListener('contextmenu', onContextMenu);
    };
  }, []);

  const rebuildProfilePackageIndex = useCallback(async (communityId: string) => {
    const requestId = ++profilePackageIndexRequestRef.current;
    const { profiles, activeProfileId } = useProfileStore.getState();
    const profile = profiles.find((p) => p.id === activeProfileId) ?? null;
    if (!profile || profile.mods.length === 0) {
      setProfilePackageIndex({});
      return;
    }

    const modNames = Array.from(new Set(profile.mods
      .filter(m => m.source !== 'local')
      .map(m => parsePackageReference(m.fullName).packageName)));

    if (modNames.length === 0) {
      setProfilePackageIndex({});
      return;
    }

    try {
      let result: Awaited<ReturnType<typeof window.ipcRenderer.lookupPackagesByNames>> | null = null;
      let lastError: unknown;
      for (let attempt = 0; attempt < 3 && !result; attempt++) {
        try {
          result = await window.ipcRenderer.lookupPackagesByNames(communityId, modNames);
        } catch (error) {
          lastError = error;
          if (attempt === 0) {
            // A freshly imported profile can render before the package cache is
            // present in backend memory. Load it explicitly; an unchanged CDN
            // index does not emit packages-loaded, so relying on that event can
            // otherwise leave the update badge empty for the whole session.
            await window.ipcRenderer.fetchPackages(communityId);
          } else {
            await new Promise(resolve => setTimeout(resolve, 250 * attempt));
          }
        }
        if (requestId !== profilePackageIndexRequestRef.current) return;
      }
      if (!result) throw lastError || new Error('Package index lookup failed');
      const index: Record<string, Package> = {};
      if (result.found) {
        for (const pkg of result.found) {
          index[pkg.full_name.toLowerCase()] = pkg;
        }
      }
      const currentState = useProfileStore.getState();
      if (requestId === profilePackageIndexRequestRef.current && currentState.activeProfileId === profile.id) {
        setProfilePackageIndex(index);
      }
    } catch (err) {
      console.error('Failed to build profile package index', err);
      if (requestId === profilePackageIndexRequestRef.current) setProfilePackageIndex({});
    }
  }, []);

  useEffect(() => {
    const unlistenPackages = listen<{ game_id: string, total_count: number }>('packages-loaded', (event) => {
      console.log(`[packages-loaded] Game ${event.payload.game_id} now has ${event.payload.total_count} packages`);
      const current = selectedCommunityRef.current;
      if (current && event.payload.game_id === current) {
        loadPackages(current, 0, true, true);
        rebuildProfilePackageIndex(current);
      }
    });

    return () => {
      unlistenPackages.then(fn => fn());
    };
  }, [])



  const previousSelectedCommunityForProfileResetRef = useRef<string | null>(null);
  useEffect(() => {
    const previous = previousSelectedCommunityForProfileResetRef.current;
    previousSelectedCommunityForProfileResetRef.current = selectedCommunity;
    if (selectedCommunity) {
      // Initial load for game (categories now fetched inside loadPackages after cache is populated)
      setTimeout(() => {
        loadPackages(selectedCommunity, 0, true)
        // Reset profile selection only when actually SWITCHING from one game
        // to a different one. Arriving at a game from nothing (previous is
        // null — home screen selection, or the startup default-game/profile
        // skip) must not clear activeProfileId: the skip path can set both
        // selectedCommunity and activeProfileId together in the same tick
        // specifically to land straight on Browse Mods, and this reset would
        // otherwise silently undo it a moment later.
        if (previous && previous !== selectedCommunity && activeProfileId) {
          selectProfile('')
        }
      }, 0);
    } else if (previous) {
      // `previous` truthy means we just transitioned AWAY from a real game
      // back to the home screen (manual "Change Game", Escape key, or the
      // safety-net effect reverting an invalid default). This can never be
      // true on the initial mount, where `previous` starts at null — so the
      // full, live-refreshing catalog load only fires here, never on every
      // launch. It's the one place that load is owed when startup took the
      // default-game skip path (cache-only, single-game view). No-ops if
      // loadData() already ran in full this session (fullCommunitiesLoadDoneRef).
      void loadData(true);
    }
  }, [selectedCommunity])

  // Server-Side Search mapping
  const packages = allPackages;

  // Search Debounce Effect
  useEffect(() => {
    if (!selectedCommunity) return;
    const timer = setTimeout(() => {
      loadPackages(selectedCommunity, 0, true);
    }, 300);
    return () => clearTimeout(timer);
  }, [searchQuery]);

  // Sort/Filter Effect
  useEffect(() => {
    if (selectedCommunity) {
      setTimeout(() => {
        loadPackages(selectedCommunity, 0, true)
      }, 0);
    }
  }, [filterOptions])

  // Build package index for profile mods (only fetch installed mods, not all packages)
  useEffect(() => {
    if (selectedCommunity) {
      const timer = setTimeout(() => {
        rebuildProfilePackageIndex(selectedCommunity);
      }, 0);
      return () => clearTimeout(timer);
    }
  }, [activeProfile?.id, selectedCommunity, activeProfileModSignature, rebuildProfilePackageIndex])



  const handleSelectProfile = (profileId: string) => {
    setIsBrowsingMode(false);
    selectProfile(profileId);
  };

  // One source of truth for every way Spotlight is opened from the profile
  // selection page. The toolbar button and the keyboard shortcut must carry
  // the exact same game context.
  const selectedGamePaletteScope = selectedCommunity
    ? {
        group: 'Profiles' as const,
        game: {
          identifier: selectedCommunity,
          name: communities.find((community) => community.identifier === selectedCommunity)?.name ?? selectedCommunity,
          image: communityImages[selectedCommunity],
        },
      }
    : undefined;

  const activeProfilePaletteScope = !isBrowsingMode && activeProfile && selectedGamePaletteScope?.game
    ? {
        ...selectedGamePaletteScope,
        profile: {
          identifier: activeProfile.id,
          name: activeProfile.name,
          image: activeProfile.profileImageUrl,
          initial: getProfileInitial(activeProfile.name),
          gradient: getProfileAvatarGradient(activeProfile.name, activeProfile.id),
        },
      }
    : undefined;

  const runProfileCommand = (profileId: string, gameIdentifier: string, command: ProfileCommand) => {
    // Adding a profile tag must not navigate. Navigation is deferred until an
    // actual action is chosen; after the profile screen mounts, it supplies the
    // exact same handlers used by its buttons and keyboard shortcuts.
    setSelectedCommunity(gameIdentifier);
    handleSelectProfile(profileId);
    setPendingProfileCommand({ command });
  };

  // Everything reachable regardless of where the user is standing: the games
  // list, every profile, and the panels that would otherwise take several
  // clicks. Registered from here because these are App's own to perform.
  useCommandSource('app', (scope) => {
    // Do not build the whole catalogue only to discard it below. Universal
    // search needs everything; a pinned search needs just its current branch.
    const visibleCommunities = scope ? [] : communities;
    const visibleProfiles = scope?.profile
      ? []
      : scope?.game
        ? profiles.filter((profile) => profile.gameIdentifier === scope.game!.identifier)
        : profiles;
    const contextualCommunities = scope?.game
      ? communities.filter((community) => community.identifier === scope.game!.identifier)
      : [];

    const games: CommandItem[] = visibleCommunities.map((community) => ({
      id: `game:${community.identifier}`,
      title: community.name,
      subtitle: community.identifier,
      group: 'Games',
      icon: 'game',
      image: communityImages[community.identifier],
      game: community.identifier,
      nextScope: {
        group: 'Profiles',
        game: {
          identifier: community.identifier,
          name: community.name,
          image: communityImages[community.identifier],
        },
      },
      current: community.identifier === selectedCommunity,
      run: () => {
        selectProfile('');
        setIsBrowsingMode(false);
        setSelectedCommunity(community.identifier);
      },
    }));

    const profileItems: CommandItem[] = visibleProfiles.map((profile) => {
      const game = communities.find((c) => c.identifier === profile.gameIdentifier);
      return {
        id: `profile:${profile.id}`,
        title: profile.name,
        // A profile name means little on its own once several games are in
        // play, so the game it belongs to travels with it.
        subtitle: game?.name ?? profile.gameIdentifier,
        group: 'Profiles',
        icon: 'profile',
        // The game's cover says which game, the badge says which profile.
        image: communityImages[profile.gameIdentifier],
        badge: {
          image: profile.profileImageUrl,
          initial: getProfileInitial(profile.name),
          gradient: getProfileAvatarGradient(profile.name, profile.id),
        },
        game: profile.gameIdentifier,
        profile: profile.id,
        nextScope: {
          group: 'Profiles',
          game: {
            identifier: profile.gameIdentifier,
            name: game?.name ?? profile.gameIdentifier,
            image: communityImages[profile.gameIdentifier],
          },
          profile: {
            identifier: profile.id,
            name: profile.name,
            image: profile.profileImageUrl,
            initial: getProfileInitial(profile.name),
            gradient: getProfileAvatarGradient(profile.name, profile.id),
          },
        },
        current: profile.id === activeProfileId,
        run: () => {
          setSelectedCommunity(profile.gameIdentifier);
          handleSelectProfile(profile.id);
        },
      };
    });

    const gameActions: CommandItem[] = contextualCommunities.flatMap((community) => [
      {
        id: `action:browse:${community.identifier}`,
        title: 'Browse mods',
        subtitle: community.name,
        group: 'Actions',
        icon: 'browse',
        game: community.identifier,
        contextOnly: true,
        run: () => {
          selectProfile('');
          setSelectedCommunity(community.identifier);
          setIsBrowsingMode(true);
        },
      },
      {
        id: `action:import-profile:${community.identifier}`,
        title: 'Import profile',
        subtitle: 'From a code or a file',
        group: 'Actions',
        icon: 'import',
        game: community.identifier,
        contextOnly: true,
        run: () => {
          selectProfile('');
          setIsBrowsingMode(false);
          setSelectedCommunity(community.identifier);
          requestAnimationFrame(() => {
            window.dispatchEvent(new CustomEvent('r2modmac:open-profile-action', { detail: 'import' }));
          });
        },
      },
      {
        id: `action:new-profile:${community.identifier}`,
        title: 'New profile',
        subtitle: `for ${community.name}`,
        group: 'Actions',
        icon: 'plus',
        game: community.identifier,
        contextOnly: true,
        hint: formatAccelerator(activeKeybinds['new-profile']),
        shortcut: 'new-profile',
        run: () => {
          selectProfile('');
          setIsBrowsingMode(false);
          setSelectedCommunity(community.identifier);
          requestAnimationFrame(() => {
            window.dispatchEvent(new CustomEvent('r2modmac:open-profile-action', { detail: 'new' }));
          });
        },
      },
    ] as CommandItem[]);

    const scopedProfile = scope?.profile
      ? profiles.find((profile) => profile.id === scope.profile!.identifier)
      : null;
    const profileActions: CommandItem[] = scopedProfile && scopedProfile.id !== activeProfileId
      ? [
          {
            id: 'action:apply',
            title: 'Apply mods to game',
            subtitle: scopedProfile.name,
            group: 'Actions',
            icon: 'apply',
            game: scopedProfile.gameIdentifier,
            profile: scopedProfile.id,
            hint: formatAccelerator(activeKeybinds['apply-mods']),
            shortcut: 'apply-mods',
            run: () => runProfileCommand(scopedProfile.id, scopedProfile.gameIdentifier, 'apply'),
          },
          {
            id: 'action:launch',
            title: 'Launch game (modded)',
            subtitle: scopedProfile.name,
            group: 'Actions',
            icon: 'play',
            game: scopedProfile.gameIdentifier,
            profile: scopedProfile.id,
            hint: formatAccelerator(activeKeybinds['launch-modded']),
            shortcut: 'launch-modded',
            run: () => runProfileCommand(scopedProfile.id, scopedProfile.gameIdentifier, 'launch'),
          },
          {
            id: 'action:launch-vanilla',
            title: 'Launch game (unmodded)',
            group: 'Actions',
            icon: 'play',
            game: scopedProfile.gameIdentifier,
            profile: scopedProfile.id,
            hint: formatAccelerator(activeKeybinds['launch-vanilla']),
            shortcut: 'launch-vanilla',
            run: () => runProfileCommand(scopedProfile.id, scopedProfile.gameIdentifier, 'launch-vanilla'),
          },
          {
            id: 'action:stop',
            title: 'Quit game',
            group: 'Actions',
            icon: 'stop',
            game: scopedProfile.gameIdentifier,
            profile: scopedProfile.id,
            hint: formatAccelerator(activeKeybinds['stop-game']),
            shortcut: 'stop-game',
            run: () => runProfileCommand(scopedProfile.id, scopedProfile.gameIdentifier, 'stop'),
          },
          {
            id: 'action:duplicate',
            title: 'Duplicate profile',
            subtitle: scopedProfile.name,
            group: 'Actions',
            icon: 'copy',
            game: scopedProfile.gameIdentifier,
            profile: scopedProfile.id,
            hint: formatAccelerator(activeKeybinds['duplicate-profile']),
            shortcut: 'duplicate-profile',
            run: () => runProfileCommand(scopedProfile.id, scopedProfile.gameIdentifier, 'duplicate'),
          },
          {
            id: 'action:export',
            title: 'Export profile',
            subtitle: scopedProfile.name,
            group: 'Actions',
            icon: 'file',
            game: scopedProfile.gameIdentifier,
            profile: scopedProfile.id,
            run: () => runProfileCommand(scopedProfile.id, scopedProfile.gameIdentifier, 'export'),
          },
        ]
      : [];

    const openPreferencesAt = (target: PreferencesTarget | null) => {
      setPreferencesPanel(target);
      setShowPreferences(true);
    };

    const settings: CommandItem[] = scope ? [] : [
      {
        id: 'settings:preferences',
        title: 'Preferences',
        group: 'Settings',
        icon: 'settings',
        run: () => openPreferencesAt(null),
      },
      {
        id: 'settings:theme',
        title: 'Theme',
        subtitle: 'Colours, background image and presets',
        group: 'Settings',
        icon: 'theme',
        run: () => openPreferencesAt('theme'),
      },
      {
        id: 'settings:shortcuts',
        title: 'Keyboard shortcuts',
        group: 'Settings',
        icon: 'keyboard',
        run: () => openPreferencesAt('keybinds'),
      },
      {
        id: 'settings:paths',
        title: 'Game paths and setup',
        group: 'Settings',
        icon: 'settings',
        run: () => setShowSettings(true),
      },
      {
        id: 'settings:check-updates',
        title: 'Check updates',
        subtitle: 'Preferences',
        group: 'Settings',
        icon: 'update',
        run: () => openPreferencesAt('updates'),
      },
      {
        id: 'settings:default-game',
        title: 'Default game and profile',
        subtitle: 'Startup behavior',
        group: 'Settings',
        icon: 'game',
        run: () => openPreferencesAt('default-game'),
      },
      {
        id: 'settings:legacy-install',
        title: 'Legacy install mode',
        subtitle: 'Install behavior',
        group: 'Settings',
        icon: 'install',
        run: () => openPreferencesAt('legacy-install'),
      },
      {
        id: 'settings:ask-version',
        title: 'Ask version before installing',
        subtitle: 'Install behavior',
        group: 'Settings',
        icon: 'version',
        run: () => openPreferencesAt('ask-version'),
      },
      {
        id: 'settings:parallel-downloads',
        title: 'Download mods in parallel',
        subtitle: 'Install behavior',
        group: 'Settings',
        icon: 'parallel',
        run: () => openPreferencesAt('parallel-downloads'),
      },
      {
        id: 'settings:confirm-apply',
        title: 'Confirm before apply to game',
        subtitle: 'Install behavior',
        group: 'Settings',
        icon: 'apply',
        run: () => openPreferencesAt('confirm-apply'),
      },
      {
        id: 'settings:debug-logs',
        title: 'Write debug logs to game folder',
        subtitle: 'Logging',
        group: 'Settings',
        icon: 'logs',
        run: () => openPreferencesAt('debug-logs'),
      },
      {
        id: 'settings:verbose-logs',
        title: 'Verbose app logging',
        subtitle: 'Logging',
        group: 'Settings',
        icon: 'logs',
        run: () => openPreferencesAt('verbose-logs'),
      },
      {
        id: 'settings:open-logs',
        title: 'Open app logs folder',
        subtitle: 'Logging',
        group: 'Settings',
        icon: 'folder',
        run: () => openPreferencesAt('open-logs'),
      },
      {
        id: 'settings:default-view',
        title: 'Default mods view',
        subtitle: 'Appearance',
        group: 'Settings',
        icon: 'layout',
        run: () => openPreferencesAt('default-view'),
      },
      {
        id: 'settings:stream-mode',
        title: 'Stream Mode',
        subtitle: 'Privacy',
        group: 'Settings',
        icon: 'stream',
        run: () => openPreferencesAt('stream-mode'),
      },
      {
        id: 'settings:sponsored-messages',
        title: 'Sponsored messages',
        subtitle: 'Support r2modmac',
        group: 'Settings',
        icon: 'support',
        run: () => openPreferencesAt('sponsored-messages'),
      },
      {
        id: 'settings:deprecated-warnings',
        title: 'Deprecated mod warnings',
        subtitle: 'Guides & alerts',
        group: 'Settings',
        icon: 'warning',
        run: () => openPreferencesAt('deprecated-warnings'),
      },
      {
        id: 'settings:restore-warnings',
        title: 'Restore setup warnings',
        subtitle: 'Guides & alerts',
        group: 'Settings',
        icon: 'warning',
        run: () => openPreferencesAt('restore-warnings'),
      },
      {
        id: 'settings:clear-cache',
        title: 'Clear app cache',
        subtitle: 'Storage',
        group: 'Settings',
        icon: 'cache',
        run: () => openPreferencesAt('clear-cache'),
      },
    ];

    return [...profileItems, ...games, ...gameActions, ...profileActions, ...settings];
  });

  const loadMorePackages = useCallback(() => {
    if (!selectedCommunity || isFetchingNextPage || loadingMods) return;
    setIsFetchingNextPage(true);
    loadPackages(selectedCommunity, currentPage + 1, false).finally(() => {
      setIsFetchingNextPage(false);
    });
  }, [selectedCommunity, currentPage, isFetchingNextPage, loadingMods]);





  // ── Mod Actions (install / uninstall / update) ───────────────────────────────
  const {
    installModWithDependencies,
    handleInstallMod,
    handleUninstallWithDependencies,
    executeUninstall,
    handleUpdateMod,
    stageProfileUpdates,
  } = useModActions({
    activeProfileId,
    selectedCommunity,
    legacyInstallMode,
    installInParallel,
    uninstallModalState,
    setProgressState,
    setUninstallModalState,
  });

  // ── Profile Actions (import / export) ────────────────────────────────────────
  const {
    handleImportProfile,
    handleImportFile,
    handleExportFile,
    handleExportCode,
  } = useProfileActions({
    selectedCommunity,
    activeProfileId,
    setProgressState,
    onInstallMod: (pkg, profileId) => handleInstallMod(pkg, profileId, undefined, true),
  });

  // ── Game Sync (Apply to Game) ─────────────────────────────────────────────────
  const { handleSyncToGame } = useGameSync({
    activeProfileId,
    selectedCommunity,
    legacyInstallMode,
    installInParallel,
    setProgressState,
    setShowCrossOverGuide,
    installModWithDependencies,
  });

  const refreshRuntimeHealth = useCallback(async (): Promise<RuntimeHealth | null> => {
    const profile = useProfileStore.getState().profiles.find(candidate => candidate.id === activeProfileId);
    if (!profile) {
      setRuntimeHealth(null);
      return null;
    }
    try {
      const result = await window.ipcRenderer.checkProfileRuntimeHealth(
        profile.id,
        profile.gameIdentifier,
        profile.platform
      );
      setRuntimeHealth(result);
      return result;
    } catch (error) {
      console.error('Failed to check profile runtime health', error);
      setRuntimeHealth(null);
      return null;
    }
  }, [activeProfileId]);

  useEffect(() => {
    const timer = window.setTimeout(() => { void refreshRuntimeHealth(); }, 0);
    return () => window.clearTimeout(timer);
  }, [activeProfileGamePath, refreshRuntimeHealth]);

  useEffect(() => {
    if (!activeProfile || activeProfile.apply_interrupted) return;
    const needsInspection = activeProfile.mods.some(mod => mod.pending_sync && mod.sync_baseline === undefined)
      || (!!activeProfile.needs_sync
        && !activeProfile.mods.some(mod => mod.pending_sync)
        && (activeProfile.pending_removals?.length ?? 0) === 0);
    if (!needsInspection) return;
    const requestKey = `${activeProfile.id}:${activeProfile.mods.length}:${activeProfile.mods.filter(mod => mod.pending_sync).length}`;
    if (syncInspectionRequestRef.current === requestKey) return;
    syncInspectionRequestRef.current = requestKey;
    void window.ipcRenderer.inspectProfileSyncState(
      activeProfile.id,
      activeProfile.gameIdentifier,
      activeProfile.platform,
    ).then(inspection => {
      if (inspection.status !== 'ready') return;
      const latest = useProfileStore.getState().profiles.find(profile => profile.id === activeProfile.id);
      if (!latest) return;
      updateProfile(latest.id, migratePendingSyncBaselines(latest, inspection));
    }).catch(error => {
      console.error('Failed to inspect synchronized profile state', error);
      syncInspectionRequestRef.current = null;
    });
  }, [activeProfile, updateProfile]);

  const repairProfileRuntime = useCallback(async (): Promise<boolean> => {
    const profile = useProfileStore.getState().profiles.find(candidate => candidate.id === activeProfileId);
    if (!profile || isRepairingRuntime) return false;
    const community = profile.gameIdentifier || selectedCommunity;
    if (!community) return false;

    const health = runtimeHealth || await refreshRuntimeHealth();
    if (health?.status === 'healthy' || hasPendingRuntimeInstall(profile, health?.runtime)) return true;
    if (!health || !health.repairable) {
      if (health?.status === 'unconfigured') setShowSettings(true);
      return false;
    }

    setIsRepairingRuntime(true);
    setProgressState({
      isOpen: true,
      title: 'Repairing Runtime',
      progress: 5,
      currentTask: 'Resolving the required loader...',
    });
    let repairTransactionStarted = false;

    try {
      const gamePath = await window.ipcRenderer.getGamePath(community, profile.platform);
      if (!gamePath) throw new Error('The game directory is not configured.');
      await window.ipcRenderer.beginProfileApplyTransaction(profile.id, community);
      repairTransactionStarted = true;

      const matchesRuntime = (pkg: Package) => {
        const name = `${pkg.full_name} ${pkg.name}`.toLowerCase();
        if (health.runtime === 'owml') return pkg.name.toLowerCase() === 'owml' || name.includes('-owml');
        if (health.runtime === 'lovely') return name.includes('thunderstore-lovely') || pkg.name.toLowerCase() === 'lovely';
        if (health.runtime === 'returnofmodding') {
          return pkg.full_name.toLowerCase() === 'returnofmodding-returnofmodding';
        }
        return name.includes('bepinexpack');
      };
      const registeredLoader = profile.mods.find(mod => {
        const name = mod.fullName.toLowerCase();
        if (health.runtime === 'owml') return name.includes('owml');
        if (health.runtime === 'lovely') return name.includes('-lovely-');
        if (health.runtime === 'returnofmodding') return name.startsWith('returnofmodding-returnofmodding-');
        return name.includes('bepinexpack');
      });

      let loaderPackage = registeredLoader
        ? await window.ipcRenderer.fetchPackageByName(
            parsePackageReference(registeredLoader.fullName).packageName,
            community
          )
        : null;
      if (!loaderPackage || !matchesRuntime(loaderPackage)) {
        const query = health.runtime === 'owml'
          ? 'OWML'
          : health.runtime === 'lovely'
            ? 'lovely'
            : health.runtime === 'returnofmodding'
              ? 'ReturnOfModding'
              : 'BepInExPack';
        const result = await window.ipcRenderer.getPackages(community, 0, 30, query, 'downloads');
        loaderPackage = result.items.find(matchesRuntime) || null;
      }
      if (!loaderPackage || loaderPackage.versions.length === 0) {
        throw new Error(`No compatible ${health.runtime} loader was found for this community.`);
      }

      const newestVersion = loaderPackage.versions.reduce((newest, candidate) =>
        compareVersions(candidate.version_number, newest.version_number) > 0 ? candidate : newest
      );
      const enabledByPackage = new Map(profile.mods.map(mod => [
        parsePackageReference(mod.fullName).packageName.toLowerCase(),
        mod.enabled,
      ]));

      setProgressState(previous => ({
        ...previous,
        progress: 25,
        currentTask: `Reinstalling ${loaderPackage.name} v${newestVersion.version_number}...`,
      }));
      await installModWithDependencies(loaderPackage, newestVersion, new Set(), profile.id, undefined, gamePath);

      const repairedProfile = useProfileStore.getState().profiles.find(candidate => candidate.id === profile.id);
      if (repairedProfile) {
        updateProfile(profile.id, {
          mods: repairedProfile.mods.map(mod => {
            const priorEnabled = enabledByPackage.get(parsePackageReference(mod.fullName).packageName.toLowerCase());
            return priorEnabled === undefined ? mod : { ...mod, enabled: priorEnabled };
          }),
        });
      }

      const checked = await refreshRuntimeHealth();
      if (checked?.status !== 'healthy') {
        throw new Error(
          `The loader was reinstalled, but the runtime is still ${checked?.status || 'unavailable'}` +
          (checked?.missingComponents.length ? ` (${checked.missingComponents.join(', ')}).` : '.')
        );
      }
      await window.ipcRenderer.commitProfileApplyTransaction(profile.id, profile.gameIdentifier);
      repairTransactionStarted = false;
      setProgressState(previous => ({ ...previous, progress: 100, currentTask: 'Runtime repaired.' }));
      window.setTimeout(() => setProgressState(previous => ({ ...previous, isOpen: false })), 500);
      return true;
    } catch (error: any) {
      setProgressState(previous => ({ ...previous, isOpen: false }));
      if (repairTransactionStarted) {
        try {
          await window.ipcRenderer.rollbackProfileApplyTransaction(profile.id, community);
        } catch (rollbackError) {
          console.error('Failed to roll back runtime repair', rollbackError);
        }
      }
      updateProfile(profile.id, {
        mods: profile.mods,
        needs_sync: profile.needs_sync,
        apply_interrupted: profile.apply_interrupted,
      });
      await refreshRuntimeHealth();
      await window.ipcRenderer.alert(
        'Runtime Repair Failed',
        String(error?.message || error || 'The runtime could not be repaired.')
      );
      return false;
    } finally {
      setIsRepairingRuntime(false);
    }
  }, [activeProfileId, installModWithDependencies, isRepairingRuntime, refreshRuntimeHealth, runtimeHealth, selectedCommunity, setProgressState, updateProfile]);

  const handleProfileModUpdate = useCallback(async (
    pkg: Package,
    targetProfileId?: string,
    version?: PackageVersion,
  ) => {
    try {
      await handleUpdateMod(pkg, targetProfileId, version);
      if (legacyInstallMode) {
        await handleSyncToGame(undefined, { silentSuccess: true });
        await refreshRuntimeHealth();
      }
    } catch (error: any) {
      await window.ipcRenderer.alert('Update Failed', String(error?.message || error || 'The update could not be prepared.'));
    }
  }, [handleSyncToGame, handleUpdateMod, legacyInstallMode, refreshRuntimeHealth]);

  const confirmProfileUpdates = useCallback(async () => {
    if (!activeProfileId || pendingProfileUpdates.length === 0 || isUpdatingProfile) return;
    setIsUpdatingProfile(true);
    try {
      await stageProfileUpdates(pendingProfileUpdates, activeProfileId);
      setPendingProfileUpdates([]);
      if (legacyInstallMode) {
        await handleSyncToGame(undefined, { silentSuccess: true });
        await refreshRuntimeHealth();
      }
    } catch (error: any) {
      await window.ipcRenderer.alert('Update Failed', String(error?.message || error || 'The update plan could not be prepared.'));
    } finally {
      setIsUpdatingProfile(false);
    }
  }, [activeProfileId, handleSyncToGame, isUpdatingProfile, legacyInstallMode, pendingProfileUpdates, refreshRuntimeHealth, stageProfileUpdates]);

  const handleRevertPending = useCallback(async (ids: string[]) => {
    const profile = useProfileStore.getState().profiles.find(candidate => candidate.id === activeProfileId);
    if (!profile || ids.length === 0) return;

    if (profile.apply_interrupted) {
      const community = profile.gameIdentifier || selectedCommunity;
      if (!community) {
        await window.ipcRenderer.alert('Cannot Revert', 'Configure the game path before reverting an interrupted Apply.');
        return;
      }
      try {
        await window.ipcRenderer.rollbackProfileApplyTransaction(profile.id, community);
      } catch (error: any) {
        await window.ipcRenderer.alert(
          'Revert Failed',
          `The game snapshot could not be restored: ${String(error?.message || error || 'unknown error')}`,
        );
        return;
      }
    }

    revertPendingMods(profile.id, ids);
    const reverted = useProfileStore.getState().profiles.find(candidate => candidate.id === profile.id);
    if (!reverted) return;
    const stillPending = reverted.mods.some(mod => mod.pending_sync)
      || (reverted.pending_removals?.length ?? 0) > 0;
    updateProfile(profile.id, {
      apply_interrupted: false,
      needs_sync: stillPending,
    });
    await window.ipcRenderer.saveProfiles(useProfileStore.getState().profiles);
  }, [activeProfileId, revertPendingMods, selectedCommunity, updateProfile]);

  const handleSyncPending = useCallback(async (ids: string[]) => {
    const original = useProfileStore.getState().profiles.find(profile => profile.id === activeProfileId);
    if (!original || ids.length === 0) return;
    const allIds = [
      ...original.mods.filter(mod => mod.pending_sync).map(mod => mod.uuid4),
      ...(original.pending_removals || []).map(removal => removal.id),
    ];
    if (ids.length === allIds.length && allIds.every(id => ids.includes(id))) {
      await installToGameRequestRef.current?.();
      return;
    }

    const health = await refreshRuntimeHealth();
    if (health && (health.status === 'missing' || health.status === 'incomplete') && !hasPendingRuntimeInstall(original, health.runtime)) {
      const confirmedRepair = await window.ipcRenderer.confirm(
        'Repair Runtime Before Sync?',
        `${health.runtime === 'bepinex' ? 'BepInEx' : health.runtime === 'owml' ? 'OWML' : health.runtime === 'returnofmodding' ? 'ReturnOfModding' : 'Lovely'} is ${health.status}. Repair it before synchronizing this selection?`
      );
      if (!confirmedRepair || !await repairProfileRuntime()) return;
    }

    const selected = new Set(ids);
    const pendingByKey = new Map(original.mods
      .filter(mod => mod.pending_sync)
      .map(mod => [getProfileModKey(mod.fullName), mod]));
    const dependencyQueue = original.mods.filter(mod => selected.has(mod.uuid4));
    while (dependencyQueue.length > 0) {
      const mod = dependencyQueue.shift()!;
      const pkg = profilePackageIndex[getProfileModKey(mod.fullName)];
      const version = pkg?.versions.find(candidate => candidate.version_number === mod.versionNumber);
      for (const dependency of version?.dependencies || []) {
        const dependencyMod = pendingByKey.get(getProfileModKey(dependency));
        if (dependencyMod && !selected.has(dependencyMod.uuid4)) {
          selected.add(dependencyMod.uuid4);
          dependencyQueue.push(dependencyMod);
        }
      }
    }
    const effectiveMods: InstalledMod[] = [];
    for (const mod of original.mods) {
      if (!mod.pending_sync || selected.has(mod.uuid4)) {
        effectiveMods.push(mod);
      } else if (mod.sync_baseline) {
        effectiveMods.push(restoreInstalledMod(mod.sync_baseline));
      } else if (mod.sync_baseline === undefined) {
        await window.ipcRenderer.alert('Cannot Sync Selection', `${mod.displayName || mod.fullName} has no verified synchronized baseline yet.`);
        return;
      }
    }
    for (const removal of original.pending_removals || []) {
      if (!selected.has(removal.id)) effectiveMods.push(restoreInstalledMod(removal.mod));
    }
    const restoreState = {
      mods: original.mods,
      pending_removals: original.pending_removals || [],
      needs_sync: true,
    };
    updateProfile(original.id, {
      mods: effectiveMods,
      pending_removals: (original.pending_removals || []).filter(removal => selected.has(removal.id)),
      selective_sync_restore: restoreState,
      needs_sync: true,
    });
    await window.ipcRenderer.saveProfiles(useProfileStore.getState().profiles);
    const succeeded = await handleSyncToGame(undefined, { silentSuccess: true });
    if (!succeeded) {
      const failedEffective = useProfileStore.getState().profiles.find(profile => profile.id === original.id);
      const failedByKey = new Map((failedEffective?.mods || []).map(mod => [getProfileModKey(mod.fullName), mod]));
      const failedRemovalById = new Map((failedEffective?.pending_removals || []).map(removal => [removal.id, removal]));
      updateProfile(original.id, {
        ...restoreState,
        mods: original.mods.map(mod => {
          if (!selected.has(mod.uuid4)) return mod;
          const failed = failedByKey.get(getProfileModKey(mod.fullName));
          return failed ? {
            ...mod,
            pending_sync_status: failed.pending_sync_status,
            pending_sync_error: failed.pending_sync_error,
          } : mod;
        }),
        pending_removals: (original.pending_removals || []).map(removal => {
          if (!selected.has(removal.id)) return removal;
          const failed = failedRemovalById.get(removal.id);
          return failed ? { ...removal, sync_status: failed.sync_status, sync_error: failed.sync_error } : removal;
        }),
        selective_sync_restore: undefined,
        apply_interrupted: true,
      });
      return;
    }

    const selectedRemovalKeys = new Set((original.pending_removals || [])
      .filter(removal => selected.has(removal.id))
      .map(removal => getProfileModKey(removal.mod.fullName)));
    const mods = original.mods
      .filter(mod => !selectedRemovalKeys.has(getProfileModKey(mod.fullName)))
      .map(mod => selected.has(mod.uuid4)
        ? { ...mod, pending_sync: false, synced_enabled: mod.enabled, pending_sync_kind: undefined, pending_sync_status: undefined, pending_sync_error: undefined, sync_baseline: undefined }
        : mod);
    const pendingRemovals = (original.pending_removals || []).filter(removal => !selected.has(removal.id));
    updateProfile(original.id, {
      mods,
      pending_removals: pendingRemovals,
      selective_sync_restore: undefined,
      apply_interrupted: false,
      needs_sync: mods.some(mod => mod.pending_sync) || pendingRemovals.length > 0,
    });
    await window.ipcRenderer.saveProfiles(useProfileStore.getState().profiles);
    await refreshRuntimeHealth();
  }, [activeProfileId, handleSyncToGame, profilePackageIndex, refreshRuntimeHealth, repairProfileRuntime, updateProfile]);

  const handleInstallRequest = async (
    pkg: Package,
    targetProfileId?: string,
    selectedVersion?: PackageVersion
  ) => {
    if (askVersionBeforeInstall && !selectedVersion) {
      setSelectedMod(pkg);
      return;
    }

    await handleInstallMod(pkg, targetProfileId, selectedVersion);
  };

  const handleCancelProgress = async () => {
    if (!progressState.isOpen || !progressState.isCancelable) return;

    if (progressState.operation === 'mod-sync') {
      setProgressState(prev => ({
        ...prev,
        isOpen: false,
        isCancelable: false,
        downloadSpeedBps: undefined,
        downloadedBytes: undefined,
        totalBytes: undefined,
        activeDownloads: 0,
      }));
      if (activeProfileId) {
        updateProfile(activeProfileId, { needs_sync: true, apply_interrupted: true });
      }
      await window.ipcRenderer.cancelModOperations();
      return;
    }

    if (isCancellingCustomModImport) return;

    customModImportCancelledRef.current = true;
    setIsCancellingCustomModImport(true);
    setProgressState(prev => ({
      ...prev,
      isOpen: false,
      isCancelable: false,
      currentTask: 'Cancelling custom mod import...'
    }));

    try {
      await window.ipcRenderer.cancelCustomModImport();
    } catch (error) {
      console.error('Failed to cancel custom mod import', error);
    }
  };

  const handleMinimizeProgress = () => {
    if (progressState.isOpen) setIsProgressMinimized(true);
  };

  useEffect(() => {
    if (progressState.isOpen) return;
    const reset = window.setTimeout(() => setIsProgressMinimized(false), 0);
    return () => window.clearTimeout(reset);
  }, [progressState.isOpen]);

  const mergeProfileArchiveIntoActiveProfile = useCallback(async (
    archivePath: string,
    result: any
  ): Promise<ProfileArchiveMergeSummary> => {
    const targetProfile = activeProfile;
    const targetCommunity = selectedCommunity || targetProfile?.gameIdentifier || null;
    const profileName = typeof result?.name === 'string' && result.name.trim()
      ? result.name.trim()
      : 'Imported Profile';
    const importedMods: ImportedProfileMod[] = Array.isArray(result?.mods) ? result.mods : [];
    const localMods = importedMods.filter((mod) => mod.source === 'local');
    const thunderstoreMods = importedMods.filter((mod) => mod.source !== 'local' && getProfileModName(mod));

    if (!targetProfile || !targetCommunity) {
      return {
        handled: true,
        profileName,
        importedCount: 0,
        failedMods: ['No active profile selected'],
      };
    }

    if (importedMods.length === 0) {
      return {
        handled: true,
        profileName,
        importedCount: 0,
        failedMods: [],
      };
    }

    setProgressState({
      isOpen: true,
      title: 'Importing Profile Mods',
      progress: 8,
      currentTask: `Reading ${profileName}...`,
      isCancelable: true,
    });

    const modNames = Array.from(new Set(thunderstoreMods.map(getProfileModName).filter(Boolean)));
    const lookup = modNames.length > 0
      ? await window.ipcRenderer.lookupPackagesByNames(targetCommunity, modNames)
      : { found: [], unknown: [] };

    const foundPackages: Package[] = Array.isArray(lookup?.found) ? lookup.found : [];
    const unknownMods: string[] = Array.isArray(lookup?.unknown) ? lookup.unknown : [];
    const failedMods: string[] = unknownMods.map((name) => `${name} (not found on Thunderstore)`);

    if (customModImportCancelledRef.current) {
      return { handled: true, cancelled: true, profileName, importedCount: 0, failedMods };
    }

    if (unknownMods.length > 0) {
      setProgressState(prev => ({ ...prev, isOpen: false, isCancelable: false }));
      const proceed = await window.ipcRenderer.confirm(
        'Some mods cannot be found',
        `${unknownMods.length} mod(s) from "${profileName}" were not found on Thunderstore and will be skipped:\n\n${unknownMods.join('\n')}\n\nContinue importing the remaining mods into "${targetProfile.name}"?`
      );
      if (!proceed) {
        return { handled: true, cancelled: true, profileName, importedCount: 0, failedMods };
      }
      setProgressState({
        isOpen: true,
        title: 'Importing Profile Mods',
        progress: 10,
        currentTask: `Importing ${profileName}...`,
        isCancelable: true,
      });
    }

    const resolvedThunderstoreMods = thunderstoreMods.filter((mod) => {
      const modName = getProfileModName(mod);
      return modName && !unknownMods.includes(modName);
    });
    const totalSteps = resolvedThunderstoreMods.length + localMods.length;
    let completedSteps = 0;
    let importedCount = 0;
    const stagedThunderstoreMods: InstalledMod[] = [];
    const resolutionFailures: string[] = [];

    const updateMergeProgress = (task: string) => {
      const progress = totalSteps === 0
        ? 100
        : Math.round((completedSteps / totalSteps) * 90) + 10;
      setProgressState(prev => ({
        ...prev,
        progress: Math.min(100, Math.max(10, progress)),
        currentTask: task,
      }));
    };

    for (const mod of resolvedThunderstoreMods) {
      if (customModImportCancelledRef.current) {
        return { handled: true, cancelled: true, profileName, importedCount, failedMods };
      }

      const modName = getProfileModName(mod);
      updateMergeProgress(`Adding ${modName} (${completedSteps + 1}/${Math.max(totalSteps, 1)})...`);

      try {
        const pkg = foundPackages.find((p) => p.full_name.toLowerCase() === modName.toLowerCase());
        if (!pkg) {
          throw new Error('Package not found after lookup');
        }
        if (!mod.version) throw new Error('Profile entry has no pinned version');
        const exactPkg = pkg.versions.some((v) => v.version_number === mod.version)
          ? pkg
          : await window.ipcRenderer.fetchPackageByName(`${modName}-${mod.version}`, targetCommunity);
        if (!exactPkg) throw new Error(`Pinned version ${mod.version} is unavailable`);
        const version = findPinnedVersion(exactPkg, mod.version, modName);

        const installedMod: InstalledMod = {
          uuid4: version.uuid4,
          fullName: version.full_name,
          versionNumber: version.version_number,
          iconUrl: version.icon,
          enabled: mod.enabled ?? true,
          pending_sync: true,
          synced_enabled: undefined,
        };
        stagedThunderstoreMods.push(installedMod);
      } catch (error) {
        console.error(`Failed to add profile mod ${modName}`, error);
        const reason = error instanceof Error ? error.message : String(error || 'unknown resolution error');
        const failure = `${modName}: ${reason}`;
        failedMods.push(failure);
        resolutionFailures.push(failure);
      } finally {
        completedSteps++;
        updateMergeProgress(`Processed ${completedSteps}/${Math.max(totalSteps, 1)}...`);
      }
    }

    if (resolutionFailures.length > 0) {
      return {
        handled: true,
        profileName,
        importedCount: 0,
        failedMods,
      };
    }
    for (const mod of stagedThunderstoreMods) addMod(targetProfile.id, mod);
    importedCount += stagedThunderstoreMods.length;

    for (const mod of localMods) {
      if (customModImportCancelledRef.current) {
        return { handled: true, cancelled: true, profileName, importedCount, failedMods };
      }

      const modName = mod.displayName || getProfileModName(mod) || 'Custom mod';
      updateMergeProgress(`Staging ${modName} (${completedSteps + 1}/${Math.max(totalSteps, 1)})...`);

      try {
        const payloadPath = mod.payload;
        const embeddedArchivePath = result.archivePath || archivePath;
        if (!payloadPath || !embeddedArchivePath) {
          throw new Error('Embedded custom mod payload is missing from this profile export.');
        }

        const imported = await window.ipcRenderer.importEmbeddedCustomMod(
          targetProfile.id,
          embeddedArchivePath,
          payloadPath,
          {
            name: mod.displayName || getProfileModName(mod).split('-').slice(1).join('-') || getProfileModName(mod),
            author: mod.author || getProfileModName(mod).split('-')[0] || 'Local',
            version: mod.version,
            enabled: mod.enabled,
            platforms: mod.platforms,
            expectedSha256: mod.sha256,
          }
        );
        addMod(targetProfile.id, {
          ...imported.mod,
          pending_sync: true,
          synced_enabled: undefined,
        });
        importedCount++;
      } catch (error) {
        console.error(`Failed to stage embedded custom mod ${modName}`, error);
        const reason = error instanceof Error ? error.message : String(error || 'custom payload error');
        failedMods.push(`${modName}: ${reason}`);
      } finally {
        completedSteps++;
        updateMergeProgress(`Processed ${completedSteps}/${Math.max(totalSteps, 1)}...`);
      }
    }

    return {
      handled: true,
      profileName,
      importedCount,
      failedMods,
    };
  }, [activeProfile, selectedCommunity, addMod]);

  const tryMergeProfileArchive = useCallback(async (
    path: string
  ): Promise<ProfileArchiveMergeSummary | null> => {
    if (!isArchiveImportPath(path)) return null;

    try {
      const result = await window.ipcRenderer.importProfileFromFile(path);
      if (result?.type !== 'profile' || !Array.isArray(result.mods)) return null;
      return await mergeProfileArchiveIntoActiveProfile(path, result);
    } catch (error) {
      console.debug('Archive is not a profile export; falling back to custom mod import.', error);
      return null;
    }
  }, [mergeProfileArchiveIntoActiveProfile]);

  const importCustomModPaths = useCallback(async (paths: string[]) => {
    if (!activeProfile) {
      await window.ipcRenderer.alert('Profile Required', 'Create or select a profile before importing a custom mod.');
      return;
    }

    const importPaths = paths.map(path => path.trim()).filter(Boolean);
    if (importPaths.length === 0) return;

    try {
      customModImportCancelledRef.current = false;
      setIsCancellingCustomModImport(false);
      let customImportedCount = 0;
      let profileImportedCount = 0;
      let profileArchiveCount = 0;
      const importedNames: string[] = [];
      const profileNames: string[] = [];
      const failedMods: string[] = [];

      setProgressState({
        isOpen: true,
        title: 'Importing Content',
        progress: 10,
        currentTask: importPaths.length > 1
          ? `Scanning 1/${importPaths.length}...`
          : 'Scanning selected content...',
        isCancelable: true
      });

      for (let index = 0; index < importPaths.length; index++) {
        const path = importPaths[index];
        setProgressState(prev => ({
          ...prev,
          progress: Math.max(10, Math.round((index / Math.max(importPaths.length, 1)) * 90)),
          currentTask: importPaths.length > 1
            ? `Staging custom mod ${index + 1}/${importPaths.length}...`
            : 'Staging custom mod...'
        }));

        const profileMerge = await tryMergeProfileArchive(path);
        if (profileMerge?.handled) {
          if (profileMerge.cancelled) {
            customModImportCancelledRef.current = false;
            setIsCancellingCustomModImport(false);
            setProgressState(prev => ({ ...prev, isOpen: false, isCancelable: false }));
            return;
          }
          profileArchiveCount++;
          profileImportedCount += profileMerge.importedCount;
          if (profileMerge.profileName) profileNames.push(profileMerge.profileName);
          failedMods.push(...profileMerge.failedMods);
          continue;
        }

        const result = await window.ipcRenderer.importCustomMod(activeProfile.id, path, {});
        if (customModImportCancelledRef.current) {
          if (result.mod.localId) {
            await window.ipcRenderer.deleteLocalModPayload(activeProfile.id, result.mod.localId);
          }
          customModImportCancelledRef.current = false;
          setIsCancellingCustomModImport(false);
          setProgressState(prev => ({ ...prev, isOpen: false, isCancelable: false }));
          return;
        }
        addMod(activeProfile.id, result.mod);
        customImportedCount++;
        importedNames.push(result.mod.displayName || result.mod.fullName);
      }

      setProgressState(prev => ({ ...prev, isOpen: false, isCancelable: false }));
      setIsCancellingCustomModImport(false);
      const totalImported = customImportedCount + profileImportedCount;
      const title = profileArchiveCount > 0 && customImportedCount === 0
        ? 'Profile Mods Imported'
        : customImportedCount === 1 && profileImportedCount === 0
          ? 'Custom Mod Imported'
          : 'Import Complete';
      let message = '';

      if (profileArchiveCount > 0) {
        const uniqueProfileNames = Array.from(new Set(profileNames));
        message += `${profileImportedCount} mod(s) from ${uniqueProfileNames.length === 1 ? `"${uniqueProfileNames[0]}"` : `${uniqueProfileNames.length} profile files`} were added or updated in "${activeProfile.name}".`;
      }

      if (customImportedCount > 0) {
        if (message) message += '\n\n';
        message += customImportedCount === 1
          ? `${importedNames[0]} was added to the profile.`
          : `${customImportedCount} custom mods were added to the profile.`;
      }

      if (totalImported > 0) {
        message += '\n\nThese changes are staged in r2modmac and will be installed when you apply the profile to the game.';
      } else if (!message) {
        message = 'No mods were imported.';
      }

      if (failedMods.length > 0) {
        message += `\n\nNot imported:\n${failedMods.join('\n')}`;
      }

      await window.ipcRenderer.alert(title, message);
    } catch (error: any) {
      const message = String(error?.message || error || 'Failed to import the selected custom mod folder.');
      const wasCancelled = customModImportCancelledRef.current || message.toLowerCase().includes('cancelled');
      customModImportCancelledRef.current = false;
      setIsCancellingCustomModImport(false);
      setProgressState(prev => ({ ...prev, isOpen: false, isCancelable: false }));
      if (wasCancelled) return;

      await window.ipcRenderer.alert(
        'Import Failed',
        message
      );
    }
  }, [activeProfile, addMod, tryMergeProfileArchive]);

  const handleImportCustomModRequest = async () => {
    let path: string | null = null;
    try {
      path = await window.ipcRenderer.selectImportPath();
    } catch (error) {
      console.warn('Unified import picker failed; falling back to folder picker.', error);
      path = await window.ipcRenderer.selectFolder();
    }
    if (!path) return;
    await importCustomModPaths([path]);
  };

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;

    getCurrentWebview().onDragDropEvent((event) => {
      if (!activeProfileId) {
        return;
      }

      const payload = event.payload;
      if (payload.type === 'enter') {
        const hasPaths = payload.paths.length > 0;
        const allValid = payload.paths.every(isValidPathForImport);
        setIsCustomModDragActive(hasPaths);
        setIsCustomModDragValid(allValid);
        return;
      }
      if (payload.type === 'over') {
        setIsCustomModDragActive(true);
        return;
      }
      if (payload.type === 'leave') {
        setIsCustomModDragActive(false);
        setIsCustomModDragValid(true);
        return;
      }
      if (payload.type === 'drop') {
        setIsCustomModDragActive(false);
        const allValid = payload.paths.every(isValidPathForImport);
        setIsCustomModDragValid(true);
        if (progressState.isOpen || isCancellingCustomModImport) return;
        if (!allValid) {
          void window.ipcRenderer.alert('Invalid Format', 'Only folders, .zip, or .r2z files can be imported.');
          return;
        }
        void importCustomModPaths(payload.paths);
      }
    }).then((cleanup) => {
      if (disposed) {
        cleanup();
      } else {
        unlisten = cleanup;
      }
    }).catch((error) => {
      console.error('Failed to listen for custom mod drag and drop', error);
    });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [
    importCustomModPaths,
    progressState.isOpen,
    isCancellingCustomModImport,
    activeProfileId
  ]);

  const handleInstallToGameRequest = async (
    isVanillaOverride?: boolean,
    options?: {
      skipConfirm?: boolean;
      silentSuccess?: boolean;
    }
  ) => {
    if (applyInFlightRef.current) return;

    applyInFlightRef.current = true;
    setIsApplyingToGame(true);

    try {
      if (isVanillaOverride === undefined) {
        const health = await refreshRuntimeHealth();
        if (health && (health.status === 'missing' || health.status === 'incomplete') && !hasPendingRuntimeInstall(activeProfile, health.runtime)) {
          const confirmedRepair = await window.ipcRenderer.confirm(
            'Repair Runtime Before Apply?',
            `${health.runtime === 'bepinex' ? 'BepInEx' : health.runtime === 'owml' ? 'OWML' : health.runtime === 'returnofmodding' ? 'ReturnOfModding' : 'Lovely'} is ${health.status}. ` +
            'The working files will be repaired before the profile is synchronized.'
          );
          if (!confirmedRepair || !await repairProfileRuntime()) return;
        }
      }
      const runSync = async () => {
        await handleSyncToGame(isVanillaOverride, { silentSuccess: options?.silentSuccess });
      };
      if (options?.skipConfirm || !confirmBeforeApplyToGame || isVanillaOverride !== undefined) {
        await runSync();
        return;
      }

      const confirmed = await window.ipcRenderer.confirm(
        'Apply Profile to Game?',
        'This will sync your profile mods into the game directory. Continue?'
      );
      if (!confirmed) return;

      await runSync();
    } finally {
      await refreshRuntimeHealth();
      clearSteamRestartingState();
      applyInFlightRef.current = false;
      setIsApplyingToGame(false);
    }
  };
  useEffect(() => {
    installToGameRequestRef.current = handleInstallToGameRequest;
    return () => {
      if (installToGameRequestRef.current === handleInstallToGameRequest) {
        installToGameRequestRef.current = null;
      }
    };
  }, [handleInstallToGameRequest]);

  /** Resolves true when the profile really ended up in the requested state. */
  const handleToggleProfileVanilla = async (profileId: string, newVanillaState: boolean): Promise<boolean> => {
    if (profileActionLockRef.current || applyInFlightRef.current) return false;
    const profile = profiles.find((p) => p.id === profileId);
    if (!profile) return false;

    const disabledMods = profile.mods.filter((m) => !m.enabled).map((m) => m.fullName);
    const hadPendingSync = !!profile.needs_sync || profile.mods.some((m) => m.pending_sync);

    let succeeded = false;
    profileActionLockRef.current = true;
    applyInFlightRef.current = true;
    setIsApplyingToGame(true);
    updateProfile(profileId, { is_vanilla: newVanillaState });

    try {
      if (newVanillaState) {
        try {
          await window.ipcRenderer.stopGame(profile.gameIdentifier, profile.platform);
          setIsGameRunning(false);
        } catch (error) {
          console.warn('Failed to stop the running game before switching to vanilla:', error);
        }
      }

      await window.ipcRenderer.saveProfiles(useProfileStore.getState().profiles);
      await window.ipcRenderer.installToGame(
        profile.gameIdentifier,
        profile.id,
        disabledMods,
        newVanillaState
      );

      updateProfile(profileId, {
        is_vanilla: newVanillaState,
        needs_sync: hadPendingSync,
      });
      succeeded = true;
    } catch (error: any) {
      updateProfile(profileId, { is_vanilla: !newVanillaState });
      await window.ipcRenderer.alert(
        'Profile Toggle Failed',
        String(error?.message || error || 'Failed to update the profile runtime state.')
      );
    } finally {
      clearSteamRestartingState();
      applyInFlightRef.current = false;
      profileActionLockRef.current = false;
      setIsApplyingToGame(false);
    }

    return succeeded;
  };

  const handleSavePreferences = async (newSettings: PreferencesSettings) => {
    setLegacyInstallMode(newSettings.legacy_install_mode);
    setAskVersionBeforeInstall(newSettings.ask_version_before_install);
    setInstallInParallel(newSettings.install_in_parallel);
    setConfirmBeforeApplyToGame(newSettings.confirm_before_apply_to_game);
    setWriteDebugLogsToGame(newSettings.write_debug_logs_to_game);
    setVerboseLogging(newSettings.verbose_logging);
    if (!newSettings.verbose_logging) setVerboseLogsWarningBytes(null);
    void window.ipcRenderer.setVerboseLogging(newSettings.verbose_logging);
    setDefaultModViewMode(newSettings.default_mod_view_mode);
    setViewMode(newSettings.default_mod_view_mode);
    setShowDeprecatedWarnings(newSettings.show_deprecated_warnings);
    setSponsoredMessagesEnabled(newSettings.sponsored_messages_enabled);
    setSponsoredMessagesScale(newSettings.sponsored_messages_scale);
    setSponsoredMessagesOpacity(newSettings.sponsored_messages_background_opacity);
    setStreamMode(newSettings.stream_mode);
    setDefaultGame(newSettings.default_game);
    setDefaultProfile(newSettings.default_profile ?? null);
    useKeybindStore.getState().hydrate(newSettings.keybinds);

    const currentSettings = await window.ipcRenderer.getSettings();
    await window.ipcRenderer.saveSettings({
      ...currentSettings,
      legacy_install_mode: newSettings.legacy_install_mode,
      ask_version_before_install: newSettings.ask_version_before_install,
      install_in_parallel: newSettings.install_in_parallel,
      confirm_before_apply_to_game: newSettings.confirm_before_apply_to_game,
      write_debug_logs_to_game: newSettings.write_debug_logs_to_game,
      verbose_logging: newSettings.verbose_logging,
      default_mod_view_mode: newSettings.default_mod_view_mode,
      show_deprecated_warnings: newSettings.show_deprecated_warnings,
      sponsored_messages_enabled: newSettings.sponsored_messages_enabled,
      sponsored_messages_scale: newSettings.sponsored_messages_scale,
      sponsored_messages_background_opacity: newSettings.sponsored_messages_background_opacity,
      default_game: newSettings.default_game,
      default_profile: newSettings.default_profile ?? null,
      stream_mode: newSettings.stream_mode,
      keybinds: newSettings.keybinds ?? {},
    });
  };

  const handleSponsorPreferencesChange = async (enabled: boolean) => {
    setSponsoredMessagesEnabled(enabled);
    await window.ipcRenderer.updateSponsorPreferences(enabled);
  };

  const handleSetGuideHidden = async (guide: 'crossover' | 'macos', hidden: boolean) => {
    if (guide === 'crossover') {
      setHideCrossOverGuide(hidden);
    }

    const currentSettings = await window.ipcRenderer.getSettings();
    await window.ipcRenderer.saveSettings({
      ...currentSettings,
      hide_crossover_guide: guide === 'crossover' ? hidden : !!currentSettings.hide_crossover_guide,
    });
  };

  const handleRestoreGuideWarnings = async () => {
    setHideCrossOverGuide(false);
    setHideVerboseLogsWarning(false);
    setVerboseLogsWarningDismissed(false);

    const currentSettings = await window.ipcRenderer.getSettings();
    await window.ipcRenderer.saveSettings({
      ...currentSettings,
      hide_crossover_guide: false,
      hide_macos_guide: false,
      hide_verbose_logs_warning: false,
    });

    await window.ipcRenderer.alert(
      'Warnings restored',
      'Setup warnings have been re-enabled. They will be shown again when needed.'
    );
  };

  const handleVerboseLoggingFromWarning = async (enabled: boolean) => {
    setVerboseLogging(enabled);
    if (!enabled) setVerboseLogsWarningBytes(null);
    await window.ipcRenderer.setVerboseLogging(enabled);

    const currentSettings = await window.ipcRenderer.getSettings();
    await window.ipcRenderer.saveSettings({
      ...currentSettings,
      verbose_logging: enabled,
    });
  };

  const handleClearAppLogs = async () => {
    await window.ipcRenderer.clearAppLogs();
    setVerboseLogsWarningBytes(null);
  };

  const handleHideVerboseLogsWarning = async () => {
    setHideVerboseLogsWarning(true);
    setVerboseLogsWarningBytes(null);

    const currentSettings = await window.ipcRenderer.getSettings();
    await window.ipcRenderer.saveSettings({
      ...currentSettings,
      hide_verbose_logs_warning: true,
    });
  };



  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Escape' || event.defaultPrevented || event.metaKey || event.ctrlKey || event.altKey) {
        return;
      }

      // The palette closes itself; without this the same Escape would also step
      // back out of the profile behind it.
      if (useCommandStore.getState().isOpen) {
        return;
      }

      if (progressState.isOpen && !isProgressMinimized) {
        event.preventDefault();
        setIsProgressMinimized(true);
        return;
      }

      if (selectedMod) {
        event.preventDefault();
        setSelectedMod(null);
        return;
      }

      if (uninstallModalState.isOpen) {
        event.preventDefault();
        setUninstallModalState((prev: any) => ({ ...prev, isOpen: false }));
        return;
      }

      if (showSettings) {
        event.preventDefault();
        setShowSettings(false);
        return;
      }

      if (showExportModal) {
        event.preventDefault();
        setShowExportModal(false);
        return;
      }

      if (showUpdateModal) {
        event.preventDefault();
        setShowUpdateModal(false);
        return;
      }

      if (showCrossOverGuide) {
        event.preventDefault();
        setShowCrossOverGuide(false);
        return;
      }

      if (showPreferences) {
        event.preventDefault();
        setShowPreferences(false);
        return;
      }

      if (activeProfileId || isBrowsingMode) {
        event.preventDefault();
        flushSync(() => {
          setSelectedMod(null);
          selectProfile('');
          setIsBrowsingMode(false);
        });
        return;
      }

      if (selectedCommunity) {
        event.preventDefault();
        flushSync(() => {
          setSelectedCommunity(null);
        });
      }
    };

    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [
    activeProfileId,
    isBrowsingMode,
    selectedCommunity,
    selectedMod,
    selectProfile,
    showCrossOverGuide,
    showExportModal,
    showPreferences,
    showSettings,
    showUpdateModal,
    uninstallModalState.isOpen,
    progressState.isOpen,
    isProgressMinimized,
  ]);

  useEffect(() => {
    const handleOpenPreferences = () => {
      setPreferencesPanel(null);
      setShowPreferences(true);
    };
    window.addEventListener('r2modmac:open-preferences', handleOpenPreferences);
    return () => window.removeEventListener('r2modmac:open-preferences', handleOpenPreferences);
  }, []);

  // VIEW LOGIC
  let content;

  if (!selectedCommunity) {
    // STEP 1: GAME SELECTION
    content = (
      <GameSelectionScreen
        communities={communities}
        communityImages={communityImages}
        communityPlatforms={communityPlatforms}
        loading={loading}
        selectedCommunity={selectedCommunity}
        onSelectCommunity={setSelectedCommunity}
        onOpenPreferences={() => {
          setPreferencesPanel(null);
          setShowPreferences(true);
        }}
        searchQuery={gameSearchQuery}
        onSearchQueryChange={setGameSearchQuery}
      />
    );
  } else if (!activeProfileId && !isBrowsingMode) {
    // STEP 2: PROFILE SELECTION
    const selectedGame = communities.find(c => c.identifier === selectedCommunity);
    const selectedGameCover = selectedGame ? communityImages[selectedGame.identifier] : undefined;

    content = (
      <div className="r2-app-backdrop flex flex-col h-full bg-gray-900 overflow-hidden">
        <div className="p-4 border-b border-gray-800 sticky top-0 bg-gray-900 z-10 relative overflow-hidden">
          <div className="absolute inset-0 bg-gray-900" />
          {selectedGameCover && (
            <>
              <div
                className="absolute inset-0 bg-cover bg-center scale-110 opacity-70 pointer-events-none"
                style={{
                  backgroundImage: `url(${selectedGameCover})`,
                  filter: 'blur(10px) saturate(0.95)',
                }}
              />
              <div className="absolute inset-0 bg-gray-900/45 pointer-events-none" />
              <div className="absolute inset-y-0 left-0 w-80 bg-gradient-to-r from-gray-900 via-gray-900/95 to-transparent pointer-events-none" />
            </>
          )}
          <div className="relative z-10 flex items-center gap-4">
            <Button variant="ghost" size="sm" onClick={() => setSelectedCommunity(null)}>
              ← Change Game
            </Button>
            <div className="h-6 w-px bg-gray-700/90" />
            <h2 className="text-xl font-bold text-white">
              {selectedGame?.name}
            </h2>
          </div>
        </div>

        <ProfileList
          profiles={profiles}
          selectedGameIdentifier={selectedCommunity}
          selectedGamePlatform={selectedCommunity ? communityPlatforms[selectedCommunity] : undefined}
          isBusy={isApplyingToGame || isLaunchingProfile || isStoppingProfile || isSteamRestarting}
          onSelectProfile={handleSelectProfile}
          onCreateProfile={(name, platform) => createProfile(name, selectedCommunity!, platform)}
          onImportProfile={handleImportProfile}
          onImportFile={handleImportFile}
          onBrowseMods={() => {
            selectProfile('');
            setIsBrowsingMode(true);
          }}
          onFindProfile={() => openPalette(selectedGamePaletteScope)}
          onDeleteProfile={deleteProfile}
          onUpdateProfile={updateProfile}
          onToggleVanilla={handleToggleProfileVanilla}
        />
      </div>
    );
  } else {
    // STEP 3: MOD MANAGEMENT
    const currentCommunity = communities.find(c => c.identifier === selectedCommunity);
    const markActiveProfileUsed = () => {
      if (!activeProfile) return;
      updateProfile(activeProfile.id, { lastUsed: Date.now() });
    };

    const handleLaunchModdedDirect = async () => {
      if (!activeProfile) return;
      if (profileActionLockRef.current || applyInFlightRef.current) return;

      // "Launch modded" says what it will do, so a switched-off profile is
      // switched back on rather than refused. The toggle takes the lock itself,
      // which is why it runs before this handler claims it.
      if (activeProfile.is_vanilla && !(await handleToggleProfileVanilla(activeProfile.id, false))) {
        return;
      }

      // Re-read after the toggle: switching back on reinstalls the mods, so the
      // sync state from this render may no longer be the truth.
      const current = useProfileStore.getState().profiles.find((p) => p.id === activeProfile.id);
      const needsSync = !!current?.apply_interrupted || !!current?.needs_sync
        || !!current?.mods.some((mod) => mod.pending_sync);
      if (needsSync) {
        await window.ipcRenderer.alert(
          current?.apply_interrupted ? 'Resume Apply Required' : 'Apply Required',
          current?.apply_interrupted
            ? 'This profile has an interrupted apply. Click “Resume Apply” before launching.'
            : 'This profile has unapplied changes. Click “Apply to Game” before launching.'
        );
        return;
      }

      try {
        profileActionLockRef.current = true;
        setIsLaunchingProfile(true);
        await window.ipcRenderer.launchGameWithMods(activeProfile.gameIdentifier, activeProfile.id, activeProfile.platform);
        // UX: immediately reflect launch intent, then confirm with backend polling.
        beginLaunchGraceWindow();
        setIsGameRunning(true);
        markActiveProfileUsed();
        const running = await waitForLaunchStateToSettle(activeProfile.gameIdentifier, activeProfile.platform);
        if (running) {
          setIsGameRunning(true);
        }
      } catch (error: any) {
        clearSteamRestartingState();
        clearLaunchGraceWindow();
        setIsGameRunning(false);
        // Shown in r2modmac's own UI rather than a native alert: these are
        // usually a Steam-side condition the user can clear in seconds, not a
        // crash, and the wording matters.
        setLaunchIssue(describeLaunchIssue(String(error?.message || error || 'Failed to launch the modded game.')));
      } finally {
        profileActionLockRef.current = false;
        setIsLaunchingProfile(false);
      }
    };

    const handleLaunchVanillaDirect = async () => {
      if (!activeProfile) return;
      if (profileActionLockRef.current || applyInFlightRef.current) return;

      // Launching unmodded means the mods have to actually leave the game
      // folder, not merely be skipped — otherwise the run is only unmodded in
      // name. Same lock ordering as above.
      if (!activeProfile.is_vanilla && !(await handleToggleProfileVanilla(activeProfile.id, true))) {
        return;
      }

      try {
        profileActionLockRef.current = true;
        setIsLaunchingProfile(true);
        await window.ipcRenderer.launchGameVanilla(activeProfile.gameIdentifier, activeProfile.id, activeProfile.platform);
        // UX: immediately reflect launch intent, then confirm with backend polling.
        beginLaunchGraceWindow();
        setIsGameRunning(true);
        markActiveProfileUsed();
        const running = await waitForLaunchStateToSettle(activeProfile.gameIdentifier, activeProfile.platform);
        if (running) {
          setIsGameRunning(true);
        }
      } catch (error: any) {
        clearSteamRestartingState();
        clearLaunchGraceWindow();
        setIsGameRunning(false);
        setLaunchIssue(describeLaunchIssue(String(error?.message || error || 'Failed to launch the vanilla game.')));
      } finally {
        profileActionLockRef.current = false;
        setIsLaunchingProfile(false);
      }
    };

    const handleLaunchProfileDirect = async () => {
      if (!activeProfile) return;
      if (activeProfile.is_vanilla) {
        await handleLaunchVanillaDirect();
        return;
      }
      await handleLaunchModdedDirect();
    };

    const handleDuplicateActiveProfile = async () => {
      if (!activeProfile) return;
      if (profileActionLockRef.current || applyInFlightRef.current) return;

      try {
        profileActionLockRef.current = true;
        const newId = await duplicateProfile(activeProfile.id);
        if (newId) handleSelectProfile(newId);
      } catch (error: any) {
        await window.ipcRenderer.alert(
          'Duplicate Failed',
          String(error?.message || error || 'Failed to duplicate the profile.')
        );
      } finally {
        profileActionLockRef.current = false;
      }
    };

    const handleStopProfileDirect = async () => {
      if (!activeProfile) return;
      if (profileActionLockRef.current || applyInFlightRef.current) return;

      try {
        profileActionLockRef.current = true;
        setIsStoppingProfile(true);
        await window.ipcRenderer.stopGame(activeProfile.gameIdentifier, activeProfile.platform);
        clearLaunchGraceWindow();
        setIsGameRunning(false);
      } catch (error: any) {
        await window.ipcRenderer.alert(
          'Stop Failed',
          String(error?.message || error || 'Failed to stop the running game.')
        );
      } finally {
        profileActionLockRef.current = false;
        setIsStoppingProfile(false);
      }
    };

    // Rendered beside the sidebar because these handlers only exist in this
    // branch — there is nothing to apply or launch without an open profile.
    // The same actions the shortcuts fire, reachable by name for anyone who
    // would rather search than remember a combination.
    const gameCommands = (
      <>
        <ProfileCommandBridge
          request={pendingProfileCommand}
          handlers={{
            apply: () => { void handleInstallToGameRequest(); },
            launch: () => { void handleLaunchModdedDirect(); },
            'launch-vanilla': () => { void handleLaunchVanillaDirect(); },
            stop: () => { void handleStopProfileDirect(); },
            duplicate: () => { void handleDuplicateActiveProfile(); },
            export: () => setShowExportModal(true),
          }}
          onHandled={() => setPendingProfileCommand(null)}
        />
        <CommandSource
          id="profile"
          items={() => activeProfile ? [
          {
            id: 'action:apply',
            title: 'Apply mods to game',
            subtitle: activeProfile.name,
            group: 'Actions',
            icon: 'apply',
            game: activeProfile.gameIdentifier,
            profile: activeProfile.id,
            hint: formatAccelerator(activeKeybinds['apply-mods']),
            shortcut: 'apply-mods',
            run: () => { void handleInstallToGameRequest(); },
          },
          {
            id: 'action:launch',
            title: 'Launch game (modded)',
            subtitle: activeProfile.name,
            group: 'Actions',
            icon: 'play',
            game: activeProfile.gameIdentifier,
            profile: activeProfile.id,
            hint: formatAccelerator(activeKeybinds['launch-modded']),
            shortcut: 'launch-modded',
            run: () => { void handleLaunchModdedDirect(); },
          },
          {
            id: 'action:launch-vanilla',
            title: 'Launch game (unmodded)',
            group: 'Actions',
            icon: 'play',
            game: activeProfile.gameIdentifier,
            profile: activeProfile.id,
            hint: formatAccelerator(activeKeybinds['launch-vanilla']),
            shortcut: 'launch-vanilla',
            run: () => { void handleLaunchVanillaDirect(); },
          },
          {
            id: 'action:stop',
            title: 'Quit game',
            group: 'Actions',
            icon: 'stop',
            game: activeProfile.gameIdentifier,
            profile: activeProfile.id,
            hint: formatAccelerator(activeKeybinds['stop-game']),
            shortcut: 'stop-game',
            run: () => { void handleStopProfileDirect(); },
          },
          {
            id: 'action:duplicate',
            title: 'Duplicate profile',
            subtitle: activeProfile.name,
            group: 'Actions',
            icon: 'copy',
            game: activeProfile.gameIdentifier,
            profile: activeProfile.id,
            hint: formatAccelerator(activeKeybinds['duplicate-profile']),
            shortcut: 'duplicate-profile',
            run: () => { void handleDuplicateActiveProfile(); },
          },
          {
            id: 'action:export',
            title: 'Export profile',
            subtitle: activeProfile.name,
            group: 'Actions',
            icon: 'file',
            game: activeProfile.gameIdentifier,
            profile: activeProfile.id,
            run: () => setShowExportModal(true),
          },
          ] : []}
        />
      </>
    );

    const gameShortcuts = (
      <KeyboardShortcuts
        enabled={!showSettings && !showPreferences && !showExportModal && !showUpdateModal && !selectedMod}
        handlers={{
          'apply-mods': () => { void handleInstallToGameRequest(); },
          'launch-modded': () => { void handleLaunchModdedDirect(); },
          'launch-vanilla': () => { void handleLaunchVanillaDirect(); },
          'stop-game': () => { void handleStopProfileDirect(); },
          'duplicate-profile': () => { void handleDuplicateActiveProfile(); },
          'view-grid': () => setViewMode('grid'),
          'view-list': () => setViewMode('list'),
        }}
      />
    );

    const sidebar = (
      <ProfileSidebar
        key={activeProfile?.id || 'no-profile'}
        activeProfile={activeProfile ?? undefined}
        currentCommunity={currentCommunity || null}
        communityImage={currentCommunity ? communityImages[currentCommunity.identifier] : undefined}
        packageIndex={profilePackageIndex}
        legacyInstallMode={legacyInstallMode}
        showDeprecatedWarnings={showDeprecatedWarnings}
        installInParallel={installInParallel}
        onSelectProfile={handleSelectProfile}
        onToggleMod={(profileId, modUuid) => toggleMod(profileId, modUuid, legacyInstallMode)}
        onViewModDetails={(pkg) => setSelectedMod(pkg)}
        onOpenModFolder={async (profileId, modName) => {
          const activeProfilePlatform = activeProfile?.platform;
          try {
            await window.ipcRenderer.openModFolder(profileId, modName, selectedCommunity || '', activeProfilePlatform);
          } catch (e: any) {
            console.error("Failed to open mod folder:", e);
            const message = String(e?.message || e || '');
            if (message.includes('MODS_NOT_APPLIED') || message.includes('MOD_NOT_INSTALLED')) {
              await window.ipcRenderer.alert(
                "Mod Not Applied Yet",
                `The "${modName}" folder is not available in the game directory yet.\n\nApply your profile to the game first, then try again.`
              );
              return;
            }
            if (message.includes('GAME_PATH_NOT_CONFIGURED')) {
              await window.ipcRenderer.alert(
                "Game Path Required",
                "Please configure the game directory in Settings, then apply your profile to the game."
              );
              return;
            }
            await window.ipcRenderer.alert(
              "Directory Not Found",
              `The "${modName}" folder could not be found.\n\nPlease make sure the game directory is set correctly in the Settings.`
            );
          }
        }}
        onUninstallMod={async (mod) => {
          if (!activeProfile) return;
          const community = selectedCommunity || activeProfile.gameIdentifier;
          if (community) {
            const searchName = mod.fullName.replace(/-\d+\.\d+\.\d+$/, '');
            const pkg = await window.ipcRenderer.fetchPackageByName(searchName, community);
            if (pkg) {
              await handleUninstallWithDependencies(pkg, activeProfile.id);
              return;
            }
          }

          const confirmed = await window.ipcRenderer.confirm(
            'Uninstall Mod',
            `Uninstall ${mod.displayName || mod.fullName}?`
          );
          if (!confirmed) return;
          await removeMod(activeProfile.id, mod.uuid4, !legacyInstallMode);
        }}
        onResolvePackage={async (mod) => {
          if (mod.source === 'local') return null;
          // Extract mod name from fullName (format: "Author-ModName-Version")
          // We need "Author-ModName" (or just "ModName" depending on API) 
          // fetchPackageByName expects "Author-ModName" or exact match with full_name

          let searchName = mod.fullName;
          // Try to strip version if present (simple regex for -X.X.X at end)
          searchName = searchName.replace(/-\d+\.\d+\.\d+$/, '');

          console.log("Resolving package for:", mod.fullName, "searching:", searchName);
          return await window.ipcRenderer.fetchPackageByName(searchName, selectedCommunity);
        }}
        onInstallToGame={handleInstallToGameRequest}
        onLaunchProfile={handleLaunchProfileDirect}
        onStopProfile={handleStopProfileDirect}
        isApplying={isApplyingToGame}
        isLaunching={isLaunchingProfile || isStoppingProfile}
        isBusy={isApplyingToGame || isLaunchingProfile || isStoppingProfile || isSteamRestarting}
        isSteamRestarting={isSteamRestarting}
        isGameRunning={isGameRunning}
        hasConfiguredGamePath={!!activeProfileGamePath}
        isCheckingGamePath={isCheckingActiveProfileGamePath}
        onExportProfile={() => setShowExportModal(true)}
        onImportCustomMod={handleImportCustomModRequest}
        onOpenSettings={() => setShowSettings(true)}
        onUpdateProfile={updateProfile}
        onToggleVanilla={handleToggleProfileVanilla}
        onUpdateMod={handleProfileModUpdate}
        onUpdateAll={setPendingProfileUpdates}
        onSyncPending={handleSyncPending}
        onRevertPending={handleRevertPending}
        runtimeHealth={runtimeHealth}
        isRepairingRuntime={isRepairingRuntime}
        onRepairRuntime={async () => { await repairProfileRuntime(); }}
      />
    );

    const main = (
      <div className="r2-app-backdrop flex-1 flex flex-col min-w-0 bg-gray-900 h-full">
        <div className="px-[clamp(1rem,3vw,50px)] py-5 border-b border-gray-800 flex flex-wrap items-center justify-between gap-4 flex-shrink-0">
          <div className="flex shrink-0 items-center gap-4">
            {isBrowsingMode && (
              <Button variant="ghost" size="sm" onClick={() => setIsBrowsingMode(false)}>
                ← Exit
              </Button>
            )}
            <h1 className="whitespace-nowrap text-2xl font-bold text-white">Browse Mods</h1>
          </div>
          <div className="flex min-w-0 flex-1 flex-wrap items-center justify-end gap-3">
            <div className="relative flex bg-gray-800 rounded-lg p-1 border border-gray-700 overflow-hidden">
              {/* Sliding Background */}
              <div
                className={`absolute top-1 bottom-1 w-[calc(50%-4px)] bg-gray-600 rounded-md transition-all duration-300 ease-[cubic-bezier(0.25,0.1,0.25,1)] ${viewMode === 'grid' ? 'left-1' : 'left-1/2'
                  }`}
              />
              <button
                onClick={() => setViewMode('grid')}
                className={`relative z-10 p-2 rounded w-10 flex items-center justify-center transition-colors ${viewMode === 'grid' ? 'text-white' : 'text-gray-400 hover:text-white'}`}
                title="Grid View"
              >
                <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2V6zM14 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2V6zM4 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2v-2zM14 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2v-2z" />
                </svg>
              </button>
              <button
                onClick={() => setViewMode('list')}
                className={`relative z-10 p-2 rounded w-10 flex items-center justify-center transition-colors ${viewMode === 'list' ? 'text-white' : 'text-gray-400 hover:text-white'}`}
                title="List View"
              >
                <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6h16M4 12h16M4 18h16" />
                </svg>
              </button>
            </div>
            <FilterPopover
              options={filterOptions}
              onChange={setFilterOptions}
              availableCategories={availableCategories}
            />
            <div className="min-w-48 max-w-80 flex-1 basis-48">
              <SearchBar value={searchQuery} onChange={setSearchQuery} />
            </div>
          </div>
        </div>

        <div className="flex-1 overflow-hidden relative flex flex-col">
          {/* Show loading overlay only on initial load when no packages are displayed yet */}
          {loadingMods && packages.length === 0 && (
            <div className="absolute inset-0 flex items-center justify-center z-10">
              <div className="text-center flex flex-col items-center">
                <svg className="animate-spin h-10 w-10 text-blue-500 mb-4" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
                  <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4"></circle>
                  <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                </svg>
                <p className="text-gray-400">Fetching packages...</p>
              </div>
            </div>
          )}

          {/* Always keep grid mounted to preserve scroll position */}
          <VirtualizedModGrid
            packages={packages}
            installedMods={activeProfile?.mods || []}
            onInstall={handleInstallRequest}
            onUninstall={handleUninstallWithDependencies}
            onModClick={setSelectedMod}
            viewMode={viewMode}
            isBrowsing={isBrowsingMode}
            searchQuery={searchQuery}
            legacyInstallMode={legacyInstallMode}
            onLoadMore={loadMorePackages}
            hasMore={packages.length < totalPackages}
            isLoadingMore={isFetchingNextPage}
            totalCount={totalPackages}
          />
        </div>
      </div>
    );

    content = <>
      {gameCommands}
      {gameShortcuts}
      <Layout
      sidebar={isBrowsingMode ? null : sidebar}
      main={main}
      isSidebarOpen={isBrowsingMode ? false : isSidebarOpen}
      onToggleSidebar={() => !isBrowsingMode && setIsSidebarOpen(!isSidebarOpen)}
      />
    </>;
  }

  // WRAPPER
  return (
    <div className="r2-app-backdrop h-screen w-screen flex flex-col bg-gray-900 overflow-hidden">
      {/* Scrollable Content Area */}
      <div className="flex-1 overflow-hidden relative">
        {content}
      </div>

      {/* Mounted here rather than per screen: search has to answer on the game
          list too, where none of the view-scoped listeners exist. */}
      <KeyboardShortcuts
        enabled={!showSettings && !showPreferences && !showExportModal && !showUpdateModal}
        handlers={{
          'open-search': () => togglePalette(
            activeProfilePaletteScope ?? selectedGamePaletteScope
          ),
          'open-preferences': () => {
            setPreferencesPanel(null);
            setShowPreferences(true);
          },
          'go-home': () => {
            flushSync(() => {
              setSelectedMod(null);
              selectProfile('');
              setIsBrowsingMode(false);
              setSelectedCommunity(null);
            });
          },
        }}
      />

      <CommandPalette />

      {isCustomModDragActive && (
        <div className="fixed inset-0 z-[55] bg-black/55 backdrop-blur-sm flex items-center justify-center pointer-events-none p-6">
          <div className={`w-full max-w-md rounded-xl border-2 border-dashed bg-gray-900/90 px-6 py-8 text-center shadow-2xl transition-colors duration-200 ${
            isCustomModDragValid ? 'border-blue-400/70' : 'border-red-500/70'
          }`}>
            <div className={`mx-auto mb-4 h-12 w-12 rounded-xl border flex items-center justify-center transition-colors duration-200 ${
              isCustomModDragValid
                ? 'bg-blue-500/15 border-blue-400/30 text-fg-accent'
                : 'bg-red-500/15 border-red-500/30 text-fg-danger'
            }`}>
              {isCustomModDragValid ? (
                <svg xmlns="http://www.w3.org/2000/svg" className="h-7 w-7" fill="none" viewBox="0 0 24 24" stroke="currentColor" aria-hidden="true">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 11v6m3-3H9" />
                </svg>
              ) : (
                <svg xmlns="http://www.w3.org/2000/svg" className="h-7 w-7" fill="none" viewBox="0 0 24 24" stroke="currentColor" aria-hidden="true">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
                </svg>
              )}
            </div>
            <div className={`text-lg font-bold transition-colors duration-200 ${isCustomModDragValid ? 'text-white' : 'text-fg-danger'}`}>
              {isCustomModDragValid ? 'Drop Custom Mod' : 'Invalid File Type'}
            </div>
            <div className="mt-1 text-sm text-gray-400">
              {isCustomModDragValid
                ? 'Folder, .zip, or .r2z'
                : 'Only folders, .zip, or .r2z files are allowed'}
            </div>
          </div>
        </div>
      )}

      {isProgressMinimized && progressState.isOpen && (
        <button
          type="button"
          onClick={() => setIsProgressMinimized(false)}
          className="fixed bottom-5 right-5 z-[58] w-72 overflow-hidden rounded-xl border border-gray-700 bg-gray-800/95 p-3 text-left shadow-2xl backdrop-blur-md transition hover:border-gray-600 hover:bg-gray-800"
          aria-label="Show background download progress"
          title="Show download progress"
        >
          <div className="flex items-center gap-3">
            <div className="relative flex h-9 w-9 shrink-0 items-center justify-center overflow-hidden rounded-lg bg-blue-500/15 text-fg-accent">
              <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" viewBox="0 0 24 24" fill="none" aria-hidden="true">
                <defs>
                  <mask id="background-download-glyph-mask" maskUnits="userSpaceOnUse" x="0" y="0" width="24" height="24">
                    <g fill="none" stroke="white" strokeWidth="2.25" strokeLinecap="round" strokeLinejoin="round">
                      <path d="M12 3.5v11" />
                      <path d="m7.75 10.75 4.25 4.25 4.25-4.25" />
                      <path d="M5 19.5h14" />
                    </g>
                  </mask>
                  <linearGradient id="background-download-sweep" x1="0" y1="0" x2="0" y2="1">
                    {/* The sweep was three hardcoded blues, so it stayed the
                        stock cyan whatever theme was on — the one thing on this
                        banner that never followed the accent. It reads from the
                        same tokens as everything else now. */}
                    <stop offset="0" style={{ stopColor: 'rgb(var(--r2-blue-400))' }} stopOpacity="0" />
                    <stop offset="0.28" style={{ stopColor: 'rgb(var(--r2-blue-300))' }} stopOpacity="0.55" />
                    <stop offset="0.5" style={{ stopColor: 'rgb(var(--r2-blue-50))' }} stopOpacity="0.95" />
                    <stop offset="0.72" style={{ stopColor: 'rgb(var(--r2-blue-300))' }} stopOpacity="0.55" />
                    <stop offset="1" style={{ stopColor: 'rgb(var(--r2-blue-400))' }} stopOpacity="0" />
                  </linearGradient>
                  <filter id="background-download-soft-glow" x="-30%" y="-30%" width="160%" height="160%">
                    <feGaussianBlur stdDeviation="0.65" />
                  </filter>
                </defs>
                <g fill="none" stroke="currentColor" strokeWidth="2.25" strokeLinecap="round" strokeLinejoin="round" opacity="0.42">
                  <path d="M12 3.5v11" />
                  <path d="m7.75 10.75 4.25 4.25 4.25-4.25" />
                  <path d="M5 19.5h14" />
                </g>
                <g mask="url(#background-download-glyph-mask)">
                  <rect
                    className="download-glyph-sweep"
                    x="0"
                    y="-12"
                    width="24"
                    height="12"
                    fill="url(#background-download-sweep)"
                    filter="url(#background-download-soft-glow)"
                  />
                </g>
              </svg>
            </div>
            <div className="min-w-0 flex-1">
              <div className="flex items-center justify-between gap-2">
                <span className="truncate text-sm font-semibold text-white">Applying in background</span>
                <span className="shrink-0 text-xs tabular-nums text-fg-accent">{Math.round(progressState.progress)}%</span>
              </div>
              <div className="mt-0.5 truncate text-xs text-gray-400">{progressState.currentTask}</div>
            </div>
          </div>
          <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-gray-700">
            <div
              className="h-full rounded-full bg-blue-500 transition-all duration-300"
              style={{ width: `${Math.min(100, Math.max(0, progressState.progress))}%` }}
            />
          </div>
        </button>
      )}



      {/* Modals */}
      <LaunchIssueModal issue={launchIssue} onClose={() => setLaunchIssue(null)} />
      <UpdateAllModal
        isOpen={pendingProfileUpdates.length > 0 && !selectedMod}
        updates={pendingProfileUpdates}
        isUpdating={isUpdatingProfile}
        onClose={() => setPendingProfileUpdates([])}
        onConfirm={() => { void confirmProfileUpdates(); }}
        onViewMod={setSelectedMod}
      />
      <AppModals
        selectedMod={selectedMod}
        setSelectedMod={setSelectedMod}
        activeProfileId={activeProfileId}
        profiles={profiles}
        selectedCommunity={selectedCommunity}
        handleInstallMod={handleInstallRequest}
        handleUpdateMod={handleProfileModUpdate}
        handleUninstallWithDependencies={handleUninstallWithDependencies}
        isBrowsingMode={isBrowsingMode}
        progressState={progressState}
        isProgressMinimized={isProgressMinimized}
        setProgressState={setProgressState}
        onCancelProgress={handleCancelProgress}
        onMinimizeProgress={handleMinimizeProgress}
        isCancellingProgress={isCancellingCustomModImport}
        uninstallModalState={uninstallModalState}
        setUninstallModalState={setUninstallModalState}
        executeUninstall={executeUninstall}
        showSettings={showSettings}
        setShowSettings={setShowSettings}
        showExportModal={showExportModal}
        setShowExportModal={setShowExportModal}
        handleExportCode={handleExportCode}
        handleExportFile={handleExportFile}
        showUpdateModal={showUpdateModal}
        setShowUpdateModal={setShowUpdateModal}
        updateInfo={updateInfo}
        showCrossOverGuide={showCrossOverGuide}
        setShowCrossOverGuide={setShowCrossOverGuide}
        hideCrossOverGuide={hideCrossOverGuide}
        setHideCrossOverGuide={setHideCrossOverGuide}
        showPreferences={showPreferences}
        preferencesInitialPanel={preferencesPanel}
        setShowPreferences={setShowPreferences}
        preferences={{
          legacy_install_mode: legacyInstallMode,
          ask_version_before_install: askVersionBeforeInstall,
          install_in_parallel: installInParallel,
          confirm_before_apply_to_game: confirmBeforeApplyToGame,
          write_debug_logs_to_game: writeDebugLogsToGame,
          verbose_logging: verboseLogging,
          default_mod_view_mode: defaultModViewMode,
          show_deprecated_warnings: showDeprecatedWarnings,
          sponsored_messages_enabled: sponsoredMessagesEnabled,
          sponsored_messages_scale: sponsoredMessagesScale,
          sponsored_messages_background_opacity: sponsoredMessagesOpacity,
          stream_mode: streamMode,
          default_game: defaultGame,
          default_profile: defaultProfile,
          keybinds: overridesFromKeybinds(activeKeybinds),
        }}
        communities={communities}
        communityImages={communityImages}
        communityPlatforms={communityPlatforms}
        onSavePreferences={handleSavePreferences}
        onSponsorPreferencesChange={handleSponsorPreferencesChange}
        hasHiddenGuideWarnings={hideCrossOverGuide || hideVerboseLogsWarning}
        onRestoreGuideWarnings={handleRestoreGuideWarnings}
        onSetGuideHidden={handleSetGuideHidden}
        legacyInstallMode={legacyInstallMode}
        onCheckForUpdates={forceCheckForUpdates}
        verboseLogsWarningBytes={verboseLogsWarningBytes}
        onVerboseLoggingChange={handleVerboseLoggingFromWarning}
        onClearAppLogs={handleClearAppLogs}
        onDismissVerboseLogsWarning={() => {
          setVerboseLogsWarningDismissed(true)
          setVerboseLogsWarningBytes(null)
        }}
        onHideVerboseLogsWarning={handleHideVerboseLogsWarning}
        codeShareDisabled={selectedCommunity === 'outerwilds'}
      />
    </div>
  )
}

export default App
