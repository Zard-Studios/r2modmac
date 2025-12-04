import type { PackageVersion } from '../types/thunderstore';

interface ModCardProps {
    mod: PackageVersion;
    onInstall: () => void;
    onUninstall?: () => void;
    onClick?: () => void;
    installStatus: 'installed' | 'not_installed' | 'update_available';
}

export function ModCard({ mod, onInstall, onUninstall, onClick, installStatus }: ModCardProps) {
    return (
        <div
            className="bg-gray-800 rounded-xl p-4 border border-gray-700 hover:border-blue-500/50 transition-all duration-200 group flex flex-col h-full cursor-pointer"
            onClick={onClick}
        >
            {/* Header with Icon and Title */}
            <div className="flex gap-4 mb-3">
                <div className="w-16 h-16 bg-gray-900 rounded-lg flex-shrink-0 overflow-hidden border border-gray-700">
                    {mod.icon ? (
                        <img
                            src={mod.icon}
                            alt={mod.name}
                            className="w-full h-full object-cover"
                            loading="lazy"
                        />
                    ) : (
                        <div className="w-full h-full flex items-center justify-center text-2xl font-bold text-gray-600">
                            {mod.name[0]}
                        </div>
                    )}
                </div>

                <div className="flex-1 min-w-0 flex flex-col justify-center">
                    <h3 className="font-bold text-white text-lg truncate group-hover:text-blue-400 transition-colors">
                        {mod.name}
                    </h3>
                    <div className="flex items-center gap-2 text-xs text-gray-400">
                        <span className="bg-gray-700/50 px-1.5 py-0.5 rounded">v{mod.version_number}</span>
                        {mod.website_url && (
                            <button
                                onClick={(e) => {
                                    e.stopPropagation();
                                    import('@tauri-apps/plugin-shell').then(({ open }) => {
                                        open(mod.website_url!);
                                    });
                                }}
                                className="hover:text-blue-400 transition-colors cursor-pointer"
                            >
                                Website ↗
                            </button>
                        )}
                    </div>
                </div>
            </div>

            {/* Description */}
            <p className="text-sm text-gray-400 mb-4 line-clamp-2 flex-1">
                {mod.description}
            </p>

            {/* Footer: Stats & Action */}
            <div className="flex items-center justify-between mt-auto pt-3 border-t border-gray-700/50">
                <div className="flex items-center gap-3 text-xs text-gray-500 font-medium">
                    <div className="flex items-center gap-1">
                        <svg className="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" /></svg>
                        {mod.downloads.toLocaleString()}
                    </div>
                    <div className="flex items-center gap-1">
                        <svg className="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4" /></svg>
                        {(mod.file_size / 1024).toFixed(0)} KB
                    </div>
                </div>

                <div className="flex gap-2">
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
                        className={`px-4 py-1.5 rounded-lg text-sm font-semibold transition-all duration-200 ${installStatus === 'installed'
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
        </div>
    );
}
