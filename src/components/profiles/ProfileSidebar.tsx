import React, { useMemo, useState } from 'react';
import type { Community, Package } from '../../types/thunderstore';
import type { Profile, InstalledMod } from '../../types/profile';
import { Button } from '../ui';

const MAX_PARALLEL_TOGGLES = 10;

interface ProfileSidebarProps {
    activeProfile: Profile | undefined;
    currentCommunity: Community | null;
    communityImage: string | undefined;
    packages: Package[];
    legacyInstallMode: boolean;
    installInParallel: boolean;
    onSelectProfile: (profileId: string) => void;
    onToggleMod: (profileId: string, modUuid: string) => Promise<void> | void;
    onViewModDetails: (pkg: Package) => void;
    onOpenModFolder: (profileId: string, modName: string) => void;
    onUninstallMod: (mod: InstalledMod) => Promise<void> | void;
    onInstallToGame: (isVanillaOverride?: boolean) => void;
    onResolvePackage: (mod: InstalledMod) => Promise<Package | null>;
    onExportProfile: () => void;
    onOpenSettings: () => void;
    onUpdateProfile: (profileId: string, updates: Partial<Profile>) => void;
    onToggleVanilla: (profileId: string, newVanillaState: boolean) => Promise<void> | void;
}

export const ProfileSidebar: React.FC<ProfileSidebarProps> = ({
    activeProfile,
    currentCommunity,
    communityImage,
    packages,
    legacyInstallMode,
    installInParallel,
    onSelectProfile,
    onToggleMod,
    onViewModDetails,
    onOpenModFolder,
    onUninstallMod,
    onInstallToGame,
    onResolvePackage,
    onExportProfile,
    onOpenSettings,
    onUpdateProfile,
    onToggleVanilla,
}) => {
    const [searchQuery, setSearchQuery] = useState('');
    const [isEditing, setIsEditing] = useState(false);
    const [editName, setEditName] = useState('');
    const [selectedModIds, setSelectedModIds] = useState<string[]>([]);
    const [selectionAnchorId, setSelectionAnchorId] = useState<string | null>(null);

    const handleEditClick = () => {
        if (activeProfile) {
            setEditName(activeProfile.name);
            setIsEditing(true);
        }
    };

    const handleUpdate = (e: React.FormEvent) => {
        e.preventDefault();
        if (activeProfile && editName.trim()) {
            onUpdateProfile(activeProfile.id, { name: editName.trim() });
            setIsEditing(false);
        }
    };

    const handleImageSelect = async () => {
        if (!activeProfile) return;
        try {
            const filePath = await window.ipcRenderer.selectFile([
                { name: 'Images', extensions: ['png', 'jpg', 'jpeg', 'webp'] }
            ]);
            if (filePath) {
                const base64 = await window.ipcRenderer.readImage(filePath);
                if (base64) {
                    onUpdateProfile(activeProfile.id, { profileImageUrl: base64 });
                    // No need to update local state as prop will update
                }
            }
        } catch (e) {
            console.error("Failed to select image:", e);
        }
    };

    const handleRemoveImage = () => {
        if (activeProfile) {
            onUpdateProfile(activeProfile.id, { profileImageUrl: undefined });
        }
    };

    const displayedMods = activeProfile?.mods.filter(mod =>
        mod.fullName.toLowerCase().includes(searchQuery.toLowerCase())
    ) || [];
    const visibleModIds = useMemo(() => new Set(displayedMods.map((m) => m.uuid4)), [displayedMods]);
    const effectiveSelectedModIds = useMemo(
        () => selectedModIds.filter((id) => visibleModIds.has(id)),
        [selectedModIds, visibleModIds]
    );
    const selectedModIdSet = useMemo(() => new Set(effectiveSelectedModIds), [effectiveSelectedModIds]);
    const selectedMods = useMemo(() => {
        if (!activeProfile) return [] as InstalledMod[];
        return activeProfile.mods.filter((m) => selectedModIdSet.has(m.uuid4));
    }, [activeProfile, selectedModIdSet]);
    const selectionMode = effectiveSelectedModIds.length > 0;

    const latestVersionByPackage = useMemo(() => {
        const map = new Map<string, string>();
        for (const pkg of packages) {
            const latest = pkg.versions?.[0]?.version_number;
            if (latest) {
                map.set(pkg.full_name, latest);
            }
        }
        return map;
    }, [packages]);

    const updatesInView = useMemo(() => {
        return displayedMods.reduce((count, mod) => {
            const modNameWithoutVersion = mod.fullName.replace(/-\d+\.\d+\.\d+$/, '');
            const latestVersion = latestVersionByPackage.get(modNameWithoutVersion);
            const hasUpdate = !!(latestVersion && latestVersion !== mod.versionNumber);
            return hasUpdate && mod.enabled ? count + 1 : count;
        }, 0);
    }, [displayedMods, latestVersionByPackage]);

    const pendingSyncCount = useMemo(() => {
        if (legacyInstallMode || !activeProfile) return 0;
        const markedMods = activeProfile.mods.filter((m) => m.pending_sync).length;
        if (markedMods > 0) return markedMods;
        return activeProfile.needs_sync ? 1 : 0;
    }, [activeProfile, legacyInstallMode]);

    const resolveAndOpenModDetails = async (mod: InstalledMod, pkg?: Package) => {
        if (pkg) {
            onViewModDetails(pkg);
            return;
        }

        try {
            const resolved = await onResolvePackage(mod);
            if (resolved) onViewModDetails(resolved);
        } catch (err) {
            console.error(err);
        }
    };

    const handleSelectRow = (e: React.MouseEvent, modId: string) => {
        if (!activeProfile) return;

        const currentIndex = displayedMods.findIndex((m) => m.uuid4 === modId);
        if (currentIndex === -1) return;

        if (e.shiftKey && selectionAnchorId) {
            const anchorIndex = displayedMods.findIndex((m) => m.uuid4 === selectionAnchorId);
            if (anchorIndex !== -1) {
                const start = Math.min(anchorIndex, currentIndex);
                const end = Math.max(anchorIndex, currentIndex);
                const rangeIds = displayedMods.slice(start, end + 1).map((m) => m.uuid4);
                setSelectedModIds(rangeIds);
                setSelectionAnchorId(modId);
                return;
            }
            setSelectedModIds([modId]);
            setSelectionAnchorId(modId);
            return;
        }

        if (e.metaKey || e.ctrlKey) {
            setSelectedModIds((prev) =>
                prev.includes(modId) ? prev.filter((id) => id !== modId) : [...prev, modId]
            );
            setSelectionAnchorId(modId);
            return;
        }
    };

    const handleToggleSingleMod = async (mod: InstalledMod) => {
        if (!activeProfile) return;
        await onToggleMod(activeProfile.id, mod.uuid4);
        if (!mod.enabled && legacyInstallMode) {
            setTimeout(() => {
                onInstallToGame();
            }, 300);
        }
    };

    const handleBulkDisableSelected = async () => {
        if (!activeProfile || selectedMods.length === 0) return;
        const enabledCount = selectedMods.filter((m) => m.enabled).length;
        const disabledCount = selectedMods.length - enabledCount;
        const bulkMode: 'disable' | 'enable' | 'toggle' =
            disabledCount === 0 ? 'disable' : enabledCount === 0 ? 'enable' : 'toggle';

        const targets = bulkMode === 'disable'
            ? selectedMods.filter((m) => m.enabled)
            : bulkMode === 'enable'
                ? selectedMods.filter((m) => !m.enabled)
                : selectedMods;

        const enablesAny = targets.some((m) => !m.enabled);
        if (targets.length === 0) return;

        const concurrency = installInParallel ? MAX_PARALLEL_TOGGLES : 1;
        for (let i = 0; i < targets.length; i += concurrency) {
            const batch = targets.slice(i, i + concurrency);
            if (concurrency === 1) {
                await onToggleMod(activeProfile.id, batch[0].uuid4);
            } else {
                await Promise.all(batch.map((mod) => onToggleMod(activeProfile.id, mod.uuid4)));
            }
        }

        if (legacyInstallMode && enablesAny) {
            setTimeout(() => {
                onInstallToGame();
            }, 300);
        }
    };

    const handleBulkDeleteSelected = async () => {
        if (selectedMods.length === 0) return;
        const confirmed = await window.ipcRenderer.confirm(
            'Remove Selected Mods',
            `Remove ${selectedMods.length} selected mod(s) from this profile?`
        );
        if (!confirmed) return;
        for (const mod of selectedMods) {
            await onUninstallMod(mod);
        }
        setSelectedModIds([]);
        setSelectionAnchorId(null);
    };

    const handleModRowClick = (e: React.MouseEvent, mod: InstalledMod, pkg?: Package) => {
        if (e.shiftKey || e.metaKey || e.ctrlKey) {
            handleSelectRow(e, mod.uuid4);
            return;
        }

        if (selectionMode) {
            setSelectedModIds([]);
            setSelectionAnchorId(null);
        }

        void resolveAndOpenModDetails(mod, pkg);
    };

    const bulkActionLabel = useMemo(() => {
        const enabledCount = selectedMods.filter((m) => m.enabled).length;
        const disabledCount = selectedMods.length - enabledCount;
        if (selectedMods.length === 0) return 'Disable';
        if (disabledCount === 0) return 'Disable';
        if (enabledCount === 0) return 'Enable';
        return 'Toggle';
    }, [selectedMods]);

    return (
        <div className="h-full flex flex-col bg-gray-900 border-r border-gray-800 w-80 flex-shrink-0">
            {/* Header */}
            <div className="px-5 py-[19px] border-b border-gray-800">
                <div className="flex items-center gap-3">
                    <button
                        onClick={() => onSelectProfile('')}
                        className="text-gray-400 hover:text-white p-1.5 -ml-2 rounded-lg hover:bg-gray-800 transition-colors"
                        title="Change Profile"
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10 19l-7-7m0 0l7-7m-7 7h18" />
                        </svg>
                    </button>
                    {activeProfile?.profileImageUrl ? (
                        <img
                            src={activeProfile.profileImageUrl}
                            alt={activeProfile.name}
                            className={`w-12 h-12 rounded-xl shadow-lg object-cover bg-gray-800 ${activeProfile?.is_vanilla ? 'grayscale' : ''}`}
                        />
                    ) : (
                        <div className={`w-12 h-12 bg-gradient-to-br from-blue-500 to-purple-600 rounded-xl shadow-lg flex items-center justify-center text-xl font-bold text-white ${activeProfile?.is_vanilla ? 'grayscale' : ''}`}>
                            {activeProfile?.name.charAt(0).toUpperCase()}
                        </div>
                    )}
                    <div className="min-w-0 flex-1">
                        <div className="flex items-center gap-2">
                            <h2 className="font-bold text-white truncate text-lg">{activeProfile?.name}</h2>
                            {activeProfile?.platform === 'mac' ? (
                                <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 384 512" fill="currentColor" className="text-gray-400">
                                    <title>MacOS Profile</title>
                                    <path d="M318.7 268.7c-.2-36.7 16.4-64.4 50-84.8-18.8-26.9-47.2-41.7-84.7-44.6-35.5-2.8-74.3 20.7-88.5 20.7-15 0-49.4-19.7-76.4-19.7C63.3 141.2 4 184.8 4 273.5q0 39.3 14.4 81.2c12.8 36.7 59 126.7 107.2 125.2 25.2-.6 43-17.9 75.8-17.9 31.8 0 48.3 17.9 76.4 17.9 48.6-.7 90.4-82.5 102.6-119.3-65.2-30.7-61.7-90-61.7-91.9zm-56.6-164.2c27.3-32.4 24.8-61.9 24-72.5-24.1 1.4-52 16.4-67.9 34.9-17.5 19.8-27.8 44.3-25.6 71.9 26.1 2 49.9-11.4 69.5-34.3z" />
                                </svg>
                            ) : (
                                <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="currentColor" className="text-gray-400">
                                    <title>Windows Profile</title>
                                    <path d="M0 3.449L9.75 2.1v9.451H0m10.949-9.602L24 0v11.4H10.949M0 12.6h9.75v9.451L0 20.699M10.949 12.6H24V24l-12.9-1.801" />
                                </svg>
                            )}
                        </div>
                        {activeProfile?.is_vanilla ? (
                            <p className="text-xs text-gray-500 font-bold uppercase tracking-wider">DISABLED</p>
                        ) : (
                            <p className="text-xs text-gray-500 truncate">{activeProfile?.mods.length} mods in profile</p>
                        )}
                    </div>
                    {/* Header Actions */}
                    <div className="flex gap-1">
                        <button
                            onClick={handleEditClick}
                            className="p-1.5 text-gray-400 hover:text-white hover:bg-gray-800 rounded-lg transition-colors"
                            title="Edit Profile"
                        >
                            <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15.232 5.232l3.536 3.536m-2.036-5.036a2.5 2.5 0 113.536 3.536L6.5 21.036H3v-3.572L16.732 3.732z" />
                            </svg>
                        </button>
                        <button
                            onClick={async () => {
                                if (activeProfile) {
                                    if (activeProfile.mods.length === 0 && !activeProfile.is_vanilla) {
                                        alert("No mods to disable!");
                                        return;
                                    }
                                    const newVanillaState = !activeProfile.is_vanilla;
                                    await onToggleVanilla(activeProfile.id, newVanillaState);
                                }
                            }}
                            className={`p-1.5 rounded-lg transition-colors ${activeProfile?.is_vanilla
                                ? 'text-yellow-500 bg-yellow-500/10 hover:bg-yellow-500/20'
                                : 'text-gray-400 hover:text-yellow-500 hover:bg-yellow-500/10'
                                }`}
                            title={activeProfile?.is_vanilla ? "Enable Mods" : "Disable All Mods (Vanilla)"}
                        >
                            <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M18.364 18.364A9 9 0 005.636 5.636m12.728 12.728A9 9 0 015.636 5.636m12.728 12.728L5.636 5.636" />
                            </svg>
                        </button>
                    </div>
                </div>
            </div>

            {/* Local Mod Search */}
            <div className="px-4 pt-4 pb-2">
                <div className="relative group">
                    <div className="absolute inset-y-0 left-0 pl-3 flex items-center pointer-events-none">
                        <svg className={`w-4 h-4 transition-colors duration-200 ${searchQuery ? 'text-blue-500' : 'text-gray-500 group-focus-within:text-blue-500'}`} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
                        </svg>
                    </div>
                    <input
                        type="text"
                        value={searchQuery}
                        onChange={(e) => setSearchQuery(e.target.value)}
                        placeholder="search profile mods..."
                        className="w-full pl-9 pr-3 py-2 bg-gray-800 border border-gray-700 rounded-lg text-sm text-white placeholder-gray-500 focus:outline-none focus:border-blue-500 focus:ring-1 focus:ring-blue-500 transition-all"
                    />
                </div>
            </div>

            {/* Mod List */}
            <div className={`flex-1 overflow-y-auto p-2 space-y-1 scrollbar-thin scrollbar-thumb-gray-700 scrollbar-track-transparent ${activeProfile?.is_vanilla ? 'grayscale opacity-75 pointer-events-none' : ''}`}>
                <div className="px-2 py-2 text-xs font-bold text-gray-500 uppercase tracking-wider flex justify-between items-center">
                    <div className="flex items-center gap-2">
                        <span>Profile Mods</span>
                        {pendingSyncCount > 0 && (
                            <span
                                title='Pending sync changes. Click "Apply to Game" to sync modified profile mods.'
                                className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-full text-[10px] bg-sky-500/10 text-sky-300 border border-sky-500/25"
                            >
                                <svg xmlns="http://www.w3.org/2000/svg" className="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.9} d="M20 11a8.1 8.1 0 0 0-15.5-2m-.5-4v4h4" />
                                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.9} d="M4 13a8.1 8.1 0 0 0 15.5 2m.5 4v-4h-4" />
                                </svg>
                                <span>{pendingSyncCount}</span>
                            </span>
                        )}
                        {updatesInView > 0 && (
                            <span
                                className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-full text-[10px] bg-amber-500/10 text-amber-400 border border-amber-500/20"
                                title={`${updatesInView} mod${updatesInView === 1 ? '' : 's'} have updates available`}
                            >
                                <svg xmlns="http://www.w3.org/2000/svg" className="h-3 w-3" viewBox="0 0 20 20" fill="currentColor">
                                    <path fillRule="evenodd" d="M8.257 3.099c.765-1.36 2.722-1.36 3.486 0l6.518 11.597c.75 1.334-.213 2.98-1.742 2.98H3.48c-1.53 0-2.493-1.646-1.743-2.98L8.257 3.1zM11 14a1 1 0 10-2 0 1 1 0 002 0zm-1-6a1 1 0 00-1 1v3a1 1 0 102 0V9a1 1 0 00-1-1z" clipRule="evenodd" />
                                </svg>
                                <span>{updatesInView}</span>
                            </span>
                        )}
                    </div>
                    <span className="bg-gray-800 text-gray-400 px-2 py-0.5 rounded-full text-[10px]">{displayedMods.length}</span>
                </div>

                {displayedMods.map(mod => {
                    const modNameWithoutVersion = mod.fullName.replace(/-\d+\.\d+\.\d+$/, '');
                    const pkg = packages.find(p => p.full_name === modNameWithoutVersion);
                    const latestVersion = pkg?.versions[0].version_number;
                    const hasUpdate = latestVersion && latestVersion !== mod.versionNumber;
                    const isSelected = selectedModIdSet.has(mod.uuid4);

                    return (
                        <div
                            key={mod.uuid4}
                            className={`flex items-center gap-3 p-2 rounded-lg group cursor-pointer transition-all border relative pr-16 overflow-hidden ${isSelected
                                ? 'bg-blue-500/12 border-blue-500/35'
                                : 'border-transparent hover:border-gray-700 hover:bg-gray-800'
                                } ${!mod.enabled ? 'opacity-50' : ''}`}
                            onClick={(e) => handleModRowClick(e, mod, pkg)}
                        >
                            {/* ... existing mod item content ... */}
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
                                </div>
                                <div className="text-xs text-gray-500 flex items-center gap-2">
                                    <span>v{mod.versionNumber}</span>
                                    {!legacyInstallMode && mod.pending_sync && (
                                        <span
                                            className="inline-flex items-center text-sky-300"
                                            title='Pending sync. Click "Apply to Game" to apply profile changes.'
                                        >
                                            <svg xmlns="http://www.w3.org/2000/svg" className="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.9} d="M20 11a8.1 8.1 0 0 0-15.5-2m-.5-4v4h4" />
                                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.9} d="M4 13a8.1 8.1 0 0 0 15.5 2m.5 4v-4h-4" />
                                            </svg>
                                            <span className="sr-only">Pending sync</span>
                                        </span>
                                    )}
                                    {hasUpdate && mod.enabled && (
                                        <span
                                            className="inline-flex items-center text-amber-400"
                                            title={`Update available: v${latestVersion}. Open details to update.`}
                                        >
                                            <svg xmlns="http://www.w3.org/2000/svg" className="h-3.5 w-3.5" viewBox="0 0 20 20" fill="currentColor">
                                                <path fillRule="evenodd" d="M8.257 3.099c.765-1.36 2.722-1.36 3.486 0l6.518 11.597c.75 1.334-.213 2.98-1.742 2.98H3.48c-1.53 0-2.493-1.646-1.743-2.98L8.257 3.1zM11 14a1 1 0 10-2 0 1 1 0 002 0zm-1-6a1 1 0 00-1 1v3a1 1 0 102 0V9a1 1 0 00-1-1z" clipRule="evenodd" />
                                            </svg>
                                            <span className="sr-only">Update available</span>
                                        </span>
                                    )}
                                </div>
                            </div>

                            <div className="absolute right-2 top-1/2 -translate-y-1/2 flex items-center opacity-0 group-hover:opacity-100 transition-opacity gap-1 bg-gray-800/90 rounded-lg p-1 shadow-sm backdrop-blur-sm z-20">
                                <button
                                    onClick={(e) => {
                                        e.stopPropagation();
                                        void handleToggleSingleMod(mod);
                                    }}
                                    className={`p-1.5 rounded-md transition-colors ${mod.enabled
                                        ? 'text-gray-400 hover:text-yellow-400 hover:bg-yellow-400/10'
                                        : 'text-yellow-500 bg-yellow-500/10 hover:bg-yellow-500/20'
                                        }`}
                                    title={mod.enabled ? 'Disable Mod' : 'Enable Mod'}
                                >
                                    <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M18.364 18.364A9 9 0 005.636 5.636m12.728 12.728A9 9 0 015.636 5.636m12.728 12.728L5.636 5.636" />
                                    </svg>
                                </button>
                                <button
                                    onClick={(e) => {
                                        e.stopPropagation();
                                        onOpenModFolder(activeProfile!.id, mod.fullName);
                                    }}
                                    className="p-1.5 text-gray-400 hover:text-blue-400 hover:bg-blue-400/10 rounded-md transition-colors"
                                    title="Locate in Finder"
                                >
                                    <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
                                    </svg>
                                </button>
                                <button
                                    onClick={async (e) => {
                                        e.stopPropagation();
                                        const confirmed = await window.ipcRenderer.confirm(
                                            'Uninstall Mod',
                                            `Uninstall ${mod.fullName}?`
                                        );
                                        if (!confirmed) return;
                                        onUninstallMod(mod);
                                    }}
                                    className="p-1.5 text-gray-400 hover:text-red-400 hover:bg-red-400/10 rounded-md transition-colors"
                                    title="Uninstall"
                                >
                                    <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                                    </svg>
                                </button>
                            </div>
                        </div>
                    );
                })}
                {activeProfile?.mods.length === 0 && (
                    <div className="text-center py-12 px-4 flex flex-col items-center">
                        <svg xmlns="http://www.w3.org/2000/svg" className="h-16 w-16 mb-3 opacity-20 text-gray-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4" />
                        </svg>
                        <p className="text-gray-500 text-sm font-medium">No mods in profile</p>
                        <p className="text-gray-600 text-xs mt-1">Search for mods to get started</p>
                    </div>
                )}
            </div>

            {/* Footer Actions */}
            <div className="p-4 border-t border-gray-800 bg-gray-900/50 backdrop-blur-sm space-y-3">
                {/* Game Info - Always Show */}
                {currentCommunity && (
                    <div className="flex items-center gap-3 px-2">
                        <div className="w-8 h-8 rounded-lg overflow-hidden bg-gray-800 flex-shrink-0 border border-gray-700 shadow-sm">
                            {communityImage ? (
                                <img
                                    src={communityImage}
                                    alt={currentCommunity.name}
                                    className="w-full h-full object-cover"
                                />
                            ) : (
                                <div className="w-full h-full flex items-center justify-center text-gray-600 text-xs font-bold">
                                    {currentCommunity.name.charAt(0)}
                                </div>
                            )}
                        </div>
                        <div className="min-w-0">
                            <h3 className="text-sm font-bold text-gray-200 truncate leading-tight">{currentCommunity.name}</h3>
                        </div>
                    </div>
                )}

                {/* Install Button - Only if profile active */}
                {activeProfile && (
                    <button
                        onClick={() => !activeProfile.is_vanilla && onInstallToGame()}
                        disabled={activeProfile.is_vanilla}
                        className={`w-full group relative flex items-center justify-center gap-2 px-4 py-3 rounded-xl transition-all duration-200 ${activeProfile.is_vanilla
                            ? 'bg-gray-700 text-gray-400 cursor-not-allowed grayscale'
                            : 'bg-gradient-to-r from-blue-600 to-indigo-600 hover:from-blue-500 hover:to-indigo-500 text-white shadow-lg shadow-blue-900/20 hover:shadow-blue-900/40 hover:-translate-y-0.5 active:translate-y-0'
                            }`}
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
                        </svg>
                        <span className="font-bold text-sm tracking-wide">
                            {activeProfile.is_vanilla ? 'Mods Disabled' : 'Apply to Game'}
                        </span>
                    </button>
                )}


                {/* Secondary Actions */}
                {selectionMode ? (
                    <div className="grid grid-cols-2 gap-2">
                        <button
                            onClick={() => { void handleBulkDisableSelected(); }}
                            className="flex items-center justify-center gap-2 px-3 py-2 rounded-lg bg-yellow-500/10 hover:bg-yellow-500/20 text-yellow-300 transition-colors text-xs font-medium border border-yellow-500/30"
                            title={`${bulkActionLabel} ${selectedMods.length} selected mod(s)`}
                        >
                            <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M18.364 18.364A9 9 0 005.636 5.636m12.728 12.728A9 9 0 015.636 5.636m12.728 12.728L5.636 5.636" />
                            </svg>
                            {bulkActionLabel} ({selectedMods.length})
                        </button>
                        <button
                            onClick={() => { void handleBulkDeleteSelected(); }}
                            className="flex items-center justify-center gap-2 px-3 py-2 rounded-lg bg-red-500/10 hover:bg-red-500/20 text-red-300 transition-colors text-xs font-medium border border-red-500/30"
                            title={`Delete ${selectedMods.length} selected mod(s)`}
                        >
                            <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                            </svg>
                            Delete ({selectedMods.length})
                        </button>
                    </div>
                ) : (
                    <div className="grid grid-cols-2 gap-2">
                        {activeProfile ? (
                            <button
                                onClick={onExportProfile}
                                className="flex items-center justify-center gap-2 px-3 py-2 rounded-lg bg-gray-800 hover:bg-gray-700 text-gray-400 hover:text-white transition-colors text-xs font-medium border border-gray-700 hover:border-gray-600"
                            >
                                <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 7H5a2 2 0 00-2 2v9a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-3m-1 4l-3 3m0 0l-3-3m3 3V4" />
                                </svg>
                                Export
                            </button>
                        ) : (
                            <div />
                        )}
                        <button
                            onClick={onOpenSettings}
                            className={`flex items-center justify-center gap-2 px-3 py-2 rounded-lg bg-gray-800 hover:bg-gray-700 text-gray-400 hover:text-white transition-colors text-xs font-medium border border-gray-700 hover:border-gray-600 ${!activeProfile ? 'col-span-2' : ''}`}
                        >
                            <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                            </svg>
                            Settings
                        </button>
                    </div>
                )}
            </div>
            {/* Edit Profile Modal (Local) */}
            {isEditing && activeProfile && (
                <div className="fixed inset-0 bg-black/70 backdrop-blur-sm flex items-center justify-center z-50 p-4">
                    <div className="bg-gray-800 rounded-xl p-6 max-w-md w-full border border-gray-700 shadow-2xl">
                        <h2 className="text-2xl font-bold text-white mb-4">Edit Profile</h2>

                        <div className="flex justify-center mb-6">
                            <div className="relative group cursor-pointer" onClick={handleImageSelect}>
                                {activeProfile.profileImageUrl ? (
                                    <img
                                        src={activeProfile.profileImageUrl}
                                        alt="Profile"
                                        className="w-24 h-24 rounded-full object-cover border-4 border-gray-700 group-hover:border-blue-500 transition-colors"
                                    />
                                ) : (
                                    <div className="w-24 h-24 rounded-full bg-gradient-to-br from-blue-500 to-purple-600 flex items-center justify-center text-4xl font-bold text-white border-4 border-gray-700 group-hover:border-blue-500 transition-colors">
                                        {activeProfile.name.charAt(0).toUpperCase()}
                                    </div>
                                )}
                                <div className="absolute inset-0 bg-black/50 rounded-full flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity">
                                    <span className="text-white font-medium text-xs">Change</span>
                                </div>
                            </div>
                        </div>

                        {activeProfile.profileImageUrl && (
                            <div className="text-center mb-4">
                                <button
                                    type="button"
                                    onClick={handleRemoveImage}
                                    className="text-xs text-red-400 hover:text-red-300 hover:underline"
                                >
                                    Remove Custom Image
                                </button>
                            </div>
                        )}

                        <form onSubmit={handleUpdate}>
                            <input
                                type="text"
                                value={editName}
                                onChange={(e) => setEditName(e.target.value)}
                                placeholder="Profile Name"
                                className="w-full bg-gray-900 border border-gray-700 rounded-lg px-4 py-3 text-white placeholder-gray-500 focus:outline-none focus:border-blue-500 mb-6"
                                autoFocus
                            />
                            <div className="flex gap-3">
                                <Button variant="secondary" fullWidth onClick={() => setIsEditing(false)} type="button">
                                    Cancel
                                </Button>
                                <Button variant="primary" fullWidth type="submit" disabled={!editName.trim()}>
                                    Save
                                </Button>
                            </div>
                        </form>
                    </div>
                </div>
            )}
        </div >
    );
};
