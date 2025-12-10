import { useState, useEffect } from 'react'
import { Layout } from './components/Layout'
import type { FilterOptions } from './components/FilterPopover'
import { FilterPopover } from './components/FilterPopover'
import { GameSelector } from './components/GameList'
import { SearchBar } from './components/SearchBar'
import { VirtualizedModGrid } from './components/VirtualizedModGrid'
import { ModDetailModal } from './components/ModDetailModal'
import { ProfileList } from './components/ProfileList'
import { ProgressModal } from './components/ProgressModal'
import { UninstallModal } from './components/UninstallModal'
import { SettingsModal } from './components/SettingsModal'
import { ExportModal } from './components/ExportModal'
import { CrossOverGuideModal } from './components/CrossOverGuideModal';
import { ProfileSidebar } from './components/ProfileSidebar';
import { useProfileStore } from './store/useProfileStore'
import type { Community, Package, PackageVersion } from './types/thunderstore'
import type { InstalledMod } from './types/profile'
import { getVersion } from '@tauri-apps/api/app';
import { listen } from '@tauri-apps/api/event';
import { UpdateModal } from './components/UpdateModal';
import PreferencesModal from './components/PreferencesModal';
import type { UpdateInfo } from './types/electron';

function App() {
  const [communities, setCommunities] = useState<Community[]>([])
  const [communityImages, setCommunityImages] = useState<Record<string, string>>({})
  const [packages, setPackages] = useState<Package[]>([])
  const [loading, setLoading] = useState(true)
  const [loadingMods, setLoadingMods] = useState(false)
  const [searchQuery, setSearchQuery] = useState('')
  const [page, setPage] = useState(0)
  const [hasMore, setHasMore] = useState(true)
  const [filterOptions, setFilterOptions] = useState<FilterOptions>({
    sort: 'downloads',
    sortDirection: 'desc',
    nsfw: false,
    deprecated: false,
    mods: false,
    modpacks: false,
    categories: []
  })
  const PAGE_SIZE = 50
  const [availableCategories, setAvailableCategories] = useState<string[]>([])

  const [selectedMod, setSelectedMod] = useState<Package | null>(null)
  const [gameSearchQuery, setGameSearchQuery] = useState('')
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
  const [showUpdateModal, setShowUpdateModal] = useState(false)
  const [showPreferences, setShowPreferences] = useState(false)
  const [legacyInstallMode, setLegacyInstallMode] = useState(false)

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

  const [selectedCommunity, setSelectedCommunity] = useState<string | null>(null)

  useEffect(() => {
    loadData()
    loadProfiles()
    checkForUpdates()

    // Load legacy mode setting
    window.ipcRenderer.getSettings().then((s: any) => {
      if (s.legacy_install_mode !== undefined) {
        setLegacyInstallMode(s.legacy_install_mode);
      }
    });

    // Listen for preferences menu event
    const unlisten = listen('show-preferences', () => {
      setShowPreferences(true);
    });

    return () => {
      unlisten.then(fn => fn());
    };
  }, [])

  const checkForUpdates = async () => {
    try {
      const ver = await getVersion();
      const info = await window.ipcRenderer.checkUpdate(ver);
      if (info.available) {
        setUpdateInfo(info);
        setShowUpdateModal(true);
      }
    } catch (e) {
      console.error("Update check failed", e);
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

  // Search Effect
  useEffect(() => {
    if (selectedCommunity) {
      const timer = setTimeout(() => {
        loadPackages(selectedCommunity, 0, true)
      }, 300) // Debounce search
      return () => clearTimeout(timer)
    }
  }, [searchQuery])

  // Sort/Filter Effect
  useEffect(() => {
    if (selectedCommunity) {
      loadPackages(selectedCommunity, 0, true)
    }
  }, [filterOptions])

  // Update Search Effect to depend on sortOrder? No, loadPackages uses current state.
  // Actually, loadPackages reads sortOrder from state closure.

  const [favoriteGames, setFavoriteGames] = useState<string[]>([])

  const loadData = async () => {
    setLoading(true)
    try {
      const [data, images, settings] = await Promise.all([
        window.ipcRenderer.fetchCommunities(),
        window.ipcRenderer.fetchCommunityImages(),
        window.ipcRenderer.getSettings()
      ])
      setCommunities(data)
      setCommunityImages(images)
      if (settings.favorite_games) {
        setFavoriteGames(settings.favorite_games)
      }
      console.log(`Loaded ${data.length} communities and ${Object.keys(images).length} images`)
    } catch (err) {
      console.error('Failed to load data', err)
    }
    setLoading(false)
  }

  const toggleFavorite = async (identifier: string, e: React.MouseEvent) => {
    e.stopPropagation();
    const newFavorites = favoriteGames.includes(identifier)
      ? favoriteGames.filter(id => id !== identifier)
      : [...favoriteGames, identifier];

    setFavoriteGames(newFavorites);

    // Save to settings
    try {
      const settings = await window.ipcRenderer.getSettings();
      await window.ipcRenderer.saveSettings({
        ...settings,
        favorite_games: newFavorites
      });
    } catch (e) {
      console.error("Failed to save favorites", e);
    }
  }

  const filteredCommunities = communities
    .filter(c =>
      c.name.toLowerCase().includes(gameSearchQuery.toLowerCase()) ||
      c.identifier.toLowerCase().includes(gameSearchQuery.toLowerCase())
    )
    .sort((a, b) => {
      // Sort favorites first
      const aFav = favoriteGames.includes(a.identifier);
      const bFav = favoriteGames.includes(b.identifier);
      if (aFav && !bFav) return -1;
      if (!aFav && bFav) return 1;
      return a.name.localeCompare(b.name);
    });

  const loadPackages = async (communityId: string, pageNum: number, reset: boolean = false) => {
    // Prevent duplicate calls while loading
    if (loadingMods) return;
    setLoadingMods(true)

    if (reset) {
      setPage(0)
      setPackages([])
      setHasMore(true)
    }

    try {
      // First fetch ensures cache is populated (returns count)
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
        searchQuery,
        filterOptions.sort,
        filterOptions.nsfw,
        filterOptions.deprecated,
        filterOptions.sortDirection,
        filterOptions.categories,
        filterOptions.mods,
        filterOptions.modpacks
      )

      if (newPackages.length < PAGE_SIZE) {
        setHasMore(false)
      }

      setPackages(prev => reset ? newPackages : [...prev, ...newPackages])
      setPage(pageNum)
    } catch (err) {
      console.error('Failed to load packages', err)
    } finally {
      setLoadingMods(false)
    }
  }

  const handleLoadMore = () => {
    if (!loadingMods && hasMore && selectedCommunity) {
      loadPackages(selectedCommunity, page + 1, false)
    }
  }

  const installModWithDependencies = async (
    pkg: Package,
    version: PackageVersion,
    installedCache: Set<string> = new Set(),
    targetProfileId?: string,
    progressCounter?: { installed: number; total: number },
    gamePath?: string
  ) => {
    if (installedCache.has(version.full_name)) return;
    installedCache.add(version.full_name);

    const profileIdToUse = targetProfileId || activeProfileId;
    if (!profileIdToUse) throw new Error("No profile selected");
    if (!gamePath) throw new Error("Game path not provided");

    // 1. Collect all dependencies that need to be installed
    const depsToInstall: string[] = [];
    for (const depString of version.dependencies) {
      // depString format: "TeamName-ModName-Version"
      const parts = depString.split('-');
      if (parts.length < 3) continue;
      const depFullName = `${parts[0]}-${parts[1]}`;

      // Check if already installed in profile (skip if so)
      const activeProfile = profiles.find(p => p.id === profileIdToUse);
      if (activeProfile?.mods.some(m => m.fullName.startsWith(depFullName))) {
        console.log(`[Dependencies] Skipping ${depFullName} - already installed`);
        continue;
      }

      // Check if already in cache (being installed in this session)
      if (installedCache.has(depFullName)) {
        console.log(`[Dependencies] Skipping ${depFullName} - in install cache`);
        continue;
      }

      depsToInstall.push(depFullName);
    }

    // 2. Fetch all missing dependencies from backend in one call
    if (depsToInstall.length > 0 && selectedCommunity) {
      console.log(`[Dependencies] Fetching ${depsToInstall.length} dependencies:`, depsToInstall);

      setProgressState(prev => ({
        ...prev,
        currentTask: `Fetching ${depsToInstall.length} dependencies...`
      }));

      try {
        const result = await window.ipcRenderer.lookupPackagesByNames(selectedCommunity, depsToInstall);

        // Update total if we discovered more deps
        if (progressCounter) {
          progressCounter.total += result.found.length;
        }

        // Install each found dependency recursively
        for (const depPkg of result.found) {
          const depVersion = depPkg.versions[0];
          if (depVersion) {
            await installModWithDependencies(depPkg, depVersion, installedCache, profileIdToUse, progressCounter, gamePath);
          }
        }

        // Log any dependencies that weren't found
        if (result.unknown.length > 0) {
          console.warn(`[Dependencies] Could not find: ${result.unknown.join(', ')}`);
        }
      } catch (err) {
        console.error('[Dependencies] Failed to lookup dependencies:', err);
      }
    }

    // 3. Install the mod itself DIRECTLY to game folder
    try {
      // Update progress using shared counter if available
      if (progressCounter) {
        progressCounter.installed++;
        const progress = Math.min(95, Math.round((progressCounter.installed / progressCounter.total) * 100));
        setProgressState(prev => ({
          ...prev,
          progress,
          currentTask: `Installing ${pkg.name}... (${progressCounter.installed}/${progressCounter.total})`
        }));
      } else {
        setProgressState(prev => ({
          ...prev,
          currentTask: `Installing ${pkg.name}...`
        }));
      }

      const result = await window.ipcRenderer.installMod(
        profileIdToUse,
        version.download_url,
        version.full_name,
        gamePath,
        true  // useProfileCache - save to profile cache in legacy mode
      );

      if (result.success) {
        const installedMod: InstalledMod = {
          uuid4: version.uuid4,
          fullName: version.full_name,
          versionNumber: version.version_number,
          iconUrl: version.icon,
          enabled: true
        };
        addMod(profileIdToUse, installedMod);
        console.log(`[Install] Successfully installed ${version.full_name}`);
      } else {
        throw new Error(result.error);
      }
    } catch (err: any) {
      console.error(`Failed to install ${pkg.name}:`, err);
      throw err;
    }
  };

  // Install mod = depends on legacyInstallMode setting
  // Legacy: download immediately to cache (old behavior)
  // New: save metadata only, download with "Apply to Game"
  const handleInstallMod = async (pkg: Package, targetProfileId?: string) => {
    const profileIdToUse = targetProfileId || activeProfileId;
    if (!profileIdToUse) {
      alert('Please select a profile first');
      return;
    }

    const version = pkg.versions[0];

    // LEGACY MODE: download immediately (old behavior)
    if (legacyInstallMode) {
      // Check game path first
      const gamePath = await window.ipcRenderer.getGamePath(selectedCommunity || '');
      if (!gamePath) {
        await window.ipcRenderer.alert(
          "Game Path Required",
          "Please configure the game directory in Settings before installing mods."
        );
        return;
      }

      setProgressState({
        isOpen: true,
        title: `Installing ${pkg.name}`,
        progress: 0,
        currentTask: 'Starting installation...'
      });

      try {
        const progressCounter = { installed: 0, total: 1 };
        await installModWithDependencies(pkg, version, new Set(), profileIdToUse, progressCounter, gamePath);
        setProgressState(prev => ({ ...prev, progress: 100, currentTask: 'Done!' }));
        setTimeout(() => setProgressState(prev => ({ ...prev, isOpen: false })), 500);
      } catch (err: any) {
        console.error('Failed to install mod:', err);
        setProgressState(prev => ({ ...prev, isOpen: false }));
        alert(`Failed to install mod: ${err.message}`);
      }
      return;
    }

    // NEW MODE: metadata only, no download
    setProgressState({
      isOpen: true,
      title: `Adding ${pkg.name}`,
      progress: 0,
      currentTask: 'Resolving dependencies...'
    });

    try {
      // Collect mod + all dependencies as metadata
      const modsToAdd: InstalledMod[] = [];
      const processed = new Set<string>();

      const collectModAndDeps = async (_pkg: Package, ver: PackageVersion) => {
        if (processed.has(ver.full_name)) return;
        processed.add(ver.full_name);

        // Check if already in profile
        const profile = profiles.find(p => p.id === profileIdToUse);
        if (profile?.mods.some(m => m.fullName === ver.full_name)) {
          console.log(`[Add] Skipping ${ver.full_name} - already in profile`);
          return;
        }

        // Add to list
        modsToAdd.push({
          uuid4: ver.uuid4,
          fullName: ver.full_name,
          versionNumber: ver.version_number,
          iconUrl: ver.icon,
          enabled: true
        });

        // Process dependencies
        for (const depString of ver.dependencies) {
          const parts = depString.split('-');
          if (parts.length < 3) continue;
          const depFullName = `${parts[0]}-${parts[1]}`;

          // Check if already in profile
          if (profile?.mods.some(m => m.fullName.startsWith(depFullName))) continue;
          if (processed.has(depFullName)) continue;

          // Lookup dependency
          if (selectedCommunity) {
            const result = await window.ipcRenderer.lookupPackagesByNames(selectedCommunity, [depFullName]);
            for (const depPkg of result.found) {
              const depVer = depPkg.versions[0];
              if (depVer) {
                await collectModAndDeps(depPkg, depVer);
              }
            }
          }
        }
      };

      await collectModAndDeps(pkg, version);

      // Add all mods to profile (metadata only!)
      setProgressState(prev => ({ ...prev, progress: 80, currentTask: `Adding ${modsToAdd.length} mods to profile...` }));

      for (const mod of modsToAdd) {
        addMod(profileIdToUse, mod);
      }

      console.log(`[Install] Added ${modsToAdd.length} mods to profile (metadata only, no download)`);
      setProgressState(prev => ({ ...prev, progress: 100, currentTask: 'Done! Click "Apply to Game" to download.' }));
      setTimeout(() => setProgressState(prev => ({ ...prev, isOpen: false })), 800);
    } catch (err: any) {
      console.error('Failed to add mod:', err);
      setProgressState(prev => ({ ...prev, isOpen: false }));
      alert(`Failed to add mod: ${err.message}`);
    }
  };

  // Select profile - NO auto-sync, user must click "Apply to Game" to sync
  const handleSelectProfile = (profileId: string) => {
    selectProfile(profileId);
  };

  // Uninstall mod with option to remove orphan dependencies - Opens modal
  const handleUninstallWithDependencies = async (pkg: Package, targetProfileId?: string) => {
    const profileIdToUse = targetProfileId || activeProfileId;
    if (!profileIdToUse) return;

    const profile = profiles.find(p => p.id === profileIdToUse);
    if (!profile) return;

    const version = pkg.versions[0];
    if (!version) {
      // No version data, do simple uninstall via modal with no deps
      setUninstallModalState({
        isOpen: true,
        pkg,
        orphanDeps: [],
        allInstalledDeps: [],
        profileId: profileIdToUse
      });
      return;
    }

    // Get dependencies of this mod
    // FILTER OUT BepInExPack IMMEDIATELY
    const modDependencies = version.dependencies
      .map(dep => {
        const parts = dep.split('-');
        return parts.length >= 2 ? `${parts[0]}-${parts[1]}` : dep;
      })
      .filter(dep => !dep.toLowerCase().includes('bepinexpack'));

    // Find which dependencies are "orphan" (not used by other installed mods)
    const orphanDepsDetails: { name: string; icon?: string }[] = [];

    if (modDependencies.length > 0 && selectedCommunity) {
      // Get dependencies of all OTHER installed mods
      const otherMods = profile.mods.filter(m => !m.fullName.startsWith(pkg.full_name));
      const otherModNames = otherMods.map(m => {
        const parts = m.fullName.split('-');
        return parts.length >= 2 ? `${parts[0]}-${parts[1]}` : m.fullName;
      });

      // Lookup other mods to get their dependencies
      let otherModsDeps = new Set<string>();
      if (otherModNames.length > 0) {
        try {
          const result = await window.ipcRenderer.lookupPackagesByNames(selectedCommunity, otherModNames);
          for (const otherPkg of result.found) {
            const otherVer = otherPkg.versions[0];
            if (otherVer) {
              for (const dep of otherVer.dependencies) {
                const parts = dep.split('-');
                const depName = parts.length >= 2 ? `${parts[0]}-${parts[1]}` : dep;
                otherModsDeps.add(depName);
              }
            }
          }
        } catch (err) {
          console.error('Failed to lookup other mods dependencies:', err);
        }
      }

      // Find orphans: deps of mod being removed that are NOT in otherModsDeps AND are installed
      for (const dep of modDependencies) {
        if (!otherModsDeps.has(dep)) {
          // Check if this dep is actually installed
          const installedDep = profile.mods.find(m => m.fullName.startsWith(dep));
          if (installedDep) {
            orphanDepsDetails.push({
              name: dep,
              icon: installedDep.iconUrl
            });
          }
        }
      }
    }

    // Get ALL installed dependencies (not just orphans)
    // Also excluding BepInExPack (already filtered from modDependencies)
    const allInstalledDeps = modDependencies.filter(dep =>
      profile.mods.some(m => m.fullName.startsWith(dep))
    );

    // If there are NO removable dependencies (or only BepInExPack which is filtered out),
    // skip the complex modal and show a simple confirmation dialog.
    if (allInstalledDeps.length === 0) {
      const confirmed = await window.ipcRenderer.confirm('Uninstall Mod', `Uninstall ${pkg.name}?`);
      if (confirmed) {
        setProgressState({
          isOpen: true,
          title: `Uninstalling ${pkg.name}`,
          progress: 0,
          currentTask: 'Removing mod...'
        });

        try {
          const installed = profile.mods.find(m => m.fullName.startsWith(pkg.full_name));
          if (installed) {
            await removeMod(profileIdToUse, installed.uuid4);
          }
          setProgressState(prev => ({ ...prev, progress: 100, currentTask: 'Done!' }));
          setTimeout(() => setProgressState(prev => ({ ...prev, isOpen: false })), 500);
        } catch (err: any) {
          console.error('Failed to uninstall:', err);
          setProgressState(prev => ({ ...prev, isOpen: false }));
          alert(`Failed to uninstall: ${err.message}`);
        }
      }
      return;
    }

    // Otherwise, open the uninstall modal
    setUninstallModalState({
      isOpen: true,
      pkg,
      orphanDeps: orphanDepsDetails,
      allInstalledDeps,
      profileId: profileIdToUse
    });
  };

  // Execute the actual uninstall with given deps to remove
  const executeUninstall = async (depsToRemove: string[]) => {
    const { pkg, profileId } = uninstallModalState;
    if (!pkg || !profileId) return;

    const profile = profiles.find(p => p.id === profileId);
    if (!profile) return;

    // Close modal
    setUninstallModalState(prev => ({ ...prev, isOpen: false }));

    // Show progress
    setProgressState({
      isOpen: true,
      title: `Uninstalling ${pkg.name}`,
      progress: 0,
      currentTask: 'Removing mod...'
    });

    try {
      // Remove the main mod
      const installed = profile.mods.find(m => m.fullName.startsWith(pkg.full_name));
      if (installed) {
        await removeMod(profileId, installed.uuid4);
      }

      setProgressState(prev => ({ ...prev, progress: 30 }));

      // Remove selected dependencies
      if (depsToRemove.length > 0) {
        const total = depsToRemove.length;
        for (let i = 0; i < depsToRemove.length; i++) {
          const depName = depsToRemove[i];
          const depMod = profile.mods.find(m => m.fullName.startsWith(depName));
          if (depMod) {
            setProgressState(prev => ({
              ...prev,
              progress: 30 + Math.round((i / total) * 60),
              currentTask: `Removing ${depName}... (${i + 1}/${total})`
            }));
            await removeMod(profileId, depMod.uuid4);
          }
        }
      }

      setProgressState(prev => ({ ...prev, progress: 100, currentTask: 'Done!' }));
      setTimeout(() => setProgressState(prev => ({ ...prev, isOpen: false })), 500);
    } catch (err: any) {
      console.error('Failed to uninstall:', err);
      setProgressState(prev => ({ ...prev, isOpen: false }));
      alert(`Failed to uninstall: ${err.message}`);
    }
  };

  const handleUpdateMod = async (pkg: Package, targetProfileId?: string) => {
    const profileIdToUse = targetProfileId || activeProfileId;
    if (!profileIdToUse) {
      alert('Please select a profile first');
      return;
    }

    setProgressState({
      isOpen: true,
      title: `Updating ${pkg.name}`,
      progress: 0,
      currentTask: 'Removing old version...'
    });

    try {
      // 1. Find and remove the old version
      const profile = profiles.find(p => p.id === profileIdToUse);
      const oldMod = profile?.mods.find(m => m.fullName.startsWith(pkg.full_name));

      if (oldMod) {
        setProgressState(prev => ({ ...prev, progress: 20, currentTask: 'Uninstalling old version...' }));
        await removeMod(profileIdToUse, oldMod.uuid4);
      }

      // 2. Install the new version
      const version = pkg.versions[0];
      setProgressState(prev => ({ ...prev, progress: 40, currentTask: 'Installing new version...' }));
      await installModWithDependencies(pkg, version, new Set(), profileIdToUse);

      setProgressState(prev => ({ ...prev, progress: 100, currentTask: 'Update complete!' }));
      setTimeout(() => setProgressState(prev => ({ ...prev, isOpen: false })), 500);
    } catch (err: any) {
      console.error('Failed to update mod:', err);
      setProgressState(prev => ({ ...prev, isOpen: false }));
      alert(`Failed to update mod: ${err.message}`);
    }
  };

  const handleImportProfile = async (code: string) => {
    if (!selectedCommunity) return;

    try {
      // Use the new import-profile handler which handles both Profile Codes and Package UUIDs
      const result = await window.ipcRenderer.importProfile(code.trim());
      await processImportResult(result);
    } catch (err: any) {
      console.error('Failed to import profile:', err);
      alert(`Import failed: ${err.message}. Please check the code.`);
    }
  };

  const handleImportFile = async (path: string) => {
    if (!selectedCommunity) return;
    try {
      const result = await window.ipcRenderer.importProfileFromFile(path);
      await processImportResult(result);
    } catch (err: any) {
      console.error('Failed to import file:', err);
      alert(`File import failed: ${err.message}`);
    }
  }

  const handleExportFile = async () => {
    if (!activeProfileId) return;
    try {
      const result = await window.ipcRenderer.exportProfile(activeProfileId);
      if (result.success) {
        alert(`Profile exported to: ${result.path}`);
      }
    } catch (e: any) {
      alert(`Export failed: ${e.message}`);
    }
  };

  const handleExportCode = async () => {
    if (!activeProfileId) return;

    setProgressState({
      isOpen: true,
      title: 'Generating Share Code',
      progress: 50,
      currentTask: 'Uploading profile...'
    });

    try {
      const code = await window.ipcRenderer.shareProfile(activeProfileId);

      setProgressState(prev => ({ ...prev, progress: 100, currentTask: 'Done!' }));
      setTimeout(() => {
        setProgressState(prev => ({ ...prev, isOpen: false }));

        // Copy to clipboard
        navigator.clipboard.writeText(code);
        alert(`Profile Code Generated: ${code}\n\nCopied to clipboard!`);
      }, 500);

    } catch (e: any) {
      console.error("Share failed:", e);
      setProgressState(prev => ({ ...prev, isOpen: false }));
      alert(`Failed to generate code: ${e}`);
    }
  };

  const processImportResult = async (result: any) => {
    if (result.type === 'profile') {
      // It's an r2modman profile export
      let profileName = result.name;

      // Check game path FIRST - required for direct installation
      const gamePath = await window.ipcRenderer.getGamePath(selectedCommunity || '');
      if (!gamePath) {
        await window.ipcRenderer.alert(
          "Game Path Required",
          "Please configure the game directory in Settings before importing profiles.\n\nGo to Settings → Game Directory → Set the path to your Wine/CrossOver game folder."
        );
        return;
      }

      // Remove "Imported: " prefix if present (compatibility with r2modman)
      if (profileName.startsWith("Imported: ")) {
        profileName = profileName.substring(10);
      }

      // Extract mod names from the import
      const modNames = result.mods.map((m: any) => m.name);

      // Batch lookup all packages from cache
      const lookup = await window.ipcRenderer.lookupPackagesByNames(
        selectedCommunity!,
        modNames
      );

      // If there are unknown mods, show a confirmation dialog
      if (lookup.unknown.length > 0) {
        const knownCount = lookup.found.length;
        const unknownCount = lookup.unknown.length;
        const unknownList = lookup.unknown.join('\n');

        const proceed = await window.ipcRenderer.confirm(
          'Some mods cannot be found',
          `${unknownCount} mod(s) from the profile were not found and will not be installed:\n\n${unknownList}\n\n${knownCount} mod(s) will be installed. Do you want to continue?`
        );

        if (!proceed) return;
      }

      const newProfileId = createProfile(profileName, selectedCommunity!);

      setProgressState({
        isOpen: true,
        title: 'Importing Profile',
        progress: 0,
        currentTask: 'Starting import...'
      });

      setTimeout(async () => {
        // Filter out unknown mods
        const modsToInstall = result.mods.filter((m: any) =>
          !lookup.unknown.includes(m.name)
        );

        let installedCount = 0;
        const totalMods = modsToInstall.length;
        const BATCH_SIZE = 5;
        const failedMods: string[] = [];

        // Process in batches
        for (let i = 0; i < totalMods; i += BATCH_SIZE) {
          const batch = modsToInstall.slice(i, i + BATCH_SIZE);

          await Promise.all(batch.map(async (mod: any, batchIdx: number) => {
            const globalIdx = i + batchIdx;

            // Update progress
            setProgressState(prev => ({
              ...prev,
              progress: Math.round(((globalIdx + 1) / totalMods) * 100),
              currentTask: `Installing ${mod.name} (${globalIdx + 1}/${totalMods})...`
            }));

            try {
              // Find package from lookup results
              const pkg = lookup.found.find((p: Package) => p.full_name === mod.name);

              if (pkg) {
                const version = pkg.versions.find(v => v.version_number === mod.version) || pkg.versions[0];

                const installResult = await window.ipcRenderer.installMod(
                  newProfileId,
                  version.download_url,
                  version.full_name,
                  gamePath,
                  legacyInstallMode
                );

                if (installResult.success) {
                  const installedMod: InstalledMod = {
                    uuid4: version.uuid4,
                    fullName: version.full_name,
                    versionNumber: version.version_number,
                    iconUrl: version.icon,
                    enabled: mod.enabled
                  };
                  addMod(newProfileId, installedMod);
                  installedCount++;
                } else {
                  throw new Error(installResult.error);
                }
              }
            } catch (e) {
              failedMods.push(mod.name);
              console.error(`Error installing ${mod.name}`, e);
            }
          }));
        }

        setProgressState(prev => ({ ...prev, progress: 100, currentTask: 'Import Complete!' }));
        setTimeout(() => {
          setProgressState(prev => ({ ...prev, isOpen: false }));
          let msg = `Imported profile "${result.name}" with ${installedCount}/${totalMods} mods.`;
          if (failedMods.length > 0) {
            msg += `\n\nFailed to install:\n${failedMods.join('\n')}`;
          }
          alert(msg);
        }, 500);

      }, 500);

    } else if (result.type === 'package') {
      // It's a single package (Modpack)
      const pkg = result.package;
      const profileName = pkg.name;
      const newProfileId = createProfile(profileName, selectedCommunity!);

      setTimeout(() => {
        handleInstallMod(pkg, newProfileId);
      }, 100);
    }
  }

  // VIEW LOGIC
  let content;

  if (!selectedCommunity) {
    // STEP 1: GAME SELECTION
    content = (
      <div className="flex flex-col h-full bg-gray-900 p-8 overflow-y-auto">
        <div className="max-w-4xl mx-auto w-full">
          <div className="text-center mb-12">
            <h1 className="text-4xl font-bold text-white mb-4">Welcome to r2modmac</h1>
            <p className="text-xl text-gray-400">Select a game to begin managing your mods</p>
          </div>

          <div className="flex gap-3 items-center w-full px-4 mb-8">
            <input
              className="flex-1 bg-gray-800 border border-gray-700 p-4 rounded-xl text-lg text-white placeholder-gray-500 focus:outline-none focus:border-blue-500 transition-all shadow-lg"
              placeholder="Search for a game..."
              value={gameSearchQuery}
              onChange={e => setGameSearchQuery(e.target.value)}
              autoFocus
            />
            <button
              onClick={() => setShowPreferences(true)}
              className="p-4 bg-gray-800 border border-gray-700 rounded-xl hover:bg-gray-700 hover:border-gray-600 transition-all text-gray-400 hover:text-white"
              title="Preferences"
            >
              <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
              </svg>
            </button>
          </div>

          {loading ? (
            <div className="text-center text-gray-400 py-12">
              <div className="flex justify-center">
                <svg className="animate-spin h-10 w-10 text-blue-500 mb-4" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
                  <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4"></circle>
                  <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                </svg>
              </div>
              <p>Loading games...</p>
            </div>
          ) : (
            <div>
              <GameSelector
                communities={filteredCommunities}
                selectedCommunity={selectedCommunity}
                onSelect={setSelectedCommunity}
                communityImages={communityImages}
                favoriteGames={favoriteGames}
                onToggleFavorite={toggleFavorite}
              />
            </div>
          )}
        </div>
      </div>
    );
  } else if (!activeProfileId) {
    // STEP 2: PROFILE SELECTION
    content = (
      <div className="flex flex-col h-full bg-gray-900 overflow-y-auto">
        <div className="p-4 border-b border-gray-800 flex items-center gap-4 sticky top-0 bg-gray-900 z-10">
          <button
            onClick={() => setSelectedCommunity(null)}
            className="text-gray-400 hover:text-white flex items-center gap-2 px-3 py-1.5 rounded-lg hover:bg-gray-800 transition-colors"
          >
            ← Change Game
          </button>
          <div className="h-6 w-px bg-gray-800" />
          <h2 className="text-xl font-bold text-white">
            {communities.find(c => c.identifier === selectedCommunity)?.name}
          </h2>
        </div>

        <ProfileList
          profiles={profiles}
          selectedGameIdentifier={selectedCommunity}
          onSelectProfile={handleSelectProfile}
          onCreateProfile={(name) => createProfile(name, selectedCommunity)}
          onImportProfile={handleImportProfile}
          onImportFile={handleImportFile}
          onDeleteProfile={deleteProfile}
          onUpdateProfile={updateProfile}
        />
      </div>
    );
  } else {
    // STEP 3: MOD MANAGEMENT
    const activeProfile = profiles.find(p => p.id === activeProfileId);
    const currentCommunity = communities.find(c => c.identifier === selectedCommunity);

    const sidebar = (
      <ProfileSidebar
        activeProfile={activeProfile}
        currentCommunity={currentCommunity || null}
        communityImage={currentCommunity ? communityImages[currentCommunity.identifier] : undefined}
        packages={packages}
        onSelectProfile={handleSelectProfile}
        onToggleMod={toggleMod}
        onViewModDetails={(pkg) => setSelectedMod(pkg)}
        onOpenModFolder={(profileId, modName) => window.ipcRenderer.openModFolder(profileId, modName, selectedCommunity || '')}
        onUninstallMod={async (mod) => {
          if (!activeProfile) return;
          const confirmed = await window.ipcRenderer.confirm('Uninstall Mod', `Uninstall ${mod.fullName}?`);
          if (confirmed) {
            await removeMod(activeProfile.id, mod.uuid4);
          }
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
        onInstallToGame={async () => {
          try {
            if (!activeProfile || !currentCommunity) return;

            // Get game path FIRST - needed for all installs
            const gamePath = await window.ipcRenderer.getGamePath(currentCommunity.identifier);
            if (!gamePath) {
              await window.ipcRenderer.alert('Game Path Required', 'Please set the game directory in Settings first.');
              return;
            }

            // --- BEPINEX AUTO-INSTALL LOGIC START ---
            // 1. Check if BepInEx is already installed
            const isBepInExInstalled = activeProfile.mods.some(m =>
              m.fullName.toLowerCase().includes("bepinexpack")
            );

            if (!isBepInExInstalled) {
              console.log("[Auto-Install] BepInEx not found, searching...");
              setProgressState({
                isOpen: true,
                title: 'Checking Requirements',
                progress: 0,
                currentTask: 'Searching for BepInExPack...'
              });

              // 2. Search for the correct BepInExPack for this community
              // Thunderstore usually names it "BepInExPack" or "{Game}_BepInExPack"
              // Best bet: Search "BepInExPack" and find the one that is NOT deprecated or is most popular
              const packages = await window.ipcRenderer.getPackages(
                currentCommunity.identifier,
                0,
                20,
                "BepInExPack", // Search query
                "downloads" // Sort by downloads to get the main one
              );

              console.log("[Auto-Install] Search results:", packages);

              // Filter for a good candidate (usually contains "BepInExPack" in name)
              const bepInExPkg = Array.isArray(packages) ? packages.find((p: Package) =>
                p.name.toLowerCase().includes("bepinexpack")
              ) : null;

              if (bepInExPkg) {
                const version = bepInExPkg.versions[0]; // Latest version
                console.log(`[Auto-Install] Found BepInExPack: ${bepInExPkg.name} v${version.version_number}`);

                setProgressState(prev => ({
                  ...prev,
                  progress: 20,
                  currentTask: `Installing missing requirement: ${bepInExPkg.name}...`
                }));

                await installModWithDependencies(bepInExPkg, version, new Set(), activeProfile.id, undefined, gamePath);

                console.log("[Auto-Install] BepInExPack installed successfully.");
              } else {
                console.warn("[Auto-Install] Could not find BepInExPack automatic candidate.");
                // Optional: warn user? For now proceed, maybe they have a custom setup.
              }

              setProgressState(prev => ({ ...prev, isOpen: false }));
            }
            // --- BEPINEX AUTO-INSTALL LOGIC END ---

            console.log('Syncing profile to game...');

            // Sync profile to game (removes mods not in profile, returns mods to install)
            // Pass legacyInstallMode to enable reverse sync (copy mods from game to cache)
            const syncResult = await window.ipcRenderer.syncProfileToGame(activeProfile.id, currentCommunity.identifier, legacyInstallMode);

            // Install any missing mods
            if (syncResult.to_install.length > 0) {
              setProgressState({
                isOpen: true,
                title: 'Syncing to Game',
                progress: 0,
                currentTask: `Installing ${syncResult.to_install.length} missing mods...`
              });

              let installed = 0;
              for (const modKey of syncResult.to_install) {
                // Find mod in profile
                const modInProfile = activeProfile.mods.find(m => {
                  const parts = m.fullName.split('-');
                  const key = parts.length >= 2 ? `${parts[0]}-${parts[1]}` : m.fullName;
                  return key.toLowerCase() === modKey.toLowerCase();
                });

                if (modInProfile) {
                  // LEGACY MODE: Try copying from cache first (INSTANT!)
                  if (legacyInstallMode) {
                    const cacheResult = await window.ipcRenderer.copyModFromCache(activeProfile.id, modInProfile.fullName, gamePath);
                    if (cacheResult.copied) {
                      // Success! Mod was copied from cache
                      installed++;
                      setProgressState(prev => ({
                        ...prev,
                        progress: Math.round((installed / syncResult.to_install.length) * 100),
                        currentTask: `Copied from cache ${installed}/${syncResult.to_install.length}: ${modKey}`
                      }));
                      continue; // Skip download
                    }
                  }

                  // Fallback: Download from Thunderstore
                  const pkg = await window.ipcRenderer.fetchPackageByName(modInProfile.fullName, currentCommunity.identifier);
                  if (pkg) {
                    const version = pkg.versions.find((v: any) => v.version_number === modInProfile.versionNumber) || pkg.versions[0];
                    await window.ipcRenderer.installMod(activeProfile.id, version.download_url, version.full_name, gamePath, legacyInstallMode);
                  }
                }
                installed++;
                setProgressState(prev => ({
                  ...prev,
                  progress: Math.round((installed / syncResult.to_install.length) * 100),
                  currentTask: `Installed ${installed}/${syncResult.to_install.length}: ${modKey}`
                }));
              }
              setProgressState(prev => ({ ...prev, isOpen: false }));
            }

            // Build smart success message - only show counts that are > 0
            const removed = syncResult.removed;
            const installed = syncResult.to_install.length;
            const cached = syncResult.cached || 0;

            let message = '';
            if (removed === 0 && installed === 0 && cached === 0) {
              message = 'Profile already synced! No changes needed.';
            } else {
              const parts: string[] = [];
              if (removed > 0) parts.push(`${removed} removed`);
              if (installed > 0) parts.push(`${installed} installed`);
              if (cached > 0) parts.push(`${cached} cached`);
              message = `Sync complete! ${parts.join(', ')}.`;
            }

            await window.ipcRenderer.alert('Success', message);
            setShowCrossOverGuide(true);
          } catch (e: any) {
            console.error("Sync to game failed:", e);
            setProgressState(prev => ({ ...prev, isOpen: false }));
            alert('Error syncing: ' + e);
          }
        }}
        onExportProfile={() => setShowExportModal(true)}
        onOpenSettings={() => setShowSettings(true)}
      />
    );

    const main = (
      <div className="flex-1 flex flex-col min-w-0 bg-gray-900 h-full">
        <div className="p-5 border-b border-gray-800 flex items-center justify-between gap-4 flex-shrink-0">
          <h1 className="text-2xl font-bold text-white">Browse Mods</h1>
          <div className="flex items-center gap-3">
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
            onInstall={handleInstallMod}
            onUninstall={handleUninstallWithDependencies}
            onModClick={setSelectedMod}
            onLoadMore={handleLoadMore}
            isLoadingMore={loadingMods}
            hasMore={hasMore}
          />
        </div>
      </div>
    );

    content = <Layout sidebar={sidebar} main={main} />;
  }

  // WRAPPER
  return (
    <div className="h-screen w-screen flex flex-col bg-gray-900 overflow-hidden">
      {/* Scrollable Content Area */}
      <div className="flex-1 overflow-hidden relative">
        {content}
      </div>

      {/* Modals */}
      {selectedMod && (
        <ModDetailModal
          mod={selectedMod.versions[0]}
          isOpen={!!selectedMod}
          gameId={selectedCommunity || ''}
          installedMods={activeProfileId ? profiles.find(p => p.id === activeProfileId)?.mods || [] : []}
          onClose={() => setSelectedMod(null)}
          onInstall={() => {
            if (activeProfileId) {
              handleInstallMod(selectedMod, activeProfileId);
            }
          }}
          onUpdate={() => {
            if (activeProfileId) {
              handleUpdateMod(selectedMod, activeProfileId);
            }
          }}
          onUninstall={async () => {
            if (!activeProfileId || !selectedMod) return;
            await handleUninstallWithDependencies(selectedMod, activeProfileId);
            setSelectedMod(null); // Close modal after uninstall
          }}
          isInstalled={
            activeProfileId
              ? profiles.find(p => p.id === activeProfileId)?.mods.some(m => m.fullName.startsWith(selectedMod.full_name)) ?? false
              : false
          }
          hasUpdate={
            activeProfileId
              ? (() => {
                const profile = profiles.find(p => p.id === activeProfileId);
                const installed = profile?.mods.find(m => m.fullName.startsWith(selectedMod.full_name));
                const latestVersion = selectedMod.versions[0].version_number;
                return installed ? latestVersion !== installed.versionNumber : false;
              })()
              : false
          }
        />
      )}

      <ProgressModal
        isOpen={progressState.isOpen}
        title={progressState.title}
        progress={progressState.progress}
        currentTask={progressState.currentTask}
      />

      <UninstallModal
        isOpen={uninstallModalState.isOpen}
        modName={uninstallModalState.pkg?.name || ''}
        modIcon={uninstallModalState.pkg?.versions[0]?.icon}
        orphanDeps={uninstallModalState.orphanDeps}
        allDepsCount={uninstallModalState.allInstalledDeps.length}
        onCancel={() => setUninstallModalState(prev => ({ ...prev, isOpen: false }))}
        onModOnly={() => executeUninstall([])}
        onWithOrphans={() => executeUninstall(uninstallModalState.orphanDeps.map(d => d.name))}
        onWithAllDeps={() => executeUninstall(uninstallModalState.allInstalledDeps)}
      />

      <SettingsModal
        isOpen={showSettings}
        onClose={() => setShowSettings(false)}
        selectedGame={selectedCommunity || undefined}
      />

      {showExportModal && activeProfileId && (
        <ExportModal
          isOpen={showExportModal}
          onClose={() => setShowExportModal(false)}
          onExportCode={handleExportCode}
          onExportFile={handleExportFile}
        />
      )}

      {showUpdateModal && updateInfo && (
        <UpdateModal
          updateInfo={updateInfo}
          onClose={() => setShowUpdateModal(false)}
          onUpdate={async () => {
            if (updateInfo.download_url) {
              setProgressState({
                isOpen: true,
                title: 'Updating r2modmac',
                progress: 0,
                currentTask: 'Downloading update...'
              });

              // Listen for progress
              const unlisten = await listen<number>('update-progress', (event) => {
                setProgressState(prev => ({
                  ...prev,
                  progress: event.payload
                }));
              });

              try {
                await window.ipcRenderer.installUpdate(updateInfo.download_url);
                unlisten(); // Clean up listener when done (or before closing)
                // The script waits for PID exit.
                window.close();
              } catch (e) {
                alert("Update failed: " + e);
                setProgressState(prev => ({ ...prev, isOpen: false }));
              }
            }
          }}
        />
      )}

      {showCrossOverGuide && !hideCrossOverGuide && (
        <CrossOverGuideModal
          isOpen={showCrossOverGuide}
          onClose={() => setShowCrossOverGuide(false)}
          onDontShowAgain={() => {
            setHideCrossOverGuide(true);
            setShowCrossOverGuide(false);
          }}
        />
      )}

      <PreferencesModal
        isOpen={showPreferences}
        onClose={() => setShowPreferences(false)}
        settings={{ legacy_install_mode: legacyInstallMode }}
        onSave={async (newSettings) => {
          setLegacyInstallMode(newSettings.legacy_install_mode);
          // Save to backend
          const currentSettings = await window.ipcRenderer.getSettings();
          await window.ipcRenderer.saveSettings({
            ...currentSettings,
            legacy_install_mode: newSettings.legacy_install_mode
          });
        }}
      />
    </div>
  )
}

export default App
