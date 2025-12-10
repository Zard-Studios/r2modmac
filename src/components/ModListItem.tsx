
import type { PackageVersion } from '../types/thunderstore';

interface ModListItemProps {
    mod: PackageVersion;
    onInstall: () => void;
    onUninstall?: () => void;
    onClick?: () => void;
    installStatus: 'installed' | 'not_installed' | 'update_available';
}

export function ModListItem({ mod, onInstall, onUninstall, onClick, installStatus }: ModListItemProps) {
    // Format bytes to human readable string (KB, MB, GB)
    const formatBytes = (bytes: number): string => {
        if (bytes >= 1000 * 1000 * 1000) {
            return `${(bytes / (1000 * 1000 * 1000)).toFixed(1)} GB`;
        } else if (bytes >= 1000 * 1000) {
            return `${(bytes / (1000 * 1000)).toFixed(1)} MB`;
        } else if (bytes >= 1000) {
            return `${(bytes / 1000).toFixed(0)} KB`;
        }
        return `${bytes} B`;
    };

    return (
        <div
            className="flex items-center gap-4 p-3 bg-gray-800 border border-gray-700 rounded-lg hover:border-blue-500/50 hover:bg-gray-750 transition-all cursor-pointer group"
            onClick={onClick}
        >
            {/* Icon */}
            <div className="w-12 h-12 bg-gray-900 rounded flex-shrink-0 overflow-hidden border border-gray-700">
                {mod.icon ? (
                    <img
                        src={mod.icon}
                        alt={mod.name}
                        className="w-full h-full object-cover"
                        loading="lazy"
                    />
                ) : (
                    <div className="w-full h-full flex items-center justify-center text-xl font-bold text-gray-600">
                        {mod.name[0]}
                    </div>
                )}
            </div>

            {/* Info */}
            <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2 mb-0.5">
                    <h3 className="font-bold text-white text-base truncate group-hover:text-blue-400 transition-colors">
                        {mod.name}
                    </h3>
                    <span className="bg-gray-700/50 px-1.5 py-0.5 rounded text-xs text-gray-400">v{mod.version_number}</span>
                    {mod.website_url && (
                        <button
                            onClick={(e) => {
                                e.stopPropagation();
                                import('@tauri-apps/plugin-shell').then(({ open }) => {
                                    open(mod.website_url!);
                                });
                            }}
                            className="text-gray-500 hover:text-blue-400 transition-colors"
                            title="Open Website"
                        >
                            <svg className="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
                            </svg>
                        </button>
                    )}
                </div>
                <p className="text-sm text-gray-400 truncate pr-4">
                    {mod.description}
                </p>
            </div>

            {/* Stats */}
            <div className="flex flex-col items-end gap-1 text-xs text-gray-500 font-medium w-24 justify-center flex-shrink-0 mr-4">
                <div className="flex items-center gap-1.5" title="Downloads">
                    <svg className="w-3.5 h-3.5 text-gray-600" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" /></svg>
                    <span className="text-gray-400">{mod.downloads.toLocaleString()}</span>
                </div>

                {mod.dependencies && mod.dependencies.length >= 5 ? (
                    <div className="flex items-center gap-1.5 text-indigo-400" title="Modpack (contains multiple mods)">
                        <svg className="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10" /></svg>
                        <span>{mod.dependencies.length} mods</span>
                    </div>
                ) : (
                    <div className="flex items-center gap-1.5" title="File Size">
                        <svg className="w-3.5 h-3.5 text-gray-600" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4" /></svg>
                        <span className="text-gray-400">{formatBytes(mod.file_size)}</span>
                    </div>
                )}
            </div>

            {/* Actions */}
            <div className="flex items-center gap-2 pl-4 border-l border-gray-700 flex-shrink-0 min-w-[140px] justify-end">
                {installStatus === 'installed' && onUninstall && (
                    <button
                        onClick={(e) => {
                            e.stopPropagation();
                            onUninstall();
                        }}
                        className="p-2 rounded-lg bg-red-500/10 text-red-500 hover:bg-red-500 hover:text-white transition-all duration-200 border border-red-500/20"
                        title="Uninstall"
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                        </svg>
                    </button>
                )}

                <button
                    onClick={(e) => {
                        e.stopPropagation();
                        if (installStatus !== 'installed') {
                            onInstall();
                        }
                    }}
                    disabled={installStatus === 'installed'}
                    className={`px-4 py-1.5 rounded-lg text-sm font-semibold transition-all duration-200 min-w-[90px] text-center ${installStatus === 'installed'
                        ? 'bg-green-500/10 text-green-500 border border-green-500/20 cursor-default'
                        : installStatus === 'update_available'
                            ? 'bg-yellow-600 hover:bg-yellow-500 text-white shadow-lg shadow-yellow-900/20 active:scale-95'
                            : 'bg-blue-600 hover:bg-blue-500 text-white shadow-lg shadow-blue-900/20 active:scale-95'
                        }`}
                >
                    {installStatus === 'installed' ? 'Installed' : installStatus === 'update_available' ? 'Update' : 'Install'}
                </button>
            </div>
        </div>
    );
}
