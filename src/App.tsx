import { useState, useEffect } from 'react'
import { Layout } from './components/Layout'
import { GameSelector } from './components/GameList'
import { SearchBar } from './components/SearchBar'
import { VirtualizedModGrid } from './components/VirtualizedModGrid'
import { ModDetailModal } from './components/ModDetailModal'
import { ProfileList } from './components/ProfileList'
import { ProgressModal } from './components/ProgressModal'
import { fetchPackages } from './api/thunderstore'
import { useProfileStore } from './store/useProfileStore'
import type { Community, Package, PackageVersion } from './types/thunderstore'
import type { InstalledMod } from './types/profile'

function App() {
  const [communities, setCommunities] = useState<Community[]>([])
  const [communityImages, setCommunityImages] = useState<Record<string, string>>({})
  const [packages, setPackages] = useState<Package[]>([])
  const [loading, setLoading] = useState(true)
  const [loadingMods, setLoadingMods] = useState(false)
  const [searchQuery, setSearchQuery] = useState('')
  const [selectedMod, setSelectedMod] = useState<Package | null>(null)
  const [gameSearchQuery, setGameSearchQuery] = useState('')
  const [progressState, setProgressState] = useState({
    isOpen: false,
    title: '',
    progress: 0,
    currentTask: ''
  })

  const {
    profiles,
    createProfile,
    loadProfiles,
    activeProfileId,
    selectProfile,
    deleteProfile,
    addMod,
    removeMod,
    toggleMod
  } = useProfileStore()

  const [selectedCommunity, setSelectedCommunity] = useState<string | null>(null)

  useEffect(() => {
    loadData()
    loadProfiles()
  }, [])

  useEffect(() => {
    if (selectedCommunity) {
      loadPackages(selectedCommunity)
      // Reset profile selection when changing game
      if (activeProfileId) {
        selectProfile('') // or null if store supports it, currently string
      }
    }
  }, [selectedCommunity])

  const loadData = async () => {
    setLoading(true)
    try {
      const [data, images] = await Promise.all([
        window.ipcRenderer.fetchCommunities(),
        window.ipcRenderer.fetchCommunityImages()
      ])
      setCommunities(data)
      setCommunityImages(images)
      console.log(`Loaded ${data.length} communities and ${Object.keys(images).length} images`)
    } catch (err) {
      console.error('Failed to load data', err)
    }
    setLoading(false)
  }

  const loadPackages = async (communityId: string) => {
    setLoadingMods(true)
    try {
      const data = await fetchPackages(communityId)
      setPackages(data)
    } catch (err) {
      console.error('Failed to load packages', err)
    } finally {
      setLoadingMods(false)
    }
  }

  const installModWithDependencies = async (pkg: Package, version: PackageVersion, installedCache: Set<string> = new Set(), targetProfileId?: string) => {
    if (installedCache.has(version.full_name)) return;
    installedCache.add(version.full_name);

    const profileIdToUse = targetProfileId || activeProfileId;
    if (!profileIdToUse) throw new Error("No profile selected");

    // 1. Install dependencies first
    for (const depString of version.dependencies) {
      // depString format: "TeamName-ModName-Version"
      const [team, mod, ver] = depString.split('-');
      const depFullName = `${team}-${mod}`;

      // Check if already installed in profile (skip if so)
      const activeProfile = profiles.find(p => p.id === profileIdToUse);
      if (activeProfile?.mods.some(m => m.fullName.startsWith(depFullName))) {
        continue;
      }

      // Find package in current list
      const depPkg = packages.find(p => p.full_name === depFullName);
      if (depPkg) {
        // Find matching version or latest
        const depVersion = depPkg.versions.find(v => v.version_number === ver) || depPkg.versions[0];
        await installModWithDependencies(depPkg, depVersion, installedCache, profileIdToUse);
      } else {
        console.warn(`Dependency ${depString} not found in current package list`);
      }
    }

    // 2. Install the mod itself
    try {
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

  const processImportResult = async (result: any) => {
    if (result.type === 'profile') {
      // It's an r2modman profile export
      const profileName = result.name;
      const newProfileId = createProfile(profileName, selectedCommunity!);

      setProgressState({
        isOpen: true,
        title: 'Importing Profile',
        progress: 0,
        currentTask: 'Creating profile...'
      });

      // Wait for profile creation
      setTimeout(async () => {
        // Install all mods from the profile
        let installedCount = 0;
        const totalMods = result.mods.length;

        for (let i = 0; i < totalMods; i++) {
          const mod = result.mods[i];
          const progress = Math.round(((i + 1) / totalMods) * 100);

          setProgressState(prev => ({
            ...prev,
            progress,
            currentTask: `Installing ${mod.name} (${i + 1}/${totalMods})...`
          }));

          // mod.name is "Team-Name"
          // Try to find in local list first
          let pkg: Package | undefined | null = packages.find(p => p.full_name === mod.name);

          // If not found, fetch from API
          if (!pkg) {
            try {
              console.log(`Fetching missing package info for ${mod.name}...`);
              pkg = await window.ipcRenderer.fetchPackageByName(mod.name);
            } catch (e) {
              console.error(`Failed to fetch info for ${mod.name}`, e);
            }
          }

          if (pkg) {
            const version = pkg.versions.find(v => v.version_number === mod.version) || pkg.versions[0];
            try {
              // Install DIRECTLY without recursive dependency check
              // The profile export already contains all dependencies
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
                console.error(`Failed to install ${mod.name}: ${installResult.error}`);
              }
            } catch (e) {
              console.error(`Failed to install ${mod.name}`, e);
            }
          } else {
            console.warn(`Mod ${mod.name} could not be found or fetched.`);
          }
        }

        setProgressState(prev => ({ ...prev, progress: 100, currentTask: 'Import Complete!' }));
        setTimeout(() => {
          setProgressState(prev => ({ ...prev, isOpen: false }));
          alert(`Imported profile "${result.name}" with ${installedCount} mods.`);
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

  const filteredPackages = packages.filter(pkg =>
    pkg.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
    pkg.full_name.toLowerCase().includes(searchQuery.toLowerCase()) ||
    pkg.owner.toLowerCase().includes(searchQuery.toLowerCase())
  )

  const filteredCommunities = communities.filter(c =>
    c.name.toLowerCase().includes(gameSearchQuery.toLowerCase()) ||
    c.identifier.toLowerCase().includes(gameSearchQuery.toLowerCase())
  )

  // VIEW LOGIC
  let content;

  if (!selectedCommunity) {
    // STEP 1: GAME SELECTION
    content = (
      <div className="flex flex-col h-full bg-gray-900 p-8">
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
              <div className="animate-spin text-4xl mb-4">⚙️</div>
              <p>Loading games...</p>
            </div>
          ) : (
            <div>
              <GameSelector
                communities={filteredCommunities}
                selectedCommunity={selectedCommunity}
                onSelect={setSelectedCommunity}
                communityImages={communityImages}
              />
            </div>
          )}
        </div>
      </div>
    );
  } else if (!activeProfileId) {
    // STEP 2: PROFILE SELECTION
    content = (
      <div className="flex flex-col h-full bg-gray-900">
        <div className="p-4 border-b border-gray-800 flex items-center gap-4">
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
        />
      </div>
    );
  } else {
    // STEP 3: MOD MANAGEMENT
    const activeProfile = profiles.find(p => p.id === activeProfileId);

    const sidebar = (
      <div className="h-full flex flex-col bg-gray-900 border-r border-gray-800 w-80">
        <div className="p-4 border-b border-gray-800">
          <button
            onClick={() => selectProfile('')} // Go back to profile list
            className="text-gray-400 hover:text-white flex items-center gap-2 text-sm mb-4"
          >
            ← Change Profile
          </button>

          <div className="flex items-center gap-3 mb-4">
            <div className="w-12 h-12 bg-gradient-to-br from-blue-500 to-purple-600 rounded-xl shadow-lg flex items-center justify-center text-xl font-bold text-white">
              {activeProfile?.name.charAt(0).toUpperCase()}
            </div>
            <div className="min-w-0 flex-1">
              <h2 className="font-bold text-white truncate text-lg">{activeProfile?.name}</h2>
              <p className="text-xs text-gray-500 truncate">{activeProfile?.mods.length} mods installed</p>
            </div>
          </div>

          <div className="flex gap-2">
            <button
              onClick={async () => {
                if (!activeProfileId) return;
                try {
                  const result = await window.ipcRenderer.exportProfile(activeProfileId);
                  if (result.success) {
                    alert(`Profile exported to: ${result.path}`);
                  }
                } catch (e: any) {
                  alert(`Export failed: ${e.message}`);
                }
              }}
              className="flex-1 bg-gray-800 hover:bg-gray-700 text-white text-xs py-2 px-3 rounded-lg transition-colors flex items-center justify-center gap-2 border border-gray-700"
            >
              <span>📤</span> Export Profile
            </button>
            <button
              onClick={() => {
                // Copy profile code/UUID to clipboard
                // Since we don't have a backend to generate codes yet, we can only share the UUID if it was imported via UUID?
                // Or we can just explain that sharing via code requires uploading to Thunderstore/r2modman backend which we don't support yet.
                // BUT, the user asked to share the UUID.
                // If the profile was imported, it might have a code. But locally created profiles don't have a Thunderstore code.
                // Let's just copy the Profile Name for now or show a message.

                // Wait, "dobbiamo fare sharing della uuid".
                // If the user means sharing the *local* profile UUID, that's useless for others.
                // They probably mean "Export as Code".
                // Since we can't generate codes (requires API key/backend), we can only support Export to File.

                // However, if they want to share the "Modpack UUID" (e.g. they are playing a modpack), we can try to find the modpack ID.
                // But for custom profiles, "Export to File" is the way.

                alert("To share this profile, please use the 'Export Profile' button to create a file you can send to your friends.\n\nGenerating share codes requires uploading to Thunderstore, which is not yet supported.");
              }}
              className="bg-gray-800 hover:bg-gray-700 text-white text-xs py-2 px-3 rounded-lg transition-colors border border-gray-700"
              title="Share Code"
            >
              🔗
            </button>
          </div>
        </div>

        <div className="flex-1 overflow-y-auto p-2 space-y-1">
          <div className="px-2 py-2 text-xs font-bold text-gray-500 uppercase tracking-wider flex justify-between items-center">
            <span>Installed Mods</span>
            <span className="bg-gray-800 text-gray-400 px-2 py-0.5 rounded-full text-[10px]">{activeProfile?.mods.length}</span>
          </div>

          {activeProfile?.mods.map(mod => {
            // Strip version from mod.fullName for comparison with packages
            const modNameWithoutVersion = mod.fullName.replace(/-\d+\.\d+\.\d+$/, '');

            // Check for updates
            const pkg = packages.find(p => p.full_name === modNameWithoutVersion);
            const latestVersion = pkg?.versions[0].version_number;
            const hasUpdate = latestVersion && latestVersion !== mod.versionNumber;

            return (
              <div
                key={mod.uuid4}
                className={`flex items-center gap-3 p-2 rounded-lg hover:bg-gray-800 group cursor-pointer transition-all border border-transparent hover:border-gray-700 relative pr-16 overflow-hidden ${!mod.enabled ? 'opacity-50' : ''}`}
                onClick={() => {
                  toggleMod(activeProfile.id, mod.uuid4);
                }}
              >
                {/* Caution Tape Overlay for disabled mods */}
                {!mod.enabled && (
                  <div
                    className="absolute inset-0 pointer-events-none z-10 opacity-30"
                    style={{
                      background: 'repeating-linear-gradient(45deg, #000 0px, #000 20px, #fbbf24 20px, #fbbf24 40px)',
                      mixBlendMode: 'multiply'
                    }}
                  />
                )}

                {/* Mod Icon */}
                <div className="w-10 h-10 bg-gray-800 rounded-lg overflow-hidden flex-shrink-0 border border-gray-700 relative">
                  {mod.iconUrl ? (
                    <img src={mod.iconUrl} alt="" className="w-full h-full object-cover" />
                  ) : (
                    <div className="w-full h-full flex items-center justify-center text-gray-600 text-xs">?</div>
                  )}
                  {!mod.enabled && (
                    <div className="absolute inset-0 flex items-center justify-center bg-black/50">
                      <span className="text-xs font-bold text-white">OFF</span>
                    </div>
                  )}
                </div>

                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <div className={`text-sm font-medium truncate transition-colors ${mod.enabled ? 'text-gray-200 group-hover:text-white' : 'text-gray-500 line-through'}`}>
                      {mod.fullName.split('-')[1] || mod.fullName}
                    </div>
                    {hasUpdate && mod.enabled && (
                      <div className="text-amber-400 text-[10px] bg-amber-400/10 px-1.5 py-0.5 rounded border border-amber-400/20" title={`Update available: ${latestVersion}`}>
                        UPDATE
                      </div>
                    )}
                  </div>
                  <div className="text-xs text-gray-500 flex items-center gap-2">
                    <span>v{mod.versionNumber}</span>
                  </div>
                </div>

                <div className="absolute right-2 top-1/2 -translate-y-1/2 flex items-center opacity-0 group-hover:opacity-100 transition-opacity gap-1 bg-gray-800/90 rounded-lg p-1 shadow-sm backdrop-blur-sm">
                  <button
                    onClick={async (e) => {
                      e.stopPropagation();
                      if (pkg) {
                        setSelectedMod(pkg);
                      } else {
                        // Try to fetch package info
                        try {
                          const fetchedPkg = await window.ipcRenderer.fetchPackageByName(mod.fullName);
                          if (fetchedPkg) {
                            setSelectedMod(fetchedPkg);
                          } else {
                            alert("Could not fetch details for this mod.");
                          }
                        } catch (err) {
                          console.error("Failed to fetch mod details", err);
                          alert("Failed to fetch mod details.");
                        }
                      }
                    }}
                    className="p-1.5 text-gray-400 hover:text-blue-400 hover:bg-blue-400/10 rounded-md transition-colors"
                    title="View Details"
                  >
                    ℹ️
                  </button>
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      window.ipcRenderer.openModFolder(activeProfile.id, mod.fullName.split('-')[1]);
                    }}
                    className="p-1.5 text-gray-400 hover:text-blue-400 hover:bg-blue-400/10 rounded-md transition-colors"
                    title="Locate in Finder"
                  >
                    📂
                  </button>
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      if (confirm(`Uninstall ${mod.fullName}?`)) {
                        removeMod(activeProfile.id, mod.uuid4);
                      }
                    }}
                    className="p-1.5 text-gray-400 hover:text-red-400 hover:bg-red-400/10 rounded-md transition-colors"
                    title="Uninstall"
                  >
                    ✕
                  </button>
                </div>
              </div>
            );
          })}

          {activeProfile?.mods.length === 0 && (
            <div className="text-center py-12 px-4">
              <div className="text-4xl mb-3 opacity-20">📦</div>
              <p className="text-gray-500 text-sm font-medium">No mods installed</p>
              <p className="text-gray-600 text-xs mt-1">Search for mods to get started</p>
            </div>
          )}
        </div>
      </div>
    );

    const main = (
      <div className="flex flex-col h-full bg-gray-900">
        <div className="p-6 border-b border-gray-800 flex items-center justify-between gap-4">
          <h1 className="text-2xl font-bold text-white">Browse Mods</h1>
          <div className="w-96">
            <SearchBar value={searchQuery} onChange={setSearchQuery} />
          </div>
        </div>

        <div className="flex-1 overflow-hidden relative">
          {loadingMods ? (
            <div className="absolute inset-0 flex items-center justify-center">
              <div className="text-center">
                <div className="animate-spin text-4xl mb-4 text-blue-500">⚙️</div>
                <p className="text-gray-400">Fetching packages...</p>
              </div>
            </div>
          ) : (
            <VirtualizedModGrid
              packages={filteredPackages}
              onInstall={handleInstallMod}
              onModClick={setSelectedMod}
            />
          )}
        </div>
      </div>
    );

    content = <Layout sidebar={sidebar} main={main} />;
  }

  return (
    <>
      {content}
      {selectedMod && (
        <ModDetailModal
          mod={selectedMod.versions[0]}
          isOpen={!!selectedMod}
          onClose={() => setSelectedMod(null)}
          onInstall={() => handleInstallMod(selectedMod)}
          isInstalled={activeProfileId ? profiles.find(p => p.id === activeProfileId)?.mods.some(m => m.uuid4 === selectedMod.versions[0].uuid4) ?? false : false}
        />
      )}
      <ProgressModal
        isOpen={progressState.isOpen}
        title={progressState.title}
        progress={progressState.progress}
        currentTask={progressState.currentTask}
      />
    </>
  )
}

export default App
