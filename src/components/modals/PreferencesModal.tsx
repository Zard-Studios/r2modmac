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

function Toggle({
    value,
    onChange,
}: {
    value: boolean;
    onChange: (next: boolean) => void;
}) {
    return (
        <button
            onClick={() => onChange(!value)}
            className={`relative w-12 h-6 rounded-full transition-colors flex-shrink-0 ${value ? 'bg-blue-600' : 'bg-gray-700'
                }`}
        >
            <span
                className={`absolute top-0.5 left-0.5 w-5 h-5 bg-white rounded-full shadow transition-transform ${value ? 'translate-x-6' : ''
                    }`}
            />
        </button>
    );
}

function RowIcon({ kind }: { kind: 'install' | 'version' | 'parallel' | 'apply' | 'layout' | 'warning' | 'cache' }) {
    if (kind === 'install') {
        return (
            <svg className="w-4 h-4 text-blue-300" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-5l-4 4m0 0l-4-4m4 4V4" />
            </svg>
        );
    }
    if (kind === 'version') {
        return (
            <svg className="w-4 h-4 text-cyan-300" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
            </svg>
        );
    }
    if (kind === 'parallel') {
        return (
            <svg className="w-4 h-4 text-violet-300" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 10V3L4 14h7v7l9-11h-7z" />
            </svg>
        );
    }
    if (kind === 'apply') {
        return (
            <svg className="w-4 h-4 text-emerald-300" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
            </svg>
        );
    }
    if (kind === 'layout') {
        return (
            <svg className="w-4 h-4 text-indigo-300" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6h7v12H4V6zm9 0h7v5h-7V6zm0 7h7v5h-7v-5z" />
            </svg>
        );
    }
    if (kind === 'warning') {
        return (
            <svg className="w-4 h-4 text-amber-300" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-7.938 4h15.876c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L2.33 16c-.77 1.333.192 3 1.732 3z" />
            </svg>
        );
    }
    return (
        <svg className="w-4 h-4 text-red-300" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
        </svg>
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
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm p-4">
            <div className="bg-gray-900 border border-gray-700 rounded-2xl shadow-2xl w-full max-w-2xl overflow-hidden">
                <div className="flex items-start justify-between p-6 border-b border-gray-800 bg-gray-900">
                    <div>
                        <h2 className="text-2xl font-bold text-white">Preferences</h2>
                    </div>
                    <button
                        onClick={onClose}
                        className="p-1.5 rounded-lg hover:bg-gray-800 text-gray-400 hover:text-white transition-colors"
                    >
                        <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                        </svg>
                    </button>
                </div>

                <div className="p-6 space-y-4 max-h-[72vh] overflow-y-auto">
                    <div className="rounded-xl border border-gray-700 overflow-hidden">
                        <div className="px-4 py-3 bg-gray-800/40 border-b border-gray-800">
                            <h3 className="text-sm font-semibold text-gray-200 uppercase tracking-wide">Behavior</h3>
                        </div>

                        <div className="divide-y divide-gray-800">
                            <div className="p-4 flex items-start justify-between gap-4">
                                <div className="flex gap-3">
                                    <div className="w-4 h-4 mt-0.5 flex items-center justify-center">
                                        <RowIcon kind="install" />
                                    </div>
                                    <div>
                                        <p className="text-sm text-white font-medium">Legacy Install Mode</p>
                                        <p className="text-xs text-gray-400 mt-1">Download starts immediately on Install. Uses more disk space.</p>
                                    </div>
                                </div>
                                <Toggle value={legacyMode} onChange={setLegacyMode} />
                            </div>

                            <div className="p-4 flex items-start justify-between gap-4">
                                <div className="flex gap-3">
                                    <div className="w-4 h-4 mt-0.5 flex items-center justify-center">
                                        <RowIcon kind="version" />
                                    </div>
                                    <div>
                                        <p className="text-sm text-white font-medium">Ask Version Before Installing</p>
                                        <p className="text-xs text-gray-400 mt-1">Install opens mod details first so you can select the exact version.</p>
                                    </div>
                                </div>
                                <Toggle value={askVersionBeforeInstall} onChange={setAskVersionBeforeInstall} />
                            </div>

                            <div className="p-4 flex items-start justify-between gap-4">
                                <div className="flex gap-3">
                                    <div className="w-4 h-4 mt-0.5 flex items-center justify-center">
                                        <RowIcon kind="parallel" />
                                    </div>
                                    <div>
                                        <p className="text-sm text-white font-medium">Install Dependencies in Parallel</p>
                                        <p className="text-xs text-gray-400 mt-1">Install dependency mods concurrently for faster total install time.</p>
                                    </div>
                                </div>
                                <Toggle value={installInParallel} onChange={setInstallInParallel} />
                            </div>

                            <div className="p-4 flex items-start justify-between gap-4">
                                <div className="flex gap-3">
                                    <div className="w-4 h-4 mt-0.5 flex items-center justify-center">
                                        <RowIcon kind="apply" />
                                    </div>
                                    <div>
                                        <p className="text-sm text-white font-medium">Confirm Before Apply to Game</p>
                                        <p className="text-xs text-gray-400 mt-1">Show a confirmation dialog before syncing profile mods to game files.</p>
                                    </div>
                                </div>
                                <Toggle value={confirmBeforeApply} onChange={setConfirmBeforeApply} />
                            </div>

                            <div className="p-4 flex items-start justify-between gap-4">
                                <div className="flex gap-3">
                                    <div className="w-4 h-4 mt-0.5 flex items-center justify-center">
                                        <RowIcon kind="layout" />
                                    </div>
                                    <div>
                                        <p className="text-sm text-white font-medium">Default Mods View</p>
                                        <p className="text-xs text-gray-400 mt-1">Choose the initial view in Browse Mods.</p>
                                    </div>
                                </div>
                                <div className="inline-flex bg-gray-800 rounded-lg p-1 border border-gray-700">
                                    <button
                                        onClick={() => setDefaultModViewMode('grid')}
                                        className={`px-3 py-1 text-xs rounded-md transition-colors ${defaultModViewMode === 'grid'
                                            ? 'bg-gray-600 text-white'
                                            : 'text-gray-400 hover:text-white'
                                            }`}
                                    >
                                        Grid
                                    </button>
                                    <button
                                        onClick={() => setDefaultModViewMode('list')}
                                        className={`px-3 py-1 text-xs rounded-md transition-colors ${defaultModViewMode === 'list'
                                            ? 'bg-gray-600 text-white'
                                            : 'text-gray-400 hover:text-white'
                                            }`}
                                    >
                                        List
                                    </button>
                                </div>
                            </div>
                        </div>
                    </div>

                    <div className="rounded-xl border border-gray-700 overflow-hidden">
                        <div className="px-4 py-3 bg-gray-800/40 border-b border-gray-800">
                            <h3 className="text-sm font-semibold text-gray-200 uppercase tracking-wide">Guides & Warnings</h3>
                        </div>
                        <div className="p-4 flex items-start justify-between gap-4">
                            <div className="flex gap-3">
                                <div className="w-4 h-4 mt-0.5 flex items-center justify-center">
                                    <RowIcon kind="warning" />
                                </div>
                                <div>
                                    <p className="text-sm text-white font-medium">Show Hidden Setup Warnings Again</p>
                                    <p className="text-xs text-gray-400 mt-1">
                                        Re-enable warnings previously hidden with the "Don't show again" checkbox.
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
                                className={`px-3 py-1.5 rounded-lg text-xs font-medium border transition-colors ${hasHiddenGuideWarnings
                                    ? 'bg-amber-500/10 hover:bg-amber-500/20 text-amber-300 border-amber-500/40'
                                    : 'bg-gray-800 text-gray-500 border-gray-700 cursor-not-allowed'
                                    }`}
                            >
                                {restoringWarnings ? 'Restoring...' : 'Show Again'}
                            </button>
                        </div>
                    </div>

                    <div className="rounded-xl border border-red-800/80 bg-red-900/15 p-4 flex items-start justify-between gap-4">
                        <div className="flex gap-3">
                            <div className="w-4 h-4 mt-0.5 flex items-center justify-center">
                                <RowIcon kind="cache" />
                            </div>
                            <div>
                                <p className="text-sm text-red-200 font-medium">Clear App Cache</p>
                                <p className="text-xs text-red-300/80 mt-1">
                                    Deletes profile cache and platform detection cache, then reloads the app.
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
                            className="px-3 py-1.5 rounded-lg bg-red-600/20 hover:bg-red-600/35 text-red-300 hover:text-red-200 text-xs font-medium border border-red-700/70 transition-colors flex-shrink-0 flex items-center gap-1.5"
                        >
                            <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                            </svg>
                            Clear Cache
                        </button>
                    </div>
                </div>

                <div className="flex justify-end gap-3 p-5 border-t border-gray-800 bg-gray-900">
                    <Button variant="secondary" onClick={onClose}>
                        Cancel
                    </Button>
                    <Button variant="primary" onClick={handleSave}>
                        Save
                    </Button>
                </div>
            </div>
        </div>
    );
}
