import React, { useState } from 'react';
import type { Community, Package } from '../types/thunderstore';
import type { Profile, InstalledMod } from '../types/profile';

interface ProfileSidebarProps {
    activeProfile: Profile | undefined;
    currentCommunity: Community | null;
    communityImage: string | undefined;
    packages: Package[];
    onSelectProfile: (profileId: string) => void;
    onToggleMod: (profileId: string, modUuid: string) => void;
    onViewModDetails: (pkg: Package) => void;
    onOpenModFolder: (profileId: string, modName: string) => void;
    onUninstallMod: (mod: InstalledMod) => void;
    onInstallToGame: () => void;
    onResolvePackage: (mod: InstalledMod) => Promise<Package | null>;
    onExportProfile: () => void;
    onOpenSettings: () => void;
}

export const ProfileSidebar: React.FC<ProfileSidebarProps> = ({
    activeProfile,
    currentCommunity,
    communityImage,
    packages,
    onSelectProfile,
    onToggleMod,
    onViewModDetails,
    onOpenModFolder,
    onUninstallMod,
    onInstallToGame,
    onResolvePackage,
    onExportProfile,
    onOpenSettings
}) => {
    const [searchQuery, setSearchQuery] = useState('');

    const displayedMods = activeProfile?.mods.filter(mod =>
        mod.fullName.toLowerCase().includes(searchQuery.toLowerCase())
    ) || [];

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
                            className="w-12 h-12 rounded-xl shadow-lg object-cover bg-gray-800"
                        />
                    ) : (
                        <div className="w-12 h-12 bg-gradient-to-br from-blue-500 to-purple-600 rounded-xl shadow-lg flex items-center justify-center text-xl font-bold text-white">
                            {activeProfile?.name.charAt(0).toUpperCase()}
                        </div>
                    )}
                    <div className="min-w-0 flex-1">
                        <h2 className="font-bold text-white truncate text-lg">{activeProfile?.name}</h2>
                        <p className="text-xs text-gray-500 truncate">{activeProfile?.mods.length} mods installed</p>
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
                        placeholder="search installed mods..."
                        className="w-full pl-9 pr-3 py-2 bg-gray-800 border border-gray-700 rounded-lg text-sm text-white placeholder-gray-500 focus:outline-none focus:border-blue-500 focus:ring-1 focus:ring-blue-500 transition-all"
                    />
                </div>
            </div>

            {/* Mod List */}
            <div className="flex-1 overflow-y-auto p-2 space-y-1 scrollbar-thin scrollbar-thumb-gray-700 scrollbar-track-transparent">
                <div className="px-2 py-2 text-xs font-bold text-gray-500 uppercase tracking-wider flex justify-between items-center">
                    <span>Installed Mods</span>
                    <span className="bg-gray-800 text-gray-400 px-2 py-0.5 rounded-full text-[10px]">{displayedMods.length}</span>
                </div>

                {displayedMods.map(mod => {
                    const modNameWithoutVersion = mod.fullName.replace(/-\d+\.\d+\.\d+$/, '');
                    const pkg = packages.find(p => p.full_name === modNameWithoutVersion);
                    const latestVersion = pkg?.versions[0].version_number;
                    const hasUpdate = latestVersion && latestVersion !== mod.versionNumber;

                    return (
                        <div
                            key={mod.uuid4}
                            className={`flex items-center gap-3 p-2 rounded-lg hover:bg-gray-800 group cursor-pointer transition-all border border-transparent hover:border-gray-700 relative pr-16 overflow-hidden ${!mod.enabled ? 'opacity-50' : ''}`}
                            onClick={() => onToggleMod(activeProfile!.id, mod.uuid4)}
                        >
                            {!mod.enabled && (
                                <div
                                    className="absolute inset-0 pointer-events-none z-10 opacity-30"
                                    style={{
                                        background: 'repeating-linear-gradient(45deg, #000 0px, #000 20px, #fbbf24 20px, #fbbf24 40px)',
                                        mixBlendMode: 'multiply'
                                    }}
                                />
                            )}

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

                            <div className="absolute right-2 top-1/2 -translate-y-1/2 flex items-center opacity-0 group-hover:opacity-100 transition-opacity gap-1 bg-gray-800/90 rounded-lg p-1 shadow-sm backdrop-blur-sm z-20">
                                <button
                                    onClick={async (e) => {
                                        e.stopPropagation();
                                        if (pkg) {
                                            onViewModDetails(pkg);
                                        } else {
                                            // Fallback: try to resolve via API
                                            try {
                                                const resolved = await onResolvePackage(mod);
                                                if (resolved) {
                                                    onViewModDetails(resolved);
                                                } else {
                                                    console.warn("Could not resolve package for mod:", mod.fullName);
                                                }
                                            } catch (err) {
                                                console.error("Error resolving package:", err);
                                            }
                                        }
                                    }}
                                    className="p-1.5 text-gray-400 hover:text-blue-400 hover:bg-blue-400/10 rounded-md transition-colors"
                                    title="View Details"
                                >
                                    <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                                    </svg>
                                </button>
                                <button
                                    onClick={(e) => {
                                        e.stopPropagation();
                                        onOpenModFolder(activeProfile!.id, mod.fullName.split('-')[1]);
                                    }}
                                    className="p-1.5 text-gray-400 hover:text-blue-400 hover:bg-blue-400/10 rounded-md transition-colors"
                                    title="Locate in Finder"
                                >
                                    <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
                                    </svg>
                                </button>
                                <button
                                    onClick={(e) => {
                                        e.stopPropagation();
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
                        <p className="text-gray-500 text-sm font-medium">No mods installed</p>
                        <p className="text-gray-600 text-xs mt-1">Search for mods to get started</p>
                    </div>
                )}
            </div>

            {/* Footer Actions */}
            <div className="p-4 border-t border-gray-800 bg-gray-900/50 backdrop-blur-sm space-y-3">
                {/* Game Info */}
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

                {/* Install Button */}
                <button
                    onClick={onInstallToGame}
                    className="w-full group relative flex items-center justify-center gap-2 px-4 py-3 rounded-xl bg-gradient-to-r from-blue-600 to-indigo-600 hover:from-blue-500 hover:to-indigo-500 text-white shadow-lg shadow-blue-900/20 transition-all duration-200 hover:shadow-blue-900/40 hover:-translate-y-0.5 active:translate-y-0"
                >
                    <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
                    </svg>
                    <span className="font-bold text-sm tracking-wide">Apply to Game</span>
                </button>

                {/* Secondary Actions */}
                <div className="grid grid-cols-2 gap-2">
                    <button
                        onClick={onExportProfile}
                        className="flex items-center justify-center gap-2 px-3 py-2 rounded-lg bg-gray-800 hover:bg-gray-700 text-gray-400 hover:text-white transition-colors text-xs font-medium border border-gray-700 hover:border-gray-600"
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 7H5a2 2 0 00-2 2v9a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-3m-1 4l-3 3m0 0l-3-3m3 3V4" />
                        </svg>
                        Export
                    </button>
                    <button
                        onClick={onOpenSettings}
                        className="flex items-center justify-center gap-2 px-3 py-2 rounded-lg bg-gray-800 hover:bg-gray-700 text-gray-400 hover:text-white transition-colors text-xs font-medium border border-gray-700 hover:border-gray-600"
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                        </svg>
                        Settings
                    </button>
                </div>
            </div>
        </div>
    );
};
