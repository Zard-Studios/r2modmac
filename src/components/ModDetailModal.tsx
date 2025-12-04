import type { PackageVersion } from '../types/thunderstore';

interface ModDetailModalProps {
    mod: PackageVersion;
    isOpen: boolean;
    onClose: () => void;
    onInstall: () => void;
    onUpdate?: () => void;
    onUninstall?: () => void;
    isInstalled: boolean;
    hasUpdate?: boolean;
}

export function ModDetailModal({ mod, isOpen, onClose, onInstall, onUpdate, onUninstall, isInstalled, hasUpdate = false }: ModDetailModalProps) {
    if (!isOpen) return null;

    return (
        <div
            className="fixed inset-0 bg-black/50 backdrop-blur-sm flex items-center justify-center z-50 p-6"
            onClick={onClose}
        >
            <div
                className="bg-gray-800 rounded-xl max-w-3xl w-full max-h-[90vh] flex flex-col border border-gray-700 shadow-2xl"
                onClick={(e) => e.stopPropagation()}
            >
                {/* Header with Icon and Title */}
                <div className="flex items-start gap-4 p-6 border-b border-gray-700 bg-gray-900/50 flex-shrink-0">
                    <div className="w-20 h-20 bg-gray-900 rounded-lg flex-shrink-0 overflow-hidden border border-gray-700">
                        {mod.icon ? (
                            <img
                                src={mod.icon}
                                alt={mod.name}
                                className="w-full h-full object-cover"
                            />
                        ) : (
                            <div className="w-full h-full flex items-center justify-center text-3xl font-bold text-gray-600">
                                {mod.name[0]}
                            </div>
                        )}
                    </div>

                    <div className="flex-1 min-w-0">
                        <h2 className="text-2xl font-bold text-white mb-1">{mod.name}</h2>
                        <p className="text-sm text-gray-400 mb-2">by {mod.full_name.split('-')[0]}</p>
                        <div className="flex items-center gap-3 text-xs text-gray-500">
                            <span className="bg-gray-700 px-2 py-1 rounded">v{mod.version_number}</span>
                            <span className="flex items-center gap-1">
                                <svg className="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
                                </svg>
                                {mod.downloads.toLocaleString()} downloads
                            </span>
                            <span className="flex items-center gap-1">
                                <svg className="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4" />
                                </svg>
                                {(mod.file_size / 1024).toFixed(0)} KB
                            </span>
                        </div>
                    </div>

                    <button
                        onClick={onClose}
                        className="text-gray-400 hover:text-white transition-colors p-2 flex-shrink-0"
                    >
                        ✕
                    </button>
                </div>

                {/* Scrollable Content */}
                <div className="overflow-y-auto flex-1 p-6 space-y-6 min-h-0">
                    {/* Description */}
                    <div>
                        <h3 className="text-sm font-bold uppercase tracking-wider text-gray-400 mb-2">Description</h3>
                        <p className="text-gray-300 whitespace-pre-wrap">{mod.description}</p>
                    </div>

                    {/* Dependencies */}
                    {mod.dependencies && mod.dependencies.length > 0 && (
                        <div>
                            <h3 className="text-sm font-bold uppercase tracking-wider text-gray-400 mb-2">
                                Dependencies ({mod.dependencies.length})
                            </h3>
                            <div className="space-y-1">
                                {mod.dependencies.map((dep, idx) => (
                                    <div key={idx} className="bg-gray-900/50 px-3 py-2 rounded border border-gray-700 text-sm text-gray-300 font-mono">
                                        {dep}
                                    </div>
                                ))}
                            </div>
                        </div>
                    )}

                    {/* Metadata */}
                    <div className="grid grid-cols-2 gap-4">
                        <div>
                            <h3 className="text-sm font-bold uppercase tracking-wider text-gray-400 mb-2">Created</h3>
                            <p className="text-gray-300">{new Date(mod.date_created).toLocaleDateString()}</p>
                        </div>

                        {mod.website_url && (
                            <div>
                                <h3 className="text-sm font-bold uppercase tracking-wider text-gray-400 mb-2">Website</h3>
                                <button
                                    onClick={() => {
                                        import('@tauri-apps/plugin-shell').then(({ open }) => {
                                            open(mod.website_url!);
                                        });
                                    }}
                                    className="text-blue-400 hover:text-blue-300 transition-colors text-sm cursor-pointer"
                                >
                                    Visit Website ↗
                                </button>
                            </div>
                        )}
                    </div>
                </div>

                {/* Footer with Actions */}
                <div className="p-6 border-t border-gray-700 bg-gray-900/50 flex gap-3 flex-shrink-0">
                    {isInstalled && onUninstall && (
                        <button
                            onClick={() => {
                                onUninstall();
                            }}
                            className="p-2 rounded-lg bg-red-500/10 text-red-500 hover:bg-red-500 hover:text-white transition-all duration-200 border border-red-500/20 flex-shrink-0"
                            title="Uninstall"
                        >
                            <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                            </svg>
                        </button>
                    )}
                    <button
                        onClick={() => {
                            if (hasUpdate && onUpdate) {
                                onUpdate();
                            } else {
                                onInstall();
                            }
                            onClose();
                        }}
                        disabled={isInstalled && !hasUpdate}
                        className={`flex-1 px-4 py-2.5 rounded-lg font-semibold transition-colors ${hasUpdate
                            ? 'bg-amber-500 hover:bg-amber-600 text-white'
                            : isInstalled
                                ? 'bg-green-500/10 text-green-500 border border-green-500/20 cursor-default'
                                : 'bg-blue-600 hover:bg-blue-500 text-white'
                            }`}
                    >
                        {hasUpdate ? 'Update' : isInstalled ? 'Installed' : 'Install Mod'}
                    </button>
                </div>
            </div>
        </div>
    );
}
