import { useState, useEffect } from 'react';
import { Button } from '../ui';
import { MAC_IMAGE_CACHE_KEY, MAC_PLATFORM_CACHE_KEY } from '../../constants/cacheKeys';

export interface PreferencesSettings {
    legacy_install_mode: boolean;
    ask_version_before_install: boolean;
    install_in_parallel: boolean;
    confirm_before_apply_to_game: boolean;
    default_mod_view_mode: 'grid' | 'list';
}

interface PreferencesModalProps {
    isOpen: boolean;
    onClose: () => void;
    settings: PreferencesSettings;
    onSave: (settings: PreferencesSettings) => void;
    hasHiddenGuideWarnings: boolean;
    onRestoreGuideWarnings: () => Promise<void>;
}

function Toggle({ value, onChange }: { value: boolean; onChange: (next: boolean) => void }) {
    return (
        <button
            onClick={() => onChange(!value)}
            className={`relative w-11 h-6 rounded-full transition-colors duration-200 ease-in-out flex-shrink-0 focus:outline-none focus:ring-2 focus:ring-blue-500/50 ${
                value ? 'bg-blue-600' : 'bg-gray-700 hover:bg-gray-600'
            }`}
             aria-pressed={value}
        >
            <span
                className={`absolute top-0.5 left-0.5 w-5 h-5 bg-white rounded-full shadow-[0_2px_5px_rgba(0,0,0,0.2)] transition-transform duration-200 ease-[cubic-bezier(0.4,0,0.2,1)] ${
                    value ? 'translate-x-5' : 'translate-x-0'
                }`}
            />
        </button>
    );
}

function IconBox({ children, colorClass }: { children: React.ReactNode; colorClass: string }) {
    return (
        <div className={`w-9 h-9 rounded-xl flex items-center justify-center bg-gray-700 border border-gray-600 flex-shrink-0 ${colorClass}`}>
            {children}
        </div>
    );
}

function RowIcon({ kind }: { kind: 'install' | 'version' | 'parallel' | 'apply' | 'layout' | 'warning' | 'cache' }) {
    if (kind === 'install') return (
        <IconBox colorClass="text-blue-400">
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-5l-4 4m0 0l-4-4m4 4V4" />
            </svg>
        </IconBox>
    );
    if (kind === 'version') return (
        <IconBox colorClass="text-cyan-400">
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
            </svg>
        </IconBox>
    );
    if (kind === 'parallel') return (
        <IconBox colorClass="text-violet-400">
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M13 10V3L4 14h7v7l9-11h-7z" />
            </svg>
        </IconBox>
    );
    if (kind === 'apply') return (
        <IconBox colorClass="text-emerald-400">
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M5 13l4 4L19 7" />
            </svg>
        </IconBox>
    );
    if (kind === 'layout') return (
        <IconBox colorClass="text-indigo-400">
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M4 6h7v12H4V6zm9 0h7v5h-7V6zm0 7h7v5h-7v-5z" />
            </svg>
        </IconBox>
    );
    if (kind === 'warning') return (
        <IconBox colorClass="text-amber-400">
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M12 9v2m0 4h.01m-7.938 4h15.876c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L2.33 16c-.77 1.333.192 3 1.732 3z" />
            </svg>
        </IconBox>
    );
    return (
        <div className="w-9 h-9 rounded-xl flex items-center justify-center bg-red-500/10 border border-red-500/20 text-red-400 flex-shrink-0">
            <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
            </svg>
        </div>
    );
}

export default function PreferencesModal({
    isOpen,
    onClose,
    settings,
    onSave,
    hasHiddenGuideWarnings,
    onRestoreGuideWarnings,
}: PreferencesModalProps) {
    const [legacyMode, setLegacyMode] = useState(settings.legacy_install_mode);
    const [askVersionBeforeInstall, setAskVersionBeforeInstall] = useState(settings.ask_version_before_install);
    const [installInParallel, setInstallInParallel] = useState(settings.install_in_parallel);
    const [confirmBeforeApply, setConfirmBeforeApply] = useState(settings.confirm_before_apply_to_game);
    const [defaultModViewMode, setDefaultModViewMode] = useState<'grid' | 'list'>(settings.default_mod_view_mode);
    const [restoringWarnings, setRestoringWarnings] = useState(false);

    // Track active state for a gentle reveal animation
    const [isVisible, setIsVisible] = useState(false);

    useEffect(() => {
        if (isOpen) {
            setIsVisible(true);
        } else {
            setIsVisible(false);
        }
    }, [isOpen]);

    useEffect(() => {
        setLegacyMode(settings.legacy_install_mode);
        setAskVersionBeforeInstall(settings.ask_version_before_install);
        setInstallInParallel(settings.install_in_parallel);
        setConfirmBeforeApply(settings.confirm_before_apply_to_game);
        setDefaultModViewMode(settings.default_mod_view_mode);
    }, [settings]);

    if (!isOpen) return null;

    const handleSave = () => {
        onSave({
            legacy_install_mode: legacyMode,
            ask_version_before_install: askVersionBeforeInstall,
            install_in_parallel: installInParallel,
            confirm_before_apply_to_game: confirmBeforeApply,
            default_mod_view_mode: defaultModViewMode,
        });
        onClose();
    };

    return (
        <div className={`fixed inset-0 z-50 flex items-center justify-center p-4 transition-all duration-300 ease-[cubic-bezier(0.4,0,0.2,1)] ${isVisible ? 'opacity-100 backdrop-blur-sm' : 'opacity-0 backdrop-blur-none'}`}>
            {/* Backdrop */}
            <div className="absolute inset-0 bg-black/60 transition-opacity" onClick={onClose} />
            
            {/* Modal Container */}
            <div className={`relative w-full max-w-[640px] max-h-[85vh] flex flex-col bg-gray-900 border border-gray-700 rounded-2xl shadow-2xl overflow-hidden transform transition-all duration-300 ease-[cubic-bezier(0.34,1.56,0.64,1)] ${isVisible ? 'scale-100 translate-y-0 opacity-100' : 'scale-95 translate-y-4 opacity-0'}`}>
                
                {/* Header */}
                <div className="flex items-center justify-between px-7 py-6 border-b border-gray-800 shrink-0 z-10 bg-gray-900">
                    <div>
                        <h2 className="text-2xl font-bold text-white tracking-tight">Preferences</h2>
                    </div>
                    <button
                        onClick={onClose}
                        className="p-2 rounded-xl hover:bg-gray-800 text-gray-400 hover:text-white transition-all active:scale-95 focus:outline-none focus:ring-2 focus:ring-gray-700"
                    >
                        <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M6 18L18 6M6 6l12 12" />
                        </svg>
                    </button>
                </div>

                {/* Scrollable Content */}
                <div className="p-7 space-y-8 overflow-y-auto flex-1 bg-gray-900 relative z-0">
                    
                    {/* Setup behavior Section */}
                    <div className="space-y-3">
                        <h3 className="text-xs font-semibold text-gray-400 uppercase tracking-widest px-1">Behavior</h3>
                        
                        <div className="bg-gray-800 border border-gray-700 rounded-2xl divide-y divide-gray-700/50 overflow-hidden">
                            <div className="p-4 flex items-center justify-between gap-4 transition-colors hover:bg-gray-750">
                                <div className="flex items-center gap-4">
                                    <RowIcon kind="install" />
                                    <div>
                                        <p className="text-[15px] font-medium text-white">Legacy Install Mode</p>
                                        <p className="text-[13px] text-gray-400 mt-0.5 leading-snug">Download starts immediately on install. Uses more disk space.</p>
                                    </div>
                                </div>
                                <Toggle value={legacyMode} onChange={setLegacyMode} />
                            </div>

                            <div className="p-4 flex items-center justify-between gap-4 transition-colors hover:bg-gray-750">
                                <div className="flex items-center gap-4">
                                    <RowIcon kind="version" />
                                    <div>
                                        <p className="text-[15px] font-medium text-white">Ask Version Before Installing</p>
                                        <p className="text-[13px] text-gray-400 mt-0.5 leading-snug">Open mod details first to select an exact version for install.</p>
                                    </div>
                                </div>
                                <Toggle value={askVersionBeforeInstall} onChange={setAskVersionBeforeInstall} />
                            </div>

                            <div className="p-4 flex items-center justify-between gap-4 transition-colors hover:bg-gray-750">
                                <div className="flex items-center gap-4">
                                    <RowIcon kind="parallel" />
                                    <div>
                                        <p className="text-[15px] font-medium text-white">Run Mod Operations in Parallel</p>
                                        <p className="text-[13px] text-gray-400 mt-0.5 leading-snug">Speeds up large operations and Apply to Game syncing.</p>
                                    </div>
                                </div>
                                <Toggle value={installInParallel} onChange={setInstallInParallel} />
                            </div>

                            <div className="p-4 flex items-center justify-between gap-4 transition-colors hover:bg-gray-750">
                                <div className="flex items-center gap-4">
                                    <RowIcon kind="apply" />
                                    <div>
                                        <p className="text-[15px] font-medium text-white">Confirm Before Apply to Game</p>
                                        <p className="text-[13px] text-gray-400 mt-0.5 leading-snug">Require confirmation before making structural game file changes.</p>
                                    </div>
                                </div>
                                <Toggle value={confirmBeforeApply} onChange={setConfirmBeforeApply} />
                            </div>

                            <div className="p-4 flex items-center justify-between gap-4 transition-colors hover:bg-gray-750">
                                <div className="flex items-center gap-4">
                                    <RowIcon kind="layout" />
                                    <div>
                                        <p className="text-[15px] font-medium text-white">Default Mods View</p>
                                        <p className="text-[13px] text-gray-400 mt-0.5 leading-snug">Choose the initial layout when browsing mods.</p>
                                    </div>
                                </div>
                                
                                {/* Grid/List Switcher — identical to Browse Mods header */}
                                <div className="relative flex bg-gray-800 rounded-lg p-1 border border-gray-700 overflow-hidden">
                                    {/* Sliding background pill */}
                                    <div
                                        className={`absolute top-1 bottom-1 w-[calc(50%-4px)] bg-gray-600 rounded-md transition-all duration-300 ease-[cubic-bezier(0.25,0.1,0.25,1)] ${defaultModViewMode === 'grid' ? 'left-1' : 'left-1/2'}`}
                                    />
                                    <button
                                        onClick={() => setDefaultModViewMode('grid')}
                                        className={`relative z-10 p-2 rounded w-10 flex items-center justify-center transition-colors ${defaultModViewMode === 'grid' ? 'text-white' : 'text-gray-400 hover:text-white'}`}
                                        title="Grid View"
                                    >
                                        <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2V6zM14 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2V6zM4 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2v-2zM14 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2v-2z" />
                                        </svg>
                                    </button>
                                    <button
                                        onClick={() => setDefaultModViewMode('list')}
                                        className={`relative z-10 p-2 rounded w-10 flex items-center justify-center transition-colors ${defaultModViewMode === 'list' ? 'text-white' : 'text-gray-400 hover:text-white'}`}
                                        title="List View"
                                    >
                                        <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6h16M4 12h16M4 18h16" />
                                        </svg>
                                    </button>
                                </div>
                            </div>
                        </div>
                    </div>

                    {/* Guides & Warnings Section */}
                    <div className="space-y-3">
                        <h3 className="text-xs font-semibold text-gray-400 uppercase tracking-widest px-1">Guides & Alerts</h3>
                        
                        <div className="bg-gray-800 border border-gray-700 rounded-2xl overflow-hidden">
                            <div className="p-4 flex flex-col sm:flex-row sm:items-center justify-between gap-4 transition-colors hover:bg-gray-750">
                                <div className="flex items-center gap-4">
                                    <RowIcon kind="warning" />
                                    <div>
                                        <p className="text-[15px] font-medium text-white">Restore Setup Warnings</p>
                                        <p className="text-[13px] text-gray-400 mt-0.5 leading-snug">
                                            Re-enable alerts previously hidden with "Don't show again".
                                        </p>
                                    </div>
                                </div>
                                <button
                                    disabled={!hasHiddenGuideWarnings || restoringWarnings}
                                    onClick={async () => {
                                        setRestoringWarnings(true);
                                        try {
                                            await onRestoreGuideWarnings();
                                        } finally {
                                            setRestoringWarnings(false);
                                        }
                                    }}
                                    className={`px-4 py-2 rounded-xl text-[13px] font-medium transition-all duration-200 shrink-0 ${hasHiddenGuideWarnings
                                        ? 'bg-amber-500/10 hover:bg-amber-500/20 text-amber-500 border border-amber-500/30'
                                        : 'bg-gray-900 text-gray-600 border border-gray-800 cursor-not-allowed'
                                        }`}
                                >
                                    {restoringWarnings ? 'Restoring...' : 'Show Again'}
                                </button>
                            </div>
                        </div>
                    </div>

                    {/* Danger Zone */}
                    <div className="space-y-3">
                        <div className="bg-red-500/5 hover:bg-red-500/10 border border-red-500/20 rounded-2xl overflow-hidden transition-colors duration-200 mt-2">
                            <div className="p-4 flex flex-col sm:flex-row sm:items-center justify-between gap-4">
                                <div className="flex items-center gap-4">
                                    <RowIcon kind="cache" />
                                    <div>
                                        <p className="text-[15px] font-medium text-white">Clear App Cache</p>
                                        <p className="text-[13px] text-gray-400 mt-0.5 leading-snug">
                                            Deletes caches. Requires app re-launch to apply.
                                        </p>
                                    </div>
                                </div>
                                <button
                                    onClick={async () => {
                                        const confirmed = await window.ipcRenderer.confirm(
                                            'Clear App Cache?',
                                            'This will delete profile cache and game detection cache. You may need to re-download mods and re-fetch game compatibility. Continue?'
                                        );
                                        if (confirmed) {
                                            const result = await window.ipcRenderer.clearProfileCache();
                                            localStorage.removeItem(MAC_PLATFORM_CACHE_KEY);
                                            localStorage.removeItem(MAC_IMAGE_CACHE_KEY);
                                            const sizeMB = (result.bytes_freed / 1024 / 1024).toFixed(1);
                                            const chunksInfo = result.chunks_cleared ? `\nThunderstore chunks cleared: ${result.chunks_cleared}` : '';
                                            await window.ipcRenderer.alert(
                                                'Cache Cleared',
                                                `Cleared ${result.cleared} profile cache(s), freed ${sizeMB} MB.${chunksInfo}\n\nGame cache cleared. Reloading app now...`
                                            );
                                            window.location.reload();
                                        }
                                    }}
                                    className="px-4 py-2 rounded-xl bg-red-500/10 hover:bg-red-500/20 text-red-500 hover:text-red-400 text-[13px] font-semibold border border-red-500/30 hover:border-red-500/50 transition-all duration-200 shrink-0 flex items-center justify-center gap-2"
                                >
                                    Clear Cache
                                </button>
                            </div>
                        </div>
                    </div>

                </div>

                {/* Footer Action Area */}
                <div className="flex items-center justify-end gap-3 px-7 py-5 bg-gray-900 border-t border-gray-800 shrink-0">
                    <Button 
                        variant="secondary" 
                        onClick={onClose}
                    >
                        Cancel
                    </Button>
                    <Button 
                        variant="primary" 
                        onClick={handleSave}
                    >
                        Save Changes
                    </Button>
                </div>
                
            </div>
        </div>
    );
}
