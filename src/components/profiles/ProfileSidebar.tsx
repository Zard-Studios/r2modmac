import React, { startTransition, useMemo, useState } from 'react';
import type { Community, Package } from '../../types/thunderstore';
import type { Profile, InstalledMod } from '../../types/profile';
import type { RuntimeHealth } from '../../types/electron';
import type { ProfileModUpdate } from '../../hooks/useModActions';
import { Button, HoverMarquee } from '../ui';
import { compareVersions, hasNewerVersion, latestVersionNumber, parsePackageReference } from '../../utils/modVersioning';

const MAX_PARALLEL_TOGGLES = 10;

const localModToPackage = (mod: InstalledMod): Package => {
    const owner = mod.author || 'Local';
    const name = mod.displayName || mod.fullName.split('-')[1] || mod.fullName;
    const version = mod.versionNumber || '1.0.0';
    const fullName = `${owner}-${name}`;
    const versionFullName = `${fullName}-${version}`;
    const now = new Date().toISOString();

    return {
        name,
        full_name: fullName,
        owner,
        package_url: '',
        date_created: now,
        date_updated: now,
        uuid4: mod.uuid4,
        rating_score: 0,
        is_pinned: false,
        is_deprecated: false,
        has_nsfw_content: false,
        categories: ['Custom'],
        versions: [{
            name,
            full_name: versionFullName,
            description: mod.description || 'Custom local mod',
            icon: mod.iconUrl || '',
            version_number: version,
            dependencies: [],
            download_url: '',
            downloads: 0,
            date_created: now,
            website_url: '',
            is_active: true,
            uuid4: mod.uuid4,
            file_size: mod.fileSize || 0,
            localReadme: mod.readme,
            isLocal: true,
        }]
    };
};

function getFirstLetter(name: string | undefined): string {
    if (!name) return '';
    try {
        const segmenter = new Intl.Segmenter(undefined, { granularity: 'grapheme' });
        const segments = segmenter.segment(name);
        const firstSegment = [...segments][0]?.segment;
        return firstSegment ? firstSegment.toUpperCase() : '';
    } catch {
        return ([...name][0] || '').toUpperCase();
    }
}

interface ProfileSidebarProps {
    activeProfile: Profile | undefined;
    currentCommunity: Community | null;
    communityImage: string | undefined;
    packageIndex: Record<string, Package>;
    legacyInstallMode: boolean;
    showDeprecatedWarnings: boolean;
    installInParallel: boolean;
    onSelectProfile: (profileId: string) => void;
    onToggleMod: (profileId: string, modUuid: string) => Promise<void> | void;
    onViewModDetails: (pkg: Package) => void;
    onOpenModFolder: (profileId: string, modName: string) => void;
    onUninstallMod: (mod: InstalledMod) => Promise<void> | void;
    onInstallToGame: (isVanillaOverride?: boolean) => void;
    onLaunchProfile: () => Promise<void> | void;
    onStopProfile: () => Promise<void> | void;
    isApplying?: boolean;
    isLaunching?: boolean;
    isBusy?: boolean;
    isSteamRestarting?: boolean;
    isGameRunning?: boolean;
    hasConfiguredGamePath?: boolean;
    isCheckingGamePath?: boolean;
    onResolvePackage: (mod: InstalledMod) => Promise<Package | null>;
    onExportProfile: () => void;
    onImportCustomMod?: () => void;
    onOpenSettings: () => void;
    onUpdateProfile: (profileId: string, updates: Partial<Profile>) => void;
    onToggleVanilla: (profileId: string, newVanillaState: boolean) => Promise<void> | void;
    onUpdateMod: (pkg: Package, profileId?: string, version?: Package['versions'][number]) => Promise<void> | void;
    onUpdateAll: (updates: ProfileModUpdate[]) => void;
    runtimeHealth?: RuntimeHealth | null;
    isRepairingRuntime?: boolean;
    onRepairRuntime: () => Promise<void> | void;
}

export const ProfileSidebar: React.FC<ProfileSidebarProps> = ({
    activeProfile,
    currentCommunity,
    communityImage,
    packageIndex,
    legacyInstallMode,
    showDeprecatedWarnings,
    installInParallel,
    onSelectProfile,
    onToggleMod,
    onViewModDetails,
    onOpenModFolder,
    onUninstallMod,
    onInstallToGame,
    onLaunchProfile,
    onStopProfile,
    isApplying = false,
    isLaunching = false,
    isBusy = false,
    isSteamRestarting = false,
    isGameRunning = false,
    hasConfiguredGamePath = false,
    isCheckingGamePath = false,
    onResolvePackage,
    onExportProfile,
    onImportCustomMod,
    onOpenSettings,
    onUpdateProfile,
    onToggleVanilla,
    onUpdateMod,
    onUpdateAll,
    runtimeHealth,
    isRepairingRuntime = false,
    onRepairRuntime,
}) => {
    const [searchQuery, setSearchQuery] = useState('');
    const [isEditing, setIsEditing] = useState(false);
    const [editName, setEditName] = useState('');
    const [selectedModIds, setSelectedModIds] = useState<string[]>([]);
    const [selectionAnchorId, setSelectionAnchorId] = useState<string | null>(null);
    const [modView, setModView] = useState<'all' | 'updates'>('all');

    const changeModView = (nextView: 'all' | 'updates') => {
        if (nextView === modView) return;
        startTransition(() => setModView(nextView));
    };

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

    const latestVersionByPackage = useMemo(() => {
        const map = new Map<string, string>();
        for (const pkg of Object.values(packageIndex)) {
            const latest = latestVersionNumber(pkg);
            if (latest) map.set(pkg.full_name.toLowerCase(), latest);
        }
        return map;
    }, [packageIndex]);

    const profileUpdates = useMemo<ProfileModUpdate[]>(() => {
        if (!activeProfile) return [];
        return activeProfile.mods.flatMap(mod => {
            if (mod.source === 'local') return [];
            const packageName = parsePackageReference(mod.fullName).packageName.toLowerCase();
            const pkg = packageIndex[packageName];
            const latest = pkg ? latestVersionByPackage.get(packageName) : undefined;
            if (!pkg || !hasNewerVersion(mod.versionNumber, latest)) return [];
            const version = pkg.versions.find(candidate => candidate.version_number === latest)
                || pkg.versions.reduce((newest, candidate) =>
                    compareVersions(candidate.version_number, newest.version_number) > 0 ? candidate : newest
                );
            return [{ mod, pkg, version }];
        });
    }, [activeProfile, latestVersionByPackage, packageIndex]);
    const updateIds = useMemo(() => new Set(profileUpdates.map(update => update.mod.uuid4)), [profileUpdates]);
    const updatesById = useMemo(
        () => new Map(profileUpdates.map(update => [update.mod.uuid4, update])),
        [profileUpdates]
    );
    const searchedMods = activeProfile?.mods.filter(mod =>
        `${mod.fullName} ${mod.displayName || ''} ${mod.author || ''}`.toLowerCase().includes(searchQuery.toLowerCase())
    ) || [];
    const displayedMods = modView === 'updates'
        ? searchedMods.filter(mod => updateIds.has(mod.uuid4))
        : searchedMods;
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
    const launchActionBlocked = !isGameRunning && (isCheckingGamePath || !hasConfiguredGamePath);
    let launchActionTitle = 'Game directory is not configured. Open Settings and set the path before launching.';
    if (isGameRunning) {
        launchActionTitle = 'Stop Game';
    } else if (isSteamRestarting) {
        launchActionTitle = 'Steam is restarting to apply launch options...';
    } else if (isCheckingGamePath) {
        launchActionTitle = 'Checking game directory...';
    } else if (hasConfiguredGamePath && activeProfile) {
        launchActionTitle = activeProfile.launchMode === 'direct'
            ? (activeProfile.is_vanilla ? 'Launch Vanilla Direct' : 'Launch Direct')
            : activeProfile.is_vanilla
                ? 'Launch Vanilla'
                : 'Launch Modded';
    }
    const launchActionButton = (
        <button
            onClick={() => {
                if (isGameRunning) {
                    void onStopProfile();
                    return;
                }
                void onLaunchProfile();
            }}
            disabled={isLaunching || isApplying || isSteamRestarting || launchActionBlocked}
            className={`w-14 flex items-center justify-center rounded-xl text-white border shadow-sm ${(isLaunching || isApplying || isSteamRestarting)
                ? 'bg-gray-700 border-gray-600 cursor-wait opacity-70'
                : launchActionBlocked
                    ? 'bg-gray-700 border-gray-600 cursor-not-allowed opacity-60'
                    : isGameRunning
                        ? 'bg-red-600 border-red-500'
                        : 'bg-green-600 border-green-500'
                }`}
            title={launchActionTitle}
        >
            {isGameRunning ? (
                <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
                    <rect width="10" height="10" x="3" y="3" rx="1.5" />
                </svg>
            ) : isLaunching || isApplying || isSteamRestarting ? (
                <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4 animate-spin" viewBox="0 0 16 16" fill="none" aria-hidden="true">
                    <circle cx="8" cy="8" r="5.25" stroke="currentColor" strokeOpacity="0.25" strokeWidth="1.5" />
                    <path d="M8 2.75a5.25 5.25 0 0 1 5.25 5.25" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
                </svg>
            ) : (
                <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="currentColor" viewBox="0 0 16 16" aria-hidden="true">
                    <path d="M3 3.732a1.5 1.5 0 0 1 2.305-1.265l6.706 4.267a1.5 1.5 0 0 1 0 2.531l-6.706 4.268A1.5 1.5 0 0 1 3 12.267V3.732Z" />
                </svg>
            )}
        </button>
    );

    const pendingSyncCount = useMemo(() => {
        if (legacyInstallMode || !activeProfile) return 0;
        const markedMods = activeProfile.mods.filter((m) => m.pending_sync).length;
        if (markedMods > 0) return markedMods;
        return activeProfile.needs_sync ? 1 : 0;
    }, [activeProfile, legacyInstallMode]);

    const resolveAndOpenModDetails = async (mod: InstalledMod, pkg?: Package) => {
        if (mod.source === 'local') {
            onViewModDetails(localModToPackage(mod));
            return;
        }

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
                            {getFirstLetter(activeProfile?.name)}
                        </div>
                    )}
                    <div className="min-w-0 flex-1">
                        <div className="flex min-w-0 items-center gap-2">
                            <HoverMarquee text={activeProfile?.name || ''} className="font-bold text-white text-lg" />
                            {activeProfile?.platform === 'mac' ? (
                                <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 384 512" fill="currentColor" className="h-3.5 w-3.5 shrink-0 text-gray-400">
                                    <title>MacOS Profile</title>
                                    <path d="M318.7 268.7c-.2-36.7 16.4-64.4 50-84.8-18.8-26.9-47.2-41.7-84.7-44.6-35.5-2.8-74.3 20.7-88.5 20.7-15 0-49.4-19.7-76.4-19.7C63.3 141.2 4 184.8 4 273.5q0 39.3 14.4 81.2c12.8 36.7 59 126.7 107.2 125.2 25.2-.6 43-17.9 75.8-17.9 31.8 0 48.3 17.9 76.4 17.9 48.6-.7 90.4-82.5 102.6-119.3-65.2-30.7-61.7-90-61.7-91.9zm-56.6-164.2c27.3-32.4 24.8-61.9 24-72.5-24.1 1.4-52 16.4-67.9 34.9-17.5 19.8-27.8 44.3-25.6 71.9 26.1 2 49.9-11.4 69.5-34.3z" />
                                </svg>
                            ) : (
                                <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="currentColor" className="h-3.5 w-3.5 shrink-0 text-gray-400">
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
                        {activeProfile?.platform === 'mac' && activeProfile.launchMode !== 'auto' && (
                            <p className="text-[10px] text-gray-500 font-bold uppercase tracking-wider">
                                {activeProfile.launchMode === 'direct'
                                    ? 'Direct launch'
                                    : 'Steam launch'}
                            </p>
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
                                if (isBusy) return;
                                if (activeProfile) {
                                    if (activeProfile.mods.length === 0 && !activeProfile.is_vanilla) {
                                        alert("No mods to disable!");
                                        return;
                                    }
                                    const newVanillaState = !activeProfile.is_vanilla;
                                    await onToggleVanilla(activeProfile.id, newVanillaState);
                                }
                            }}
                            disabled={isBusy}
                            className={`p-1.5 rounded-lg transition-colors ${activeProfile?.is_vanilla
                                ? 'text-yellow-500 bg-yellow-500/10 hover:bg-yellow-500/20'
                                : 'text-gray-400 hover:text-yellow-500 hover:bg-yellow-500/10'
                                }`}
                            style={isBusy ? { opacity: 0.5, cursor: 'not-allowed' } : undefined}
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
                <div className="flex gap-2">
                    <div className="relative group flex-1 min-w-0">
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
                            spellCheck={false}
                            autoCorrect="off"
                            autoCapitalize="none"
                            autoComplete="off"
                            className="w-full pl-9 pr-3 py-2 bg-gray-800 border border-gray-700 rounded-lg text-sm text-white placeholder-gray-500 focus:outline-none focus:border-blue-500 focus:ring-1 focus:ring-blue-500 transition-all"
                        />
                    </div>
                    <button
                        type="button"
                        onClick={() => onImportCustomMod?.()}
                        className="w-[38px] h-[38px] flex-shrink-0 rounded-lg bg-gray-800 border border-gray-700 text-gray-400 hover:text-white hover:border-blue-500 hover:bg-gray-800 focus:outline-none focus:border-blue-500 focus:ring-1 focus:ring-blue-500 transition-colors flex items-center justify-center"
                        title="Import Custom Mod"
                        aria-label="Import Custom Mod"
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" aria-hidden="true">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 5v14m7-7H5" />
                        </svg>
                    </button>
                </div>
            </div>

            {runtimeHealth && ['missing', 'incomplete', 'unconfigured'].includes(runtimeHealth.status) && (
                <div className="mx-4 mb-2 flex items-center gap-2 rounded-xl border border-amber-500/30 bg-amber-500/10 px-3 py-2.5">
                    <svg className="h-4 w-4 flex-shrink-0 text-amber-400" viewBox="0 0 20 20" fill="currentColor" aria-hidden="true">
                        <path fillRule="evenodd" d="M8.257 3.099c.765-1.36 2.722-1.36 3.486 0l6.518 11.597c.75 1.334-.213 2.98-1.742 2.98H3.48c-1.53 0-2.493-1.646-1.743-2.98L8.257 3.1zM11 14a1 1 0 10-2 0 1 1 0 002 0zm-1-6a1 1 0 00-1 1v3a1 1 0 102 0V9a1 1 0 00-1-1z" clipRule="evenodd" />
                    </svg>
                    <div className="min-w-0 flex-1">
                        <div className="truncate text-xs font-medium text-amber-200">
                            {runtimeHealth.status === 'unconfigured'
                                ? 'Game path not configured'
                                : `${runtimeHealth.runtime === 'bepinex' ? 'BepInEx' : runtimeHealth.runtime === 'owml' ? 'OWML' : 'Lovely'} runtime ${runtimeHealth.status}`}
                        </div>
                        {runtimeHealth.missingComponents.length > 0 ? (
                            <div className="truncate text-[10px] text-amber-300/70">Missing: {runtimeHealth.missingComponents.join(', ')}</div>
                        ) : null}
                    </div>
                    <button
                        type="button"
                        onClick={runtimeHealth.status === 'unconfigured' ? onOpenSettings : () => { void onRepairRuntime(); }}
                        disabled={isRepairingRuntime || (runtimeHealth.status !== 'unconfigured' && !runtimeHealth.repairable)}
                        className="flex-shrink-0 rounded-lg border border-amber-500/35 bg-amber-500/15 px-2.5 py-1.5 text-xs font-medium text-amber-200 disabled:opacity-50"
                    >
                        {runtimeHealth.status === 'unconfigured' ? 'Settings' : isRepairingRuntime ? 'Repairing…' : 'Repair'}
                    </button>
                </div>
            )}

            {/* Mod List */}
            <div className={`flex-1 overflow-y-auto p-2 space-y-1 scrollbar-thin scrollbar-thumb-gray-700 scrollbar-track-transparent ${activeProfile?.is_vanilla ? 'grayscale opacity-75' : ''}`}>
                <div className="px-2 py-2 text-xs font-bold">
                    <div role="tablist" aria-label="Profile mod view" className="relative flex w-full overflow-hidden rounded-lg border border-gray-700 bg-gray-800 p-0.5">
                        <div
                            aria-hidden="true"
                            className={`profile-mod-segment-indicator absolute bottom-0.5 left-0.5 top-0.5 w-[calc(50%-2px)] rounded-md transition-[transform,background-color,box-shadow] duration-300 ease-[cubic-bezier(0.22,1,0.36,1)] ${modView === 'updates'
                                ? 'translate-x-full bg-amber-500/20 shadow-[0_1px_8px_rgba(245,158,11,0.08)]'
                                : 'translate-x-0 bg-gray-600 shadow-sm'
                                }`}
                        />
                        <button
                            type="button"
                            role="tab"
                            aria-controls="profile-mod-view-panel"
                            onClick={() => changeModView('all')}
                            aria-selected={modView === 'all'}
                            className={`relative z-10 flex-1 rounded-md px-2 py-1.5 transition-colors duration-200 ${modView === 'all' ? 'text-white' : 'text-gray-400 hover:text-gray-200'}`}
                        >
                            <span className="inline-flex items-center justify-center gap-1.5">
                                All {activeProfile?.mods.length ?? 0}
                                {pendingSyncCount > 0 ? (
                                    <span
                                        title={`${pendingSyncCount} pending sync change${pendingSyncCount === 1 ? '' : 's'}`}
                                        className="h-1.5 w-1.5 rounded-full bg-sky-300 shadow-[0_0_5px_rgba(125,211,252,0.55)]"
                                    />
                                ) : null}
                            </span>
                        </button>
                        <button
                            type="button"
                            role="tab"
                            aria-controls="profile-mod-view-panel"
                            onClick={() => changeModView('updates')}
                            aria-selected={modView === 'updates'}
                            className={`relative z-10 flex-1 rounded-md px-2 py-1.5 transition-colors duration-200 ${modView === 'updates' ? 'text-amber-300' : 'text-gray-400 hover:text-amber-300'}`}
                        >
                            Updates {profileUpdates.length}
                        </button>
                    </div>
                </div>

                <div
                    key={modView}
                    id="profile-mod-view-panel"
                    role="tabpanel"
                    className={`space-y-1 ${modView === 'updates' ? 'profile-mod-view-enter-forward' : 'profile-mod-view-enter-backward'}`}
                >
                {modView === 'updates' && profileUpdates.length > 0 ? (
                    <button
                        type="button"
                        onClick={() => onUpdateAll(profileUpdates)}
                        className="group/update-all mx-2 mb-2 flex w-[calc(100%-16px)] items-center gap-3 rounded-xl border border-blue-500/25 bg-blue-500/10 px-3 py-2.5 text-left transition-[background-color,border-color,transform] duration-200 hover:border-blue-500/45 hover:bg-blue-500/15 active:scale-[0.985]"
                    >
                        <span className="flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-full bg-blue-500/15 text-blue-300">
                            <svg className="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 4v6h6M20 20v-6h-6M5.6 15a7 7 0 0011.9 2M18.4 9A7 7 0 006.5 7" />
                            </svg>
                        </span>
                        <span className="min-w-0 flex-1">
                            <span className="block text-xs font-bold text-blue-100">Update all {profileUpdates.length} mods</span>
                            <span className="block truncate text-[10px] font-medium text-blue-300/65">Review versions before updating</span>
                        </span>
                        <svg className="h-4 w-4 flex-shrink-0 text-blue-300/70 transition-transform duration-200 group-hover/update-all:translate-x-0.5" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="m9 18 6-6-6-6" />
                        </svg>
                    </button>
                ) : null}

                {displayedMods.map(mod => {
                    const packageName = parsePackageReference(mod.fullName).packageName.toLowerCase();
                    const pkg = mod.source === 'local' ? undefined : packageIndex[packageName];
                    const latestVersion = pkg ? latestVersionNumber(pkg) : undefined;
                    const hasUpdate = hasNewerVersion(mod.versionNumber, latestVersion);
                    const update = updatesById.get(mod.uuid4);
                    const isSelected = selectedModIdSet.has(mod.uuid4);
                    const displayName = mod.displayName || mod.fullName.split('-')[1] || mod.fullName;

                    return (
                        <div
                            key={mod.uuid4}
                            className={`flex items-center gap-3 p-2 rounded-lg group cursor-pointer transition-all border relative overflow-hidden ${modView === 'updates' ? 'pr-24' : 'pr-16'} ${isSelected
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
                                    <div className="w-full h-full flex items-center justify-center text-gray-500 text-xs font-bold">
                                        {mod.source === 'local' ? 'C' : '?'}
                                    </div>
                                )}
                                {!mod.enabled && (
                                    <div className="absolute inset-0 z-10 flex items-center justify-center bg-black/50">
                                        <span className="text-xs font-bold text-white">OFF</span>
                                    </div>
                                )}
                                {showDeprecatedWarnings && pkg?.is_deprecated ? (
                                    <span
                                        className="absolute right-0 top-0 z-20 flex h-5 w-5 items-center justify-center rounded-bl-lg bg-red-950/85 text-red-300 shadow-[-1px_1px_5px_rgba(0,0,0,0.35)] backdrop-blur-sm"
                                        title="Deprecated mod"
                                        aria-label="Deprecated mod"
                                    >
                                        <svg className="h-3.5 w-3.5" viewBox="0 0 20 20" fill="currentColor" aria-hidden="true">
                                            <path fillRule="evenodd" d="M8.26 3.1c.76-1.36 2.72-1.36 3.48 0l6.52 11.6c.75 1.33-.21 2.98-1.74 2.98H3.48c-1.53 0-2.49-1.65-1.74-2.98L8.26 3.1ZM10 7.75a.75.75 0 0 1 .75.75v3a.75.75 0 0 1-1.5 0v-3a.75.75 0 0 1 .75-.75Zm0 7.25a1 1 0 1 0 0-2 1 1 0 0 0 0 2Z" clipRule="evenodd" />
                                        </svg>
                                    </span>
                                ) : null}
                            </div>

                            <div className="min-w-0 flex-1">
                                <div className="flex items-center gap-2">
                                    <div className={`text-sm font-medium truncate transition-colors ${mod.enabled ? 'text-gray-200 group-hover:text-white' : 'text-gray-500 line-through'}`}>
                                        {displayName}
                                    </div>
                                    {mod.source === 'local' && (
                                        <span className="text-[10px] px-1.5 py-0.5 rounded-full border border-blue-500/25 bg-blue-500/10 text-blue-300">
                                            Custom
                                        </span>
                                    )}
                                </div>
                                <div className="flex min-w-0 items-center gap-2 overflow-hidden text-xs text-gray-500">
                                    <span className="truncate">
                                        v{mod.versionNumber}
                                        {modView === 'updates' && update ? (
                                            <span className="text-amber-300"> → v{update.version.version_number}</span>
                                        ) : null}
                                    </span>
                                    {!legacyInstallMode && mod.pending_sync && (
                                        <span
                                            className="inline-flex flex-shrink-0 items-center text-sky-300"
                                            title='Pending sync. Click "Apply to Game" to apply profile changes.'
                                        >
                                            <svg xmlns="http://www.w3.org/2000/svg" className="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.9} d="M20 11a8.1 8.1 0 0 0-15.5-2m-.5-4v4h4" />
                                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.9} d="M4 13a8.1 8.1 0 0 0 15.5 2m.5 4v-4h-4" />
                                            </svg>
                                            <span className="sr-only">Pending sync</span>
                                        </span>
                                    )}
                                    {hasUpdate && (
                                        <span
                                            className="inline-flex flex-shrink-0 items-center text-amber-400"
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

                            {modView === 'updates' && update ? (
                                <button
                                    type="button"
                                    onClick={(event) => {
                                        event.stopPropagation();
                                        void onUpdateMod(update.pkg, activeProfile?.id, update.version);
                                    }}
                                    className="absolute right-2 top-1/2 z-20 -translate-y-1/2 rounded-lg border border-amber-500/40 bg-amber-500/15 px-2.5 py-1.5 text-xs font-bold text-amber-200 transition-colors hover:bg-amber-500/25"
                                >
                                    Update
                                </button>
                            ) : (
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
                                        await onUninstallMod(mod);
                                    }}
                                    className="p-1.5 text-gray-400 hover:text-red-400 hover:bg-red-400/10 rounded-md transition-colors"
                                    title="Uninstall"
                                >
                                    <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                                    </svg>
                                </button>
                            </div>
                            )}
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
                {activeProfile && activeProfile.mods.length > 0 && displayedMods.length === 0 && (
                    <div className="flex flex-col items-center px-4 py-12 text-center">
                        <p className="text-sm font-medium text-gray-400">
                            {modView === 'updates' && profileUpdates.length === 0
                                ? 'All mods are up to date'
                                : 'No matching mods'}
                        </p>
                        <p className="mt-1 text-xs text-gray-600">
                            {modView === 'updates' && profileUpdates.length === 0
                                ? 'Updates will appear here when available.'
                                : 'Try a different search.'}
                        </p>
                    </div>
                )}
                </div>
            </div>

            {/* Footer Actions */}
            <div className="p-4 border-t border-gray-800 bg-gray-900/50 backdrop-blur-sm space-y-3">
                {/* Game Info - Always Show */}
                {currentCommunity && (
                    <div className="flex items-center gap-3">
                        <div className="w-8 h-8 rounded-lg overflow-hidden bg-gray-800 flex-shrink-0 border border-gray-700 shadow-sm">
                            {communityImage ? (
                                <img
                                    src={communityImage}
                                    alt={currentCommunity.name}
                                    className="w-full h-full object-cover"
                                />
                            ) : (
                                <div className="w-full h-full flex items-center justify-center text-gray-600 text-xs font-bold">
                                    {getFirstLetter(currentCommunity.name)}
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
                    <div className="flex gap-2">
                        <button
                            onClick={() => onInstallToGame()}
                            disabled={isApplying}
                            className={`flex-1 flex items-center justify-center gap-2 px-4 py-3 rounded-xl text-white border shadow-sm ${
                                isApplying
                                    ? 'bg-gray-700 border-gray-600 cursor-wait opacity-70'
                                    : 'bg-blue-600 border-blue-500'
                            }`}
                            title={activeProfile.apply_interrupted
                                ? 'Resume the interrupted profile apply'
                                : activeProfile.is_vanilla
                                    ? 'Sync mods into the disabled BepInEx runtime'
                                    : 'Apply mods to game'}
                        >
                            <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
                                <path d="M8.75 2.75a.75.75 0 0 0-1.5 0v5.69L5.03 6.22a.75.75 0 0 0-1.06 1.06l3.5 3.5a.75.75 0 0 0 1.06 0l3.5-3.5a.75.75 0 0 0-1.06-1.06L8.75 8.44V2.75Z" />
                                <path d="M3.5 9.75a.75.75 0 0 0-1.5 0v1.5A2.75 2.75 0 0 0 4.75 14h6.5A2.75 2.75 0 0 0 14 11.25v-1.5a.75.75 0 0 0-1.5 0v1.5c0 .69-.56 1.25-1.25 1.25h-6.5c-.69 0-1.25-.56-1.25-1.25v-1.5Z" />
                            </svg>
                            <span className="font-bold text-sm tracking-wide">
                                {isApplying ? 'Applying...' : activeProfile.apply_interrupted ? 'Resume Apply' : 'Apply to Game'}
                            </span>
                        </button>

                        {launchActionBlocked ? (
                            <span className="inline-flex" title={launchActionTitle}>
                                {launchActionButton}
                            </span>
                        ) : (
                            launchActionButton
                        )}
                    </div>
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
                                className="flex items-center justify-center gap-2 px-3 py-2 rounded-lg bg-gray-800 text-gray-300 text-xs font-medium border border-gray-700"
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
                            className={`flex items-center justify-center gap-2 px-3 py-2 rounded-lg bg-gray-800 text-gray-300 text-xs font-medium border border-gray-700 ${!activeProfile ? 'col-span-2' : ''}`}
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
                                        {getFirstLetter(activeProfile.name)}
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
