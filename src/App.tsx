import { useState, useEffect } from 'react'
import { Layout } from './components/Layout'
import { GameSelector } from './components/GameList'
import { SearchBar } from './components/SearchBar'
import { VirtualizedModGrid } from './components/VirtualizedModGrid'
import { ModDetailModal } from './components/ModDetailModal'
import { ProfileList } from './components/ProfileList'
import { ProgressModal } from './components/ProgressModal'
import { SettingsModal } from './components/SettingsModal'
import { ExportModal } from './components/ExportModal'
import { CrossOverGuideModal } from './components/CrossOverGuideModal';
import { ProfileSidebar } from './components/ProfileSidebar';
import { useProfileStore } from './store/useProfileStore'
import type { Community, Package, PackageVersion } from './types/thunderstore'
import type { InstalledMod } from './types/profile'
import { getVersion } from '@tauri-apps/api/app';
import { UpdateModal } from './components/UpdateModal';
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
  const [sortOrder, setSortOrder] = useState('downloads') // Default sort
  const PAGE_SIZE = 50

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
  const [showUpdateModal, setShowUpdateModal] = useState(false)

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
      // Initial load for game
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

  // Sort Effect
  useEffect(() => {
    if (selectedCommunity) {
      loadPackages(selectedCommunity, 0, true)
    }
  }, [sortOrder])

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
    if (reset) {
      setLoadingMods(true)
      setPage(0)
      setPackages([])
      setHasMore(true)
    }

    try {
      // First fetch ensures cache is populated (returns count)
      if (pageNum === 0 && reset) {
        await window.ipcRenderer.fetchPackages(communityId)
      }

      const newPackages = await window.ipcRenderer.getPackages(communityId, pageNum, PAGE_SIZE, searchQuery, sortOrder)

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

  const installModWithDependencies = async (pkg: Package, version: PackageVersion, installedCache: Set<string> = new Set(), targetProfileId?: string) => {
    if (installedCache.has(version.full_name)) return;
    installedCache.add(version.full_name);

    const profileIdToUse = targetProfileId || activeProfileId;
    if (!profileIdToUse) throw new Error("No profile selected");

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

        // Install each found dependency recursively
        for (const depPkg of result.found) {
          const depVersion = depPkg.versions[0];
          if (depVersion) {
            setProgressState(prev => ({
              ...prev,
              currentTask: `Installing dependency: ${depPkg.name}...`
            }));
            await installModWithDependencies(depPkg, depVersion, installedCache, profileIdToUse);
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

    // 3. Install the mod itself
    try {
      setProgressState(prev => ({
        ...prev,
        currentTask: `Installing ${pkg.name}...`
      }));

      const result = await window.ipcRenderer.installMod(
        profileIdToUse,
        version.download_url,
        version.full_name
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

  const handleInstallMod = async (pkg: Package, targetProfileId?: string) => {
    const profileIdToUse = targetProfileId || activeProfileId;
    if (!profileIdToUse) {
      alert('Please select a profile first');
      return;
    }

    const version = pkg.versions[0];

    setProgressState({
      isOpen: true,
      title: `Installing ${pkg.name}`,
      progress: 0,
      currentTask: 'Starting installation...'
    });

    try {
      setProgressState(prev => ({ ...prev, progress: 10, currentTask: 'Checking dependencies...' }));
      await installModWithDependencies(pkg, version, new Set(), profileIdToUse);
      setProgressState(prev => ({ ...prev, progress: 100, currentTask: 'Done!' }));
      setTimeout(() => setProgressState(prev => ({ ...prev, isOpen: false })), 500);
      // alert(`Successfully installed ${pkg.name} and dependencies`); // Removed alert
    } catch (err: any) {
      console.error('Failed to install mod:', err);
      setProgressState(prev => ({ ...prev, isOpen: false }));
      alert(`Failed to install mod: ${err.message}`);
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
                  version.full_name
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

          <div className="mb-8">
            <input
              className="w-full bg-gray-800 border border-gray-700 p-4 rounded-xl text-lg text-white placeholder-gray-500 focus:outline-none focus:border-blue-500 transition-all shadow-lg"
              placeholder="Search for a game..."
              value={gameSearchQuery}
              onChange={e => setGameSearchQuery(e.target.value)}
              autoFocus
            />
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
          onSelectProfile={selectProfile}
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
        onSelectProfile={selectProfile}
        onToggleMod={toggleMod}
        onViewModDetails={(pkg) => setSelectedMod(pkg)}
        onOpenModFolder={(profileId, modName) => window.ipcRenderer.openModFolder(profileId, modName)}
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

                await installModWithDependencies(bepInExPkg, version, new Set(), activeProfile.id);

                console.log("[Auto-Install] BepInExPack installed successfully.");
              } else {
                console.warn("[Auto-Install] Could not find BepInExPack automatic candidate.");
                // Optional: warn user? For now proceed, maybe they have a custom setup.
              }

              setProgressState(prev => ({ ...prev, isOpen: false }));
            }
            // --- BEPINEX AUTO-INSTALL LOGIC END ---

            // Get list of disabled mod names for filtering
            const disabledMods = activeProfile.mods
              .filter(m => !m.enabled)
              .map(m => {
                // Extract mod name from fullName (format: "Author-ModName-Version")
                const parts = m.fullName.split('-');
                return parts.length >= 2 ? parts[1].toLowerCase() : m.fullName.toLowerCase();
              });

            console.log('Installing with disabled mods:', disabledMods);
            await window.ipcRenderer.installToGame(currentCommunity.identifier, activeProfile.id, disabledMods);
            await window.ipcRenderer.alert('Success', 'Mods applied! Launch the game via Steam to play.');
            setShowCrossOverGuide(true);
          } catch (e: any) {
            console.error("Install to game failed:", e);
            setProgressState(prev => ({ ...prev, isOpen: false }));
            alert('Error installing modpack: ' + e);
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
            <select
              value={sortOrder}
              onChange={(e) => setSortOrder(e.target.value)}
              className="bg-gray-800 text-white border border-gray-700 rounded-lg px-3 py-2 text-sm focus:outline-none focus:border-blue-500 h-[42px]"
            >
              <option value="downloads">Most Downloaded</option>
              <option value="rating">Top Rated</option>
              <option value="updated">Last Updated</option>
              <option value="alphabetical">Alphabetical</option>
            </select>
            <div className="w-80">
              <SearchBar value={searchQuery} onChange={setSearchQuery} />
            </div>
          </div>
        </div>

        <div className="flex-1 overflow-hidden relative flex flex-col">
          {loadingMods ? (
            <div className="absolute inset-0 flex items-center justify-center">
              <div className="text-center flex flex-col items-center">
                <svg className="animate-spin h-10 w-10 text-blue-500 mb-4" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
                  <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4"></circle>
                  <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                </svg>
                <p className="text-gray-400">Fetching packages...</p>
              </div>
            </div>
          ) : (
            <VirtualizedModGrid
              packages={packages}
              installedMods={activeProfile?.mods || []}
              onInstall={handleInstallMod}
              onUninstall={async (pkg) => {
                if (!activeProfileId) return;
                const confirmed = await window.ipcRenderer.confirm('Uninstall Mod', `Uninstall ${pkg.name}?`);
                if (confirmed) {
                  // Find the installed mod UUID
                  const installed = activeProfile?.mods.find(m => m.fullName.startsWith(pkg.full_name));
                  if (installed) {
                    await removeMod(activeProfileId, installed.uuid4);
                  }
                }
              }}
              onModClick={setSelectedMod}
              onLoadMore={handleLoadMore}
            />
          )}
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
            if (!activeProfileId) return;
            const confirmed = await window.ipcRenderer.confirm('Uninstall Mod', `Uninstall ${selectedMod.name}?`);
            if (confirmed) {
              const profile = profiles.find(p => p.id === activeProfileId);
              const installed = profile?.mods.find(m => m.fullName.startsWith(selectedMod.full_name));
              if (installed) {
                await removeMod(activeProfileId, installed.uuid4);
                setSelectedMod(null); // Close modal after uninstall
              }
            }
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

              try {
                await window.ipcRenderer.installUpdate(updateInfo.download_url);
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
    </div>
  )
}

export default App
