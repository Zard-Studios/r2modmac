import { useState, useEffect } from 'react';
import type { PackageVersion, Package } from '../types/thunderstore'; // Import Package type if needed (checking usage)
import DOMPurify from 'dompurify';

interface ModDetailModalProps {
    mod: PackageVersion;
    isOpen: boolean;
    onClose: () => void;
    onInstall: () => void;
    onUpdate?: () => void;
    onUninstall?: () => void;
    isInstalled: boolean;
    hasUpdate?: boolean;
    gameId: string;
}

type Tab = 'description' | 'changelog' | 'dependencies';

export function ModDetailModal({ mod, isOpen, onClose, onInstall, onUpdate, onUninstall, isInstalled, hasUpdate = false, gameId }: ModDetailModalProps) {
    const [activeTab, setActiveTab] = useState<Tab>('description');
    const [readmeContent, setReadmeContent] = useState<string | null>(null);
    const [changelogContent, setChangelogContent] = useState<string | null>(null);
    const [loadingContent, setLoadingContent] = useState(false);
    const [dependencies, setDependencies] = useState<Package[]>([]);
    const [loadingKey, setLoadingKey] = useState<string>('');

    useEffect(() => {
        if (isOpen && mod && loadingKey !== mod.full_name) {
            // Reset state on open/mod change
            setLoadingKey(mod.full_name);
            setReadmeContent(null);
            setChangelogContent(null);
            setDependencies([]);
            setActiveTab('description');
            fetchContent('readme');

            if (mod.dependencies?.length > 0 && gameId) {
                fetchDependencies();
            }
        }
    }, [isOpen, mod.full_name, gameId]);

    const fetchDependencies = async () => {
        try {
            const deps = await window.ipcRenderer.lookupPackagesByNames(gameId, mod.dependencies);
            if (Array.isArray(deps)) {
                // The backend returns Value which might be array of packages
                setDependencies(deps);
            }
        } catch (e) {
            console.error("Failed to fetch dependencies", e);
        }
    };

    const fetchContent = async (type: 'readme' | 'changelog') => {
        setLoadingContent(true);
        try {
            // Construct Thunderstore API URL -> returns JSON with 'html' field
            // Format: https://thunderstore.io/api/cyberstorm/package/{owner}/{name}/v/{version}/{type}/
            // mod.full_name is "Owner-Name-Version"
            const parts = mod.full_name.split('-');
            const owner = parts[0];
            const name = parts[1];
            const version = parts[2]; // or mod.version_number

            const url = `https://thunderstore.io/api/cyberstorm/package/${owner}/${name}/v/${version}/${type}/`;

            const jsonString = await window.ipcRenderer.fetchTextContent(url);
            const data = JSON.parse(jsonString);

            if (data.html) {
                const sanitized = DOMPurify.sanitize(data.html);
                if (type === 'readme') setReadmeContent(sanitized);
                else setChangelogContent(sanitized);
            }
        } catch (e) {
            console.error(`Failed to fetch ${type}:`, e);
        } finally {
            setLoadingContent(false);
        }
    };

    // Fetch changelog when tab is switched, if not already loaded
    useEffect(() => {
        if (activeTab === 'changelog' && !changelogContent) {
            fetchContent('changelog');
        }
    }, [activeTab]);

    if (!isOpen) return null;

    return (
        <div
            className="fixed inset-0 bg-black/50 backdrop-blur-sm flex items-center justify-center z-50 p-6"
            onClick={onClose}
        >
            <div
                className="bg-gray-800 rounded-xl max-w-4xl w-full h-[90vh] flex flex-col border border-gray-700 shadow-2xl"
                onClick={(e) => e.stopPropagation()}
            >
                {/* Header with Icon and Title */}
                <div className="flex items-start gap-4 p-6 border-b border-gray-700 bg-gray-900/50 flex-shrink-0">
                    <div className="w-20 h-20 bg-gray-900 rounded-lg flex-shrink-0 overflow-hidden border border-gray-700 relative group">
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
                        <div className="absolute inset-0 bg-black/40 opacity-0 group-hover:opacity-100 transition-opacity flex items-center justify-center pointer-events-none">
                            <span className="text-white font-bold text-xs">View</span>
                        </div>
                        {mod.website_url && (
                            <button
                                className="absolute inset-0 w-full h-full cursor-pointer opacity-0"
                                onClick={() => {
                                    import('@tauri-apps/plugin-shell').then(({ open }) => {
                                        open(mod.website_url!);
                                    });
                                }}
                                title="Open Website"
                            />
                        )}
                    </div>

                    <div className="flex-1 min-w-0">
                        <div className="flex justify-between items-start">
                            <div>
                                <h2 className="text-2xl font-bold text-white mb-1">{mod.name}</h2>
                                <p className="text-sm text-gray-400 mb-2">by {mod.full_name.split('-')[0]}</p>
                            </div>
                            <div className="text-right flex flex-col items-end gap-1">
                                <div>
                                    <span className="block text-xs uppercase text-gray-500 font-bold tracking-wider mb-0.5">Updated</span>
                                    <span className="text-sm text-gray-300">{new Date(mod.date_created).toLocaleDateString()}</span>
                                </div>
                                {mod.website_url && (
                                    <button
                                        onClick={() => {
                                            import('@tauri-apps/plugin-shell').then(({ open }) => {
                                                open(mod.website_url!);
                                            });
                                        }}
                                        className="flex items-center gap-1 text-sm text-blue-400 hover:text-blue-300 transition-colors mt-2"
                                    >
                                        Website
                                        <svg className="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
                                        </svg>
                                    </button>
                                )}
                            </div>
                        </div>

                        <div className="flex items-center gap-3 text-xs text-gray-500 mt-2">
                            <span className="bg-gray-700 px-2 py-1 rounded text-gray-300">v{mod.version_number}</span>
                            <span className="flex items-center gap-1" title={`${mod.downloads.toLocaleString()} downloads`}>
                                <svg className="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
                                </svg>
                                {mod.downloads.toLocaleString()}
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

                {/* Tabs */}
                <div className="flex border-b border-gray-700 bg-gray-900/30 px-6 gap-6">
                    {(['description', 'changelog', 'dependencies'] as const).map(tab => (
                        <button
                            key={tab}
                            onClick={() => setActiveTab(tab)}
                            className={`py-3 text-sm font-medium border-b-2 transition-colors capitalize ${activeTab === tab
                                    ? 'text-blue-400 border-blue-400'
                                    : 'text-gray-400 border-transparent hover:text-gray-200'
                                }`}
                        >
                            {tab}
                        </button>
                    ))}
                </div>

                {/* Scrollable Content */}
                <div className="overflow-y-auto flex-1 p-6 min-h-0 bg-gray-850">
                    <style>{`
                        .prose details {
                            background-color: rgba(31, 41, 55, 0.5);
                            border: 1px solid rgba(75, 85, 99, 0.4);
                            border-radius: 0.5rem;
                            padding: 0.5rem;
                            margin-top: 1rem;
                            margin-bottom: 1rem;
                        }
                        .prose summary {
                            cursor: pointer;
                            font-weight: 600;
                            color: #d1d5db;
                            outline: none;
                        }
                        .prose summary:hover {
                            color: #ffffff;
                        }
                    `}</style>
                    {activeTab === 'description' && (
                        <div>
                            {loadingContent && !readmeContent ? (
                                <div className="animate-pulse space-y-3">
                                    <div className="h-4 bg-gray-700 rounded w-3/4"></div>
                                    <div className="h-4 bg-gray-700 rounded w-1/2"></div>
                                    <div className="h-4 bg-gray-700 rounded w-5/6"></div>
                                </div>
                            ) : readmeContent ? (
                                <div
                                    className="prose prose-invert prose-sm max-w-none prose-headings:text-white prose-a:text-blue-400 hover:prose-a:text-blue-300 prose-img:rounded-lg break-words overflow-hidden"
                                    dangerouslySetInnerHTML={{ __html: readmeContent }}
                                />
                            ) : (
                                <div className="text-center text-gray-500 py-10">
                                    <p>{mod.description || "No description available."}</p>
                                </div>
                            )}
                        </div>
                    )}

                    {activeTab === 'changelog' && (
                        <div>
                            {loadingContent && !changelogContent ? (
                                <div className="animate-pulse space-y-3">
                                    <div className="h-4 bg-gray-700 rounded w-1/4"></div>
                                    <div className="h-4 bg-gray-700 rounded w-full"></div>
                                </div>
                            ) : changelogContent ? (
                                <div
                                    className="prose prose-invert prose-sm max-w-none prose-headings:text-white prose-a:text-blue-400 hover:prose-a:text-blue-300 break-words overflow-hidden"
                                    dangerouslySetInnerHTML={{ __html: changelogContent }}
                                />
                            ) : (
                                <div className="text-center text-gray-500 py-10">
                                    <p>No changelog found.</p>
                                </div>
                            )}
                        </div>
                    )}

                    {activeTab === 'dependencies' && (
                        <div>
                            {dependencies.length > 0 ? (
                                <div className="space-y-2">
                                    {dependencies.map((dep, idx) => (
                                        <div key={idx} className="bg-gray-800 p-3 rounded border border-gray-700 flex items-center justify-between">
                                            <div className="flex items-center gap-3">
                                                <div className="w-12 h-12 bg-gray-900 rounded flex-shrink-0 overflow-hidden border border-gray-700">
                                                    {dep.versions[0]?.icon ? (
                                                        <img src={dep.versions[0].icon} alt={dep.name} className="w-full h-full object-cover" />
                                                    ) : (
                                                        <div className="w-full h-full flex items-center justify-center text-xs font-bold text-gray-600">
                                                            {dep.name[0]}
                                                        </div>
                                                    )}
                                                </div>
                                                <div>
                                                    <p className="text-white font-medium text-sm">{dep.name}</p>
                                                    <p className="text-gray-500 text-xs truncate max-w-[300px]">{dep.versions[0]?.description}</p>
                                                </div>
                                            </div>
                                        </div>
                                    ))}
                                </div>
                            ) : (
                                // Fallback if lookup failed or still loading or no deps
                                mod.dependencies && mod.dependencies.length > 0 ? (
                                    <div className="space-y-2 opacity-50">
                                        {mod.dependencies.map((dep, idx) => (
                                            <div key={idx} className="bg-gray-800 p-3 rounded border border-gray-700 flex items-center gap-3">
                                                <div className="w-10 h-10 bg-gray-900 rounded flex items-center justify-center text-gray-500 text-xs font-mono border border-gray-700">DEP</div>
                                                <span className="text-gray-300 font-mono text-sm">{dep}</span>
                                            </div>
                                        ))}
                                    </div>
                                ) : (
                                    <div className="text-center text-gray-500 py-10">
                                        <p>No dependencies.</p>
                                    </div>
                                )
                            )}
                        </div>
                    )}
                </div>

                {/* Footer with Actions */}
                <div className="p-6 border-t border-gray-700 bg-gray-900/50 flex gap-3 flex-shrink-0">
                    {activeTab === 'description' && (
                        <div className="absolute top-0 right-0 hidden"></div> // Spacer hack if needed
                    )}
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
