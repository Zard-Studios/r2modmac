import { useState, useEffect, useMemo, useRef } from 'react'
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
import { listen } from '@tauri-apps/api/event';
import { AppModals } from './components/screens/AppModals';
import type { AppSettings, UpdateInfo } from './types/electron';
import { MAC_IMAGE_CACHE_KEY, MAC_PLATFORM_CACHE_KEY } from './constants/cacheKeys';
import type { PreferencesSettings } from './components/modals/PreferencesModal';

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
  const [loading, setLoading] = useState(true)
  const [loadingMods, setLoadingMods] = useState(false)
  const [searchQuery, setSearchQuery] = useState('')
  const [filterOptions, setFilterOptions] = useState<FilterOptions>({
    sort: 'downloads',
    sortDirection: 'desc',
    nsfw: false,
    deprecated: false,
    mods: false,
    modpacks: false,
    categories: [],
  })
  const PAGE_SIZE = 10000 // Load all mods at once for instant search
  const [availableCategories, setAvailableCategories] = useState<string[]>([])
  const [isSidebarOpen, setIsSidebarOpen] = useState(true)

  const [selectedMod, setSelectedMod] = useState<Package | null>(null)
  // Game Selector state moved to component
  const [progressState, setProgressState] = useState({
    isOpen: false,
    title: '',
    progress: 0,
    currentTask: ''
  })
  const [showSettings, setShowSettings] = useState(false)
  const [showExportModal, setShowExportModal] = useState(false)
  const [showCrossOverGuide, setShowCrossOverGuide] = useState(false)
  const [hideCrossOverGuide, setHideCrossOverGuide] = useState(false)
  const [showMacOSGuide, setShowMacOSGuide] = useState(false)
  const [hideMacOSGuide, setHideMacOSGuide] = useState(false)
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null)
  const [uninstallModalState, setUninstallModalState] = useState<{
    isOpen: boolean;
    pkg: Package | null;
    orphanDeps: { name: string; icon?: string }[];
    allInstalledDeps: string[];
    profileId: string | null;
  }>({
    isOpen: false,
    pkg: null,
    orphanDeps: [],
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
  const [defaultModViewMode, setDefaultModViewMode] = useState<'grid' | 'list'>('grid')
  const [isBrowsingMode, setIsBrowsingMode] = useState(false)
  const isInitialLoadRunningRef = useRef(false)

  const {
    profiles,
    createProfile,
    loadProfiles,
    activeProfileId,
    selectProfile,
    deleteProfile,
    updateProfile,
    removeMod,
    toggleMod
  } = useProfileStore()
  // App State Store
  const { communities, communityImages, communityPlatforms, setCommunities, setCommunityImages, setCommunityPlatforms } = useAppStore();

  const [selectedCommunity, setSelectedCommunity] = useState<string | null>(null)

  useEffect(() => {
    loadData()
    loadProfiles()
    checkForUpdates()

    // Load app preferences
    window.ipcRenderer.getSettings().then((s: AppSettings) => {
      setLegacyInstallMode(!!s.legacy_install_mode);
      setAskVersionBeforeInstall(s.ask_version_before_install ?? true);
      setInstallInParallel(s.install_in_parallel ?? true);
      setConfirmBeforeApplyToGame(!!s.confirm_before_apply_to_game);
      const storedViewMode = s.default_mod_view_mode === 'list' ? 'list' : 'grid';
      setDefaultModViewMode(storedViewMode);
      setViewMode(storedViewMode);
      setHideCrossOverGuide(!!s.hide_crossover_guide);
      setHideMacOSGuide(!!s.hide_macos_guide);
    });

    // Listen for preferences menu event
    const unlistenPrefs = listen('show-preferences', () => {
      setShowPreferences(true);
    });

    return () => {
      unlistenPrefs.then(fn => fn());
    };
  }, [])

  useEffect(() => {
    const unlistenPackages = listen<{ game_id: string, total_count: number }>('packages-loaded', (event) => {
      console.log(`[packages-loaded] Game ${event.payload.game_id} now has ${event.payload.total_count} packages`);

      if (selectedCommunity && event.payload.game_id === selectedCommunity) {
        loadPackages(selectedCommunity, 0, true);
      }
    });

    return () => {
      unlistenPackages.then(fn => fn());
    };
  }, [selectedCommunity])

  const checkForUpdates = async () => {
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

  useEffect(() => {
    if (selectedCommunity) {
      // Initial load for game (categories now fetched inside loadPackages after cache is populated)
      loadPackages(selectedCommunity, 0, true)
      // Reset profile selection when changing game
      if (activeProfileId) {
        selectProfile('')
      }
    }
  }, [selectedCommunity])

  // Client-Side Search (derived state via useMemo - zero flicker!)
  const packages = useMemo(() => {
    const filtered = allPackages;

    if (!searchQuery.trim()) {
      return filtered;
    }
    const searchLower = searchQuery.toLowerCase()
    return filtered.filter(pkg =>
      pkg.name.toLowerCase().includes(searchLower) ||
      pkg.full_name.toLowerCase().includes(searchLower)
    )
  }, [searchQuery, allPackages])

  // Sort/Filter Effect
  useEffect(() => {
    if (selectedCommunity) {
      loadPackages(selectedCommunity, 0, true)
    }
  }, [filterOptions])

  // Update Search Effect to depend on sortOrder? No, loadPackages uses current state.
  // Actually, loadPackages reads sortOrder from state closure.

  // loadData fetches the communities

  const loadData = async () => {
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

  const handleSelectProfile = (profileId: string) => {
    setIsBrowsingMode(false);
    selectProfile(profileId);
  };

  const loadPackages = async (communityId: string, pageNum: number, reset: boolean = false) => {
    // Prevent duplicate calls while loading
    if (loadingMods) return;
    setLoadingMods(true)

    if (reset) {
      setAllPackages([])
    }

    try {
      if (pageNum === 0 && reset) {
        await window.ipcRenderer.fetchPackages(communityId)
        // Now that cache is populated, fetch available categories
        const cats = await window.ipcRenderer.getAvailableCategories(communityId)
        setAvailableCategories(cats)
      }

      const newPackages = await window.ipcRenderer.getPackages(
        communityId,
        pageNum,
        PAGE_SIZE,
        '', // Empty search - filter client-side instead
        filterOptions.sort,
        filterOptions.nsfw,
        filterOptions.deprecated,
        filterOptions.sortDirection,
        filterOptions.categories,
        filterOptions.mods,
        filterOptions.modpacks
      )



      // Update allPackages (packages is derived via useMemo)
      const updated = reset ? newPackages : [...allPackages, ...newPackages]
      setAllPackages(updated)
    } catch (err) {
      console.error('Failed to load packages', err)
    } finally {
      setLoadingMods(false)
    }
  }



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
    hideMacOSGuide,
    setProgressState,
    setShowCrossOverGuide,
    setShowMacOSGuide,
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

  const handleInstallToGameRequest = async (isVanillaOverride?: boolean) => {
    if (!confirmBeforeApplyToGame || isVanillaOverride !== undefined) {
      await handleSyncToGame(isVanillaOverride);
      return;
    }

    const confirmed = await window.ipcRenderer.confirm(
      'Apply Profile to Game?',
      'This will sync your profile mods into the game directory. Continue?'
    );
    if (!confirmed) return;

    await handleSyncToGame();
  };

  const handleSavePreferences = async (newSettings: PreferencesSettings) => {
    setLegacyInstallMode(newSettings.legacy_install_mode);
    setAskVersionBeforeInstall(newSettings.ask_version_before_install);
    setInstallInParallel(newSettings.install_in_parallel);
    setConfirmBeforeApplyToGame(newSettings.confirm_before_apply_to_game);
    setDefaultModViewMode(newSettings.default_mod_view_mode);
    setViewMode(newSettings.default_mod_view_mode);

    const currentSettings = await window.ipcRenderer.getSettings();
    await window.ipcRenderer.saveSettings({
      ...currentSettings,
      legacy_install_mode: newSettings.legacy_install_mode,
      ask_version_before_install: newSettings.ask_version_before_install,
      install_in_parallel: newSettings.install_in_parallel,
      confirm_before_apply_to_game: newSettings.confirm_before_apply_to_game,
      default_mod_view_mode: newSettings.default_mod_view_mode,
    });
  };

  const handleSetGuideHidden = async (guide: 'crossover' | 'macos', hidden: boolean) => {
    if (guide === 'crossover') {
      setHideCrossOverGuide(hidden);
    } else {
      setHideMacOSGuide(hidden);
    }

    const currentSettings = await window.ipcRenderer.getSettings();
    await window.ipcRenderer.saveSettings({
      ...currentSettings,
      hide_crossover_guide: guide === 'crossover' ? hidden : !!currentSettings.hide_crossover_guide,
      hide_macos_guide: guide === 'macos' ? hidden : !!currentSettings.hide_macos_guide,
    });
  };

  const handleRestoreGuideWarnings = async () => {
    setHideCrossOverGuide(false);
    setHideMacOSGuide(false);

    const currentSettings = await window.ipcRenderer.getSettings();
    await window.ipcRenderer.saveSettings({
      ...currentSettings,
      hide_crossover_guide: false,
      hide_macos_guide: false,
    });

    await window.ipcRenderer.alert(
      'Warnings Restored',
      'Setup warnings have been re-enabled. They will be shown again when needed.'
    );
  };


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
          onSelectProfile={handleSelectProfile}
          onCreateProfile={(name, platform) => createProfile(name, selectedCommunity!, platform)}
          onImportProfile={handleImportProfile}
          onImportFile={handleImportFile}
          onBrowseMods={() => setIsBrowsingMode(true)}
          onDeleteProfile={deleteProfile}
          onUpdateProfile={updateProfile}
          onToggleVanilla={async (profileId, newVanillaState) => {
            // Update profile state
            updateProfile(profileId, { is_vanilla: newVanillaState });
            // Find the profile to get its mods
            const profile = profiles.find(p => p.id === profileId);
            if (profile) {
              const disabledMods = profile.mods.filter(m => !m.enabled).map(m => m.fullName);
              // Apply directly to game with vanilla override
              await window.ipcRenderer.installToGame(selectedCommunity, profileId, disabledMods, newVanillaState);
              updateProfile(profileId, {
                needs_sync: false,
                mods: profile.mods.map((m) => ({
                  ...m,
                  pending_sync: false,
                  synced_enabled: m.enabled,
                })),
              });
            }
          }}
        />
      </div>
    );
  } else {
    // STEP 3: MOD MANAGEMENT
    const activeProfile = profiles.find(p => p.id === activeProfileId);
    const currentCommunity = communities.find(c => c.identifier === selectedCommunity);

    const sidebar = (
      <ProfileSidebar
        key={activeProfile?.id || 'no-profile'}
        activeProfile={activeProfile}
        currentCommunity={currentCommunity || null}
        communityImage={currentCommunity ? communityImages[currentCommunity.identifier] : undefined}
        packages={packages}
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
          await removeMod(activeProfile.id, mod.uuid4);
        }}
        onResolvePackage={async (mod) => {
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
        onExportProfile={() => setShowExportModal(true)}
        onOpenSettings={() => setShowSettings(true)}
        onUpdateProfile={updateProfile}
      />
    );

    const main = (
      <div className="flex-1 flex flex-col min-w-0 bg-gray-900 h-full">
        <div className="p-5 border-b border-gray-800 flex items-center justify-between gap-4 flex-shrink-0">
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
        showMacOSGuide={showMacOSGuide}
        setShowMacOSGuide={setShowMacOSGuide}
        hideMacOSGuide={hideMacOSGuide}
        setHideMacOSGuide={setHideMacOSGuide}
        showPreferences={showPreferences}
        setShowPreferences={setShowPreferences}
        preferences={{
          legacy_install_mode: legacyInstallMode,
          ask_version_before_install: askVersionBeforeInstall,
          install_in_parallel: installInParallel,
          confirm_before_apply_to_game: confirmBeforeApplyToGame,
          default_mod_view_mode: defaultModViewMode,
        }}
        onSavePreferences={handleSavePreferences}
        hasHiddenGuideWarnings={hideCrossOverGuide || hideMacOSGuide}
        onRestoreGuideWarnings={handleRestoreGuideWarnings}
        onSetGuideHidden={handleSetGuideHidden}
        legacyInstallMode={legacyInstallMode}
      />
    </div>
  )
}

export default App
