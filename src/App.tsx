import { useState, useEffect, useRef, useCallback } from 'react'
import { Button } from './components/ui'
import { Layout } from './components/Layout'
import type { FilterOptions } from './components/FilterPopover'
import { FilterPopover } from './components/FilterPopover'
import { GameSelectionScreen } from './components/screens/GameSelectionScreen'
import { SearchBar } from './components/SearchBar'
import { VirtualizedModGrid } from './components/VirtualizedModGrid'
import { ProfileList } from './components/profiles/ProfileList';
import { ProfileSidebar } from './components/profiles/ProfileSidebar';
import { useProfileStore } from './store/useProfileStore';
import { useAppStore } from './store/useAppStore';
import type { CommunityPlatformInfo, Package, PackageVersion } from './types/thunderstore';
import { getVersion } from '@tauri-apps/api/app';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { flushSync } from 'react-dom';
import { AppModals } from './components/screens/AppModals';
import type { AppSettings, UpdateInfo } from './types/electron';
import type { InstalledMod } from './types/profile';
import { MAC_IMAGE_CACHE_KEY, MAC_PLATFORM_CACHE_KEY } from './constants/cacheKeys';
import type { PreferencesSettings } from './components/modals/PreferencesModal';
import type { ProgressState } from './types/progress';

import { useModActions } from './hooks/useModActions';
import { useProfileActions } from './hooks/useProfileActions';
import { useGameSync } from './hooks/useGameSync';

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
]);

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
  const [isCancellingCustomModImport, setIsCancellingCustomModImport] = useState(false)
  const [isCustomModDragActive, setIsCustomModDragActive] = useState(false)
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
  const [showPreferences, setShowPreferences] = useState(false)
  const [legacyInstallMode, setLegacyInstallMode] = useState(false)
  const [askVersionBeforeInstall, setAskVersionBeforeInstall] = useState(true)
  const [installInParallel, setInstallInParallel] = useState(true)
  const [confirmBeforeApplyToGame, setConfirmBeforeApplyToGame] = useState(false)
  const [writeDebugLogsToGame, setWriteDebugLogsToGame] = useState(false)
  const [defaultModViewMode, setDefaultModViewMode] = useState<'grid' | 'list'>('grid')
  const [isBrowsingMode, setIsBrowsingMode] = useState(false)
  const isInitialLoadRunningRef = useRef(false)
  const packagesLoadRequestRef = useRef(0)
  const autoApplyProfileRef = useRef<string | null>(null)

  const {
    profiles,
    createProfile,
    loadProfiles,
    activeProfileId,
    selectProfile,
    deleteProfile,
    updateProfile,
    addMod,
    removeMod,
    toggleMod
  } = useProfileStore()
  // App State Store
  const { communities, communityImages, communityPlatforms, streamMode, setCommunities, setCommunityImages, setCommunityPlatforms, setStreamMode, setUsername } = useAppStore();

  const [selectedCommunity, setSelectedCommunity] = useState<string | null>(null)
  const activeProfile = profiles.find((profile) => profile.id === activeProfileId) ?? null
  const [activeProfileGamePath, setActiveProfileGamePath] = useState<string | null>(null)
  const [isCheckingActiveProfileGamePath, setIsCheckingActiveProfileGamePath] = useState(false)
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

  async function loadData() {
    if (isInitialLoadRunningRef.current) return;

    // If we already have communities, don't re-fetch them.
    if (communities.length > 0) return;

    isInitialLoadRunningRef.current = true;
    setLoading(true)
    try {
      const data = await window.ipcRenderer.fetchCommunities();
      setCommunities(data)
      console.log(`[communities] loaded ${data.length} communities`);

      const storedPlatformCache = readMacPlatformCache();
      const storedImageCache = readMacImageCache();
      const knownGamesSet = new Set(storedPlatformCache.known_games);
      let sessionImages: Record<string, string> = {};

      try {
        sessionImages = await window.ipcRenderer.fetchCommunityImages();
        setCommunityImages(sessionImages);
      } catch (imgErr) {
        console.warn('[community-images] failed to fetch image map, using cached mac images', imgErr);
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

  async function loadPackages(communityId: string, pageNum: number, reset: boolean = false) {
    const requestId = ++packagesLoadRequestRef.current;
    const isStaleRequest = () => requestId !== packagesLoadRequestRef.current;

    if (reset) {
      setLoadingMods(true);
      setAllPackages([]);
      setTotalPackages(0);
      setCurrentPage(0);
      setAvailableCategories([]);
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
      if (requestId === packagesLoadRequestRef.current) {
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
      loadData()
      loadProfiles()
      checkForUpdates()
    }, 0);

    // Load app preferences
    window.ipcRenderer.getSettings().then((s: AppSettings) => {
      setLegacyInstallMode(!!s.legacy_install_mode);
      setAskVersionBeforeInstall(s.ask_version_before_install ?? true);
      setInstallInParallel(s.install_in_parallel ?? true);
      setConfirmBeforeApplyToGame(!!s.confirm_before_apply_to_game);
      setWriteDebugLogsToGame(s.write_debug_logs_to_game ?? false);
      const storedViewMode = s.default_mod_view_mode === 'list' ? 'list' : 'grid';
      setDefaultModViewMode(storedViewMode);
      setViewMode(storedViewMode);
      setHideCrossOverGuide(!!s.hide_crossover_guide);
      setStreamMode(!!s.stream_mode);
    });

    window.ipcRenderer.getUsername().then((u: string) => {
      setUsername(u);
    }).catch((err) => {
      console.error('Failed to get username', err);
    });

    // Listen for preferences menu event
    const unlistenPrefs = listen('show-preferences', () => {
      setShowPreferences(true);
    });

    const unlistenStorageVolumes = listen('storage-volumes-changed', () => {
      setStorageVolumeEventCount((count) => count + 1);
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
      unlistenSteamLaunchOptionsRestart.then(fn => fn());
    };
  }, [])

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
        text: 'Aggiorna',
        action: () => {
          window.location.reload();
        }
      }));

      if (SHOW_DEVTOOLS_CONTEXT_MENU_ITEM) {
        items.push(await MI.new({
          text: 'Ispeziona pagina',
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
    const { profiles, activeProfileId } = useProfileStore.getState();
    const profile = profiles.find((p) => p.id === activeProfileId) ?? null;
    if (!profile || profile.mods.length === 0) {
      setProfilePackageIndex({});
      return;
    }

    const modNames = profile.mods
      .filter(m => m.source !== 'local')
      .map(m => m.fullName.replace(/-\d+\.\d+\.\d+$/, ''));

    if (modNames.length === 0) {
      setProfilePackageIndex({});
      return;
    }

    try {
      const result = await window.ipcRenderer.lookupPackagesByNames(communityId, modNames);
      const index: Record<string, Package> = {};
      if (result?.found) {
        for (const pkg of result.found) {
          index[pkg.full_name] = pkg;
        }
      }
      setProfilePackageIndex(index);
    } catch (err) {
      console.error('Failed to build profile package index', err);
      setProfilePackageIndex({});
    }
  }, []);

  useEffect(() => {
    const unlistenPackages = listen<{ game_id: string, total_count: number }>('packages-loaded', (event) => {
      console.log(`[packages-loaded] Game ${event.payload.game_id} now has ${event.payload.total_count} packages`);
      const current = selectedCommunityRef.current;
      if (current && event.payload.game_id === current) {
        loadPackages(current, 0, true);
        rebuildProfilePackageIndex(current);
      }
    });

    return () => {
      unlistenPackages.then(fn => fn());
    };
  }, [])



  useEffect(() => {
    if (selectedCommunity) {
      // Initial load for game (categories now fetched inside loadPackages after cache is populated)
      setTimeout(() => {
        loadPackages(selectedCommunity, 0, true)
        // Reset profile selection when changing game
        if (activeProfileId) {
          selectProfile('')
        }
      }, 0);
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
  }, [activeProfile?.id, selectedCommunity, activeProfile?.mods.length, rebuildProfilePackageIndex])



  const handleSelectProfile = (profileId: string) => {
    setIsBrowsingMode(false);
    selectProfile(profileId);
  };

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
    legacyInstallMode,
    installInParallel,
    setProgressState,
    onInstallMod: handleInstallMod,
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
    if (!progressState.isOpen || !progressState.isCancelable || isCancellingCustomModImport) return;

    customModImportCancelledRef.current = true;
    setIsCancellingCustomModImport(true);
    setProgressState(prev => ({
      ...prev,
      currentTask: 'Cancelling custom mod import...'
    }));

    try {
      await window.ipcRenderer.cancelCustomModImport();
    } catch (error) {
      console.error('Failed to cancel custom mod import', error);
    }
  };

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
        const pkg = foundPackages.find((p) => p.full_name === modName);
        if (!pkg) {
          throw new Error('Package not found after lookup');
        }
        const version = pkg.versions.find((v) => v.version_number === mod.version) || pkg.versions[0];
        if (!version) {
          throw new Error('Package has no available versions');
        }

        const installedMod: InstalledMod = {
          uuid4: version.uuid4,
          fullName: version.full_name,
          versionNumber: version.version_number,
          iconUrl: version.icon,
          enabled: mod.enabled ?? true,
          pending_sync: true,
          synced_enabled: undefined,
        };
        addMod(targetProfile.id, installedMod);
        importedCount++;
      } catch (error) {
        console.error(`Failed to add profile mod ${modName}`, error);
        failedMods.push(modName);
      } finally {
        completedSteps++;
        updateMergeProgress(`Processed ${completedSteps}/${Math.max(totalSteps, 1)}...`);
      }
    }

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
        failedMods.push(modName);
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
      const payload = event.payload;
      if (payload.type === 'enter') {
        setIsCustomModDragActive(payload.paths.length > 0);
        return;
      }
      if (payload.type === 'over') {
        setIsCustomModDragActive(true);
        return;
      }
      if (payload.type === 'leave') {
        setIsCustomModDragActive(false);
        return;
      }
      if (payload.type === 'drop') {
        setIsCustomModDragActive(false);
        if (progressState.isOpen || isCancellingCustomModImport) return;
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
  }, [importCustomModPaths, progressState.isOpen, isCancellingCustomModImport]);

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
      if (options?.skipConfirm || !confirmBeforeApplyToGame || isVanillaOverride !== undefined) {
        await handleSyncToGame(isVanillaOverride, { silentSuccess: options?.silentSuccess });
        return;
      }

      const confirmed = await window.ipcRenderer.confirm(
        'Apply Profile to Game?',
        'This will sync your profile mods into the game directory. Continue?'
      );
      if (!confirmed) return;

      await handleSyncToGame(undefined, { silentSuccess: options?.silentSuccess });
    } finally {
      clearSteamRestartingState();
      applyInFlightRef.current = false;
      setIsApplyingToGame(false);
    }
  };

  useEffect(() => {
    if (!activeProfileId || isBrowsingMode) {
      autoApplyProfileRef.current = null;
      return;
    }

    const selectedProfile = useProfileStore.getState().profiles.find((profile) => profile.id === activeProfileId);
    if (!selectedProfile) {
      return;
    }

    const profileNeedsSync = !!selectedProfile.needs_sync || selectedProfile.mods.some((mod) => mod.pending_sync);
    if (!profileNeedsSync) {
      return;
    }

    if (autoApplyProfileRef.current === activeProfileId) {
      return;
    }

    autoApplyProfileRef.current = activeProfileId;
    void handleInstallToGameRequest(
      undefined,
      { skipConfirm: true, silentSuccess: true }
    );
  }, [activeProfileId, isBrowsingMode]);

  const handleToggleProfileVanilla = async (profileId: string, newVanillaState: boolean) => {
    if (profileActionLockRef.current || applyInFlightRef.current) return;
    const profile = profiles.find((p) => p.id === profileId);
    if (!profile) return;

    const disabledMods = profile.mods.filter((m) => !m.enabled).map((m) => m.fullName);
    const hadPendingSync = !!profile.needs_sync || profile.mods.some((m) => m.pending_sync);

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
  };

  const handleSavePreferences = async (newSettings: PreferencesSettings) => {
    setLegacyInstallMode(newSettings.legacy_install_mode);
    setAskVersionBeforeInstall(newSettings.ask_version_before_install);
    setInstallInParallel(newSettings.install_in_parallel);
    setConfirmBeforeApplyToGame(newSettings.confirm_before_apply_to_game);
    setWriteDebugLogsToGame(newSettings.write_debug_logs_to_game);
    setDefaultModViewMode(newSettings.default_mod_view_mode);
    setViewMode(newSettings.default_mod_view_mode);
    setStreamMode(newSettings.stream_mode);

    const currentSettings = await window.ipcRenderer.getSettings();
    await window.ipcRenderer.saveSettings({
      ...currentSettings,
      legacy_install_mode: newSettings.legacy_install_mode,
      ask_version_before_install: newSettings.ask_version_before_install,
      install_in_parallel: newSettings.install_in_parallel,
      confirm_before_apply_to_game: newSettings.confirm_before_apply_to_game,
      write_debug_logs_to_game: newSettings.write_debug_logs_to_game,
      default_mod_view_mode: newSettings.default_mod_view_mode,
      stream_mode: newSettings.stream_mode,
    });
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

    const currentSettings = await window.ipcRenderer.getSettings();
    await window.ipcRenderer.saveSettings({
      ...currentSettings,
      hide_crossover_guide: false,
      hide_macos_guide: false,
    });

    await window.ipcRenderer.alert(
      'Warnings restored',
      'Setup warnings have been re-enabled. They will be shown again when needed.'
    );
  };



  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Escape' || event.defaultPrevented || event.metaKey || event.ctrlKey || event.altKey) {
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
  ]);


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
        onOpenPreferences={() => setShowPreferences(true)}
        searchQuery={gameSearchQuery}
        onSearchQueryChange={setGameSearchQuery}
      />
    );
  } else if (!activeProfileId && !isBrowsingMode) {
    // STEP 2: PROFILE SELECTION
    const selectedGame = communities.find(c => c.identifier === selectedCommunity);
    const selectedGameCover = selectedGame ? communityImages[selectedGame.identifier] : undefined;

    content = (
      <div className="flex flex-col h-full bg-gray-900 overflow-y-auto">
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
          onBrowseMods={() => setIsBrowsingMode(true)}
          onDeleteProfile={deleteProfile}
          onUpdateProfile={updateProfile}
          onToggleVanilla={handleToggleProfileVanilla}
        />
      </div>
    );
  } else {
    // STEP 3: MOD MANAGEMENT
    const currentCommunity = communities.find(c => c.identifier === selectedCommunity);
    const profileNeedsSync = !!activeProfile?.needs_sync || !!activeProfile?.mods.some((mod) => mod.pending_sync);
    const markActiveProfileUsed = () => {
      if (!activeProfile) return;
      updateProfile(activeProfile.id, { lastUsed: Date.now() });
    };

    const handleLaunchModdedDirect = async () => {
      if (!activeProfile) return;
      if (profileActionLockRef.current || applyInFlightRef.current) return;
      if (activeProfile.is_vanilla) {
        await window.ipcRenderer.alert('Mods Disabled', 'Enable the profile before launching the modded game.');
        return;
      }

      try {
        profileActionLockRef.current = true;
        setIsLaunchingProfile(true);
        if (profileNeedsSync) {
          await handleInstallToGameRequest();
        }
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
        await window.ipcRenderer.alert(
          'Launch Failed',
          String(error?.message || error || 'Failed to launch the modded game.')
        );
      } finally {
        profileActionLockRef.current = false;
        setIsLaunchingProfile(false);
      }
    };

    const handleLaunchVanillaDirect = async () => {
      if (!activeProfile) return;
      if (profileActionLockRef.current || applyInFlightRef.current) return;

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
        await window.ipcRenderer.alert(
          'Launch Failed',
          String(error?.message || error || 'Failed to launch the vanilla game.')
        );
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

    const sidebar = (
      <ProfileSidebar
        key={activeProfile?.id || 'no-profile'}
        activeProfile={activeProfile ?? undefined}
        currentCommunity={currentCommunity || null}
        communityImage={currentCommunity ? communityImages[currentCommunity.identifier] : undefined}
        packageIndex={profilePackageIndex}
        legacyInstallMode={legacyInstallMode}
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
          await removeMod(activeProfile.id, mod.uuid4);
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
      />
    );

    const main = (
      <div className="flex-1 flex flex-col min-w-0 bg-gray-900 h-full">
        <div className="px-[50px] py-5 border-b border-gray-800 flex items-center justify-between gap-4 flex-shrink-0">
          <div className="flex items-center gap-4">
            {isBrowsingMode && (
              <Button variant="ghost" size="sm" onClick={() => setIsBrowsingMode(false)}>
                ← Exit
              </Button>
            )}
            <h1 className="text-2xl font-bold text-white">Browse Mods</h1>
          </div>
          <div className="flex items-center gap-3">
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
            <div className="w-80">
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

    content = <Layout
      sidebar={isBrowsingMode ? null : sidebar}
      main={main}
      isSidebarOpen={isBrowsingMode ? false : isSidebarOpen}
      onToggleSidebar={() => !isBrowsingMode && setIsSidebarOpen(!isSidebarOpen)}
    />;
  }

  // WRAPPER
  return (
    <div className="h-screen w-screen flex flex-col bg-gray-900 overflow-hidden">
      {/* Scrollable Content Area */}
      <div className="flex-1 overflow-hidden relative">
        {content}
      </div>

      {isCustomModDragActive && (
        <div className="fixed inset-0 z-[55] bg-black/55 backdrop-blur-sm flex items-center justify-center pointer-events-none p-6">
          <div className="w-full max-w-md rounded-xl border-2 border-dashed border-blue-400/70 bg-gray-900/90 px-6 py-8 text-center shadow-2xl">
            <div className="mx-auto mb-4 h-12 w-12 rounded-xl bg-blue-500/15 border border-blue-400/30 flex items-center justify-center text-blue-300">
              <svg xmlns="http://www.w3.org/2000/svg" className="h-7 w-7" fill="none" viewBox="0 0 24 24" stroke="currentColor" aria-hidden="true">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 11v6m3-3H9" />
              </svg>
            </div>
            <div className="text-lg font-bold text-white">Drop Custom Mod</div>
            <div className="mt-1 text-sm text-gray-400">Folder, .zip, or .r2z</div>
          </div>
        </div>
      )}



      {/* Modals */}
      <AppModals
        selectedMod={selectedMod}
        setSelectedMod={setSelectedMod}
        activeProfileId={activeProfileId}
        profiles={profiles}
        selectedCommunity={selectedCommunity}
        handleInstallMod={handleInstallRequest}
        handleUpdateMod={handleUpdateMod}
        handleUninstallWithDependencies={handleUninstallWithDependencies}
        isBrowsingMode={isBrowsingMode}
        progressState={progressState}
        setProgressState={setProgressState}
        onCancelProgress={handleCancelProgress}
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
        setShowPreferences={setShowPreferences}
        preferences={{
          legacy_install_mode: legacyInstallMode,
          ask_version_before_install: askVersionBeforeInstall,
          install_in_parallel: installInParallel,
          confirm_before_apply_to_game: confirmBeforeApplyToGame,
          write_debug_logs_to_game: writeDebugLogsToGame,
          default_mod_view_mode: defaultModViewMode,
          stream_mode: streamMode,
        }}
        onSavePreferences={handleSavePreferences}
        hasHiddenGuideWarnings={hideCrossOverGuide}
        onRestoreGuideWarnings={handleRestoreGuideWarnings}
        onSetGuideHidden={handleSetGuideHidden}
        legacyInstallMode={legacyInstallMode}
      />
    </div>
  )
}

export default App
