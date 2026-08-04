import { useState, useEffect } from 'react';
import { Button } from '../ui';
import type { SponsorMessage } from '../../types/electron';
import { HeartIcon } from '../LikeStat';

export interface PreferencesSettings {
    legacy_install_mode: boolean;
    ask_version_before_install: boolean;
    install_in_parallel: boolean;
    confirm_before_apply_to_game: boolean;
    write_debug_logs_to_game: boolean;
    default_mod_view_mode: 'grid' | 'list';
    show_deprecated_warnings: boolean;
    stream_mode: boolean;
    sponsored_messages_enabled: boolean;
}

interface PreferencesModalProps {
    isOpen: boolean;
    onClose: () => void;
    settings: PreferencesSettings;
    onSave: (settings: PreferencesSettings) => void;
    onSponsorPreferencesChange: (enabled: boolean) => Promise<void>;
    hasHiddenGuideWarnings: boolean;
    onRestoreGuideWarnings: () => Promise<void>;
    onCheckForUpdates: () => Promise<void>;
}

function Toggle({ value, onChange, label }: { value: boolean; onChange: (next: boolean) => void; label?: string }) {
    return (
        <button
            type="button"
            onClick={() => onChange(!value)}
            className={`relative w-11 h-6 rounded-full transition-colors duration-200 ease-in-out flex-shrink-0 focus:outline-none focus:ring-2 focus:ring-blue-500/50 ${value ? 'bg-blue-600' : 'bg-gray-700 hover:bg-gray-600'
                }`}
            aria-pressed={value}
            aria-label={label}
        >
            <span
                className={`absolute top-0.5 left-0.5 w-5 h-5 bg-white rounded-full shadow-[0_2px_5px_rgba(0,0,0,0.2)] transition-transform duration-200 ease-[cubic-bezier(0.4,0,0.2,1)] ${value ? 'translate-x-5' : 'translate-x-0'
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

function RowIcon({ kind }: { kind: 'install' | 'version' | 'parallel' | 'apply' | 'logs' | 'layout' | 'warning' | 'cache' | 'stream' | 'update' | 'support' }) {
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
    if (kind === 'logs') return (
        <IconBox colorClass="text-sky-400">
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M9 12h6m-6 4h6M8 4h8a2 2 0 012 2v12a2 2 0 01-2 2H8a2 2 0 01-2-2V6a2 2 0 012-2z" />
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
    if (kind === 'update') return (
        <IconBox colorClass="text-green-400">
            <svg className="w-5 h-5" fill="none" stroke="currentColor" strokeWidth={1.5} viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" d="M16.023 9.348h4.992v-.001M2.985 19.644v-4.992m0 0h4.992m-4.993 0l3.181 3.183a8.25 8.25 0 0013.803-3.7M4.031 9.865a8.25 8.25 0 0113.803-3.7l3.181 3.182m0-4.991v4.99" />
            </svg>
        </IconBox>
    );
    if (kind === 'stream') return (
        <IconBox colorClass="text-fuchsia-400">
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M5 18h8a2 2 0 002-2V8a2 2 0 00-2-2H5a2 2 0 00-2 2v8a2 2 0 002 2z" />
            </svg>
        </IconBox>
    );
    if (kind === 'support') return (
        <IconBox colorClass="text-rose-400">
            <HeartIcon className="h-5 w-5" />
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
    onSponsorPreferencesChange,
    hasHiddenGuideWarnings,
    onRestoreGuideWarnings,
    onCheckForUpdates,
}: PreferencesModalProps) {
    const [legacyMode, setLegacyMode] = useState(settings.legacy_install_mode);
    const [askVersionBeforeInstall, setAskVersionBeforeInstall] = useState(settings.ask_version_before_install);
    const [installInParallel, setInstallInParallel] = useState(settings.install_in_parallel);
    const [confirmBeforeApply, setConfirmBeforeApply] = useState(settings.confirm_before_apply_to_game);
    const [writeDebugLogsToGame, setWriteDebugLogsToGame] = useState(settings.write_debug_logs_to_game);
    const [defaultModViewMode, setDefaultModViewMode] = useState<'grid' | 'list'>(settings.default_mod_view_mode);
    const [showDeprecatedWarnings, setShowDeprecatedWarnings] = useState(settings.show_deprecated_warnings);
    const [streamMode, setStreamMode] = useState(settings.stream_mode);
    const [sponsoredMessagesEnabled, setSponsoredMessagesEnabled] = useState(settings.sponsored_messages_enabled);
    const [sponsorMessage, setSponsorMessage] = useState<SponsorMessage | null>(null);
    const [sponsorPreferenceRevision, setSponsorPreferenceRevision] = useState(0);
    const [restoringWarnings, setRestoringWarnings] = useState(false);
    const [checkingUpdates, setCheckingUpdates] = useState(false);

    // Track active state for a gentle reveal animation
    const [isVisible, setIsVisible] = useState(false);

    // Track props to reset state during rendering (React-recommended pattern)
    const [prevSettings, setPrevSettings] = useState(settings);
    const [prevIsOpen, setPrevIsOpen] = useState(isOpen);

    if (isOpen !== prevIsOpen || settings !== prevSettings) {
        setPrevIsOpen(isOpen);
        setPrevSettings(settings);
        if (isOpen) {
            setLegacyMode(settings.legacy_install_mode);
            setAskVersionBeforeInstall(settings.ask_version_before_install);
            setInstallInParallel(settings.install_in_parallel);
            setConfirmBeforeApply(settings.confirm_before_apply_to_game);
            setWriteDebugLogsToGame(settings.write_debug_logs_to_game);
            setDefaultModViewMode(settings.default_mod_view_mode);
            setShowDeprecatedWarnings(settings.show_deprecated_warnings);
            setStreamMode(settings.stream_mode);
            setSponsoredMessagesEnabled(settings.sponsored_messages_enabled);
            setSponsorMessage(null);
        } else {
            setIsVisible(false);
        }
    }

    useEffect(() => {
        if (isOpen) {
            const raf = requestAnimationFrame(() => {
                setIsVisible(true);
            });
            return () => cancelAnimationFrame(raf);
        }
    }, [isOpen]);

    useEffect(() => {
        if (!isOpen || !sponsoredMessagesEnabled) return;
        let cancelled = false;
        let timeoutId: number | undefined;

        const requestNextSponsor = async () => {
            try {
                const candidate = await window.ipcRenderer.requestSponsor();
                if (!cancelled && candidate) {
                    setSponsorMessage((current) => current?.id === candidate.id ? current : candidate);
                }
            } catch {
                // Sponsorship is optional; keep Preferences fully usable.
            } finally {
                if (!cancelled) timeoutId = window.setTimeout(requestNextSponsor, 15_000);
            }
        };

        void requestNextSponsor();
        return () => {
            cancelled = true;
            if (timeoutId !== undefined) window.clearTimeout(timeoutId);
        };
    }, [isOpen, sponsoredMessagesEnabled, sponsorPreferenceRevision]);

    useEffect(() => {
        if (!sponsorMessage) return;
        void window.ipcRenderer.acknowledgeSponsorDisplay(sponsorMessage.id).catch(() => undefined);
    }, [sponsorMessage]);

    if (!isOpen) return null;

    const handleSave = () => {
        onSave({
            legacy_install_mode: legacyMode,
            ask_version_before_install: askVersionBeforeInstall,
            install_in_parallel: installInParallel,
            confirm_before_apply_to_game: confirmBeforeApply,
            write_debug_logs_to_game: writeDebugLogsToGame,
            default_mod_view_mode: defaultModViewMode,
            show_deprecated_warnings: showDeprecatedWarnings,
            stream_mode: streamMode,
            sponsored_messages_enabled: sponsoredMessagesEnabled,
        });
        onClose();
    };

    const persistSponsorPreferences = (enabled: boolean) => {
        setSponsoredMessagesEnabled(enabled);
        if (!enabled) setSponsorMessage(null);
        window.dispatchEvent(new CustomEvent('r2modmac:sponsor-preferences', { detail: { enabled } }));
        void onSponsorPreferencesChange(enabled)
            .catch(() => undefined)
            .finally(() => setSponsorPreferenceRevision((revision) => revision + 1));
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

                    {/* Updates Section */}
                    <div className="space-y-3">
                        <h3 className="text-xs font-semibold text-gray-400 uppercase tracking-widest px-1">Updates</h3>

                        <div className="bg-gray-800 border border-gray-700 rounded-2xl overflow-hidden">
                            <div className="p-4 flex flex-col sm:flex-row sm:items-center justify-between gap-4 transition-colors hover:bg-gray-750">
                                <div className="flex items-center gap-4">
                                    <RowIcon kind="update" />
                                    <div>
                                        <p className="text-[15px] font-medium text-white">Check updates</p>
                                        <p className="text-[13px] text-gray-400 mt-0.5 leading-snug">
                                            Check for new versions of r2modmac.
                                        </p>
                                    </div>
                                </div>
                                <button
                                    disabled={checkingUpdates}
                                    onClick={async () => {
                                        setCheckingUpdates(true);
                                        try {
                                            await onCheckForUpdates();
                                        } finally {
                                            setCheckingUpdates(false);
                                        }
                                    }}
                                    className="px-4 py-2 rounded-xl text-[13px] font-medium transition-all duration-200 shrink-0 bg-green-600/10 hover:bg-green-600/20 active:bg-green-600/30 text-green-400 hover:text-green-300 active:text-green-200 border border-green-600/30 hover:border-green-600/40 active:border-green-600/50 active:scale-95 disabled:opacity-50 disabled:cursor-not-allowed"
                                >
                                    {checkingUpdates ? 'Checking...' : 'Check updates'}
                                </button>
                            </div>
                        </div>
                    </div>

                    {/* Setup behavior Section */}
                    <div className="space-y-3">
                        <h3 className="text-xs font-semibold text-gray-400 uppercase tracking-widest px-1">Behavior</h3>

                        <div className="bg-gray-800 border border-gray-700 rounded-2xl divide-y divide-gray-700/50 overflow-hidden">
                            <div className="p-4 flex items-center justify-between gap-4 transition-colors hover:bg-gray-750">
                                <div className="flex items-center gap-4">
                                    <RowIcon kind="install" />
                                    <div>
                                        <p className="text-[15px] font-medium text-white">Legacy install mode</p>
                                        <p className="text-[13px] text-gray-400 mt-0.5 leading-snug">Download starts immediately on install. Uses more disk space.</p>
                                    </div>
                                </div>
                                <Toggle value={legacyMode} onChange={setLegacyMode} />
                            </div>

                            <div className="p-4 flex items-center justify-between gap-4 transition-colors hover:bg-gray-750">
                                <div className="flex items-center gap-4">
                                    <RowIcon kind="version" />
                                    <div>
                                        <p className="text-[15px] font-medium text-white">Ask version before installing</p>
                                        <p className="text-[13px] text-gray-400 mt-0.5 leading-snug">Open mod details first to select an exact version for install.</p>
                                    </div>
                                </div>
                                <Toggle value={askVersionBeforeInstall} onChange={setAskVersionBeforeInstall} />
                            </div>

                            <div className="p-4 flex items-center justify-between gap-4 transition-colors hover:bg-gray-750">
                                <div className="flex items-center gap-4">
                                    <RowIcon kind="parallel" />
                                    <div>
                                        <p className="text-[15px] font-medium text-white">Download mods in parallel</p>
                                        <p className="text-[13px] text-gray-400 mt-0.5 leading-snug">Downloads mods in parallel for faster installation.</p>
                                    </div>
                                </div>
                                <Toggle value={installInParallel} onChange={setInstallInParallel} />
                            </div>

                            <div className="p-4 flex items-center justify-between gap-4 transition-colors hover:bg-gray-750">
                                <div className="flex items-center gap-4">
                                    <RowIcon kind="apply" />
                                    <div>
                                        <p className="text-[15px] font-medium text-white">Confirm before apply to game</p>
                                        <p className="text-[13px] text-gray-400 mt-0.5 leading-snug">Require confirmation before making structural game file changes.</p>
                                    </div>
                                </div>
                                <Toggle value={confirmBeforeApply} onChange={setConfirmBeforeApply} />
                            </div>

                            <div className="p-4 flex items-center justify-between gap-4 transition-colors hover:bg-gray-750">
                                <div className="flex items-center gap-4">
                                    <RowIcon kind="logs" />
                                    <div>
                                        <p className="text-[15px] font-medium text-white">Write debug logs to game folder</p>
                                        <p className="text-[13px] text-gray-400 mt-0.5 leading-snug">Writes r2modmac bootstrap, dyld, and exec logs when launching supported games from the app.</p>
                                    </div>
                                </div>
                                <Toggle value={writeDebugLogsToGame} onChange={setWriteDebugLogsToGame} />
                            </div>

                            <div className="p-4 flex items-center justify-between gap-4 transition-colors hover:bg-gray-750">
                                <div className="flex items-center gap-4">
                                    <RowIcon kind="layout" />
                                    <div>
                                        <p className="text-[15px] font-medium text-white">Default mods view</p>
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
                                        title="Grid view"
                                    >
                                        <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2V6zM14 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2V6zM4 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2v-2zM14 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2v-2z" />
                                        </svg>
                                    </button>
                                    <button
                                        onClick={() => setDefaultModViewMode('list')}
                                        className={`relative z-10 p-2 rounded w-10 flex items-center justify-center transition-colors ${defaultModViewMode === 'list' ? 'text-white' : 'text-gray-400 hover:text-white'}`}
                                        title="List view"
                                    >
                                        <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6h16M4 12h16M4 18h16" />
                                        </svg>
                                    </button>
                                </div>
                            </div>

                            <div className="p-4 flex items-center justify-between gap-4 transition-colors hover:bg-gray-750">
                                <div className="flex items-center gap-4">
                                    <RowIcon kind="stream" />
                                    <div>
                                        <p className="text-[15px] font-medium text-white">Stream Mode</p>
                                        <p className="text-[13px] text-gray-400 mt-0.5 leading-snug">Automatically censors usernames in file paths to protect your privacy during streaming or screen sharing.</p>
                                    </div>
                                </div>
                                <Toggle value={streamMode} onChange={setStreamMode} />
                            </div>
                        </div>
                    </div>

                    {/* Support Section */}
                    <div className="space-y-3">
                        <h3 className="text-xs font-semibold text-gray-400 uppercase tracking-widest px-1">Support r2modmac</h3>

                        <div className="divide-y divide-gray-700/50 overflow-hidden rounded-2xl border border-gray-700 bg-gray-800">
                            <div className="flex items-center justify-between gap-4 p-4 transition-colors hover:bg-gray-750">
                                <div className="flex items-center gap-4">
                                    <RowIcon kind="support" />
                                    <div>
                                        <p className="text-[15px] font-medium text-white">Support r2modmac with sponsored messages</p>
                                        <p className="mt-0.5 text-[13px] leading-snug text-gray-400">r2modmac may occasionally display short sponsored messages to help fund development. This feature is enabled by default, can be disabled at any time from Preferences, and never affects the application&apos;s functionality.</p>
                                    </div>
                                </div>
                                <Toggle value={sponsoredMessagesEnabled} onChange={persistSponsorPreferences} label="Enable sponsored messages" />
                            </div>
                            <div className="flex flex-col gap-3 p-4 sm:flex-row sm:items-center sm:justify-between">
                                <div className="flex items-center gap-4">
                                    <RowIcon kind="cache" />
                                    <div>
                                        <p className="text-[15px] font-medium text-white">Reset sponsor cache</p>
                                        <p className="mt-0.5 text-[13px] leading-snug text-gray-400">Forget recently shown and dismissed messages on this device.</p>
                                    </div>
                                </div>
                                <button
                                    type="button"
                                    onClick={async () => {
                                        await window.ipcRenderer.resetSponsorCache().catch(() => undefined);
                                        setSponsorMessage(null);
                                    }}
                                    className="shrink-0 rounded-xl border border-gray-600 px-4 py-2 text-[13px] font-medium text-gray-200 transition-colors hover:border-gray-500 hover:bg-gray-700"
                                >
                                    Reset
                                </button>
                            </div>
                        </div>

                        <p className="px-1 text-[13px] leading-snug text-gray-500">Sponsored messages are kept outside installs, updates, Sync, Apply, warnings, and dialogs.</p>
                        <button
                            type="button"
                            onClick={() => {
                                void import('@tauri-apps/plugin-shell').then(({ open }) => open('https://github.com/Zard-Studios/r2modmac/blob/main/docs/sponsored-messages.md'));
                            }}
                            className="px-1 text-[13px] font-medium text-blue-400 transition-colors hover:text-blue-300"
                        >
                            Learn more about sponsored messages ↗
                        </button>

                        {sponsorMessage ? (
                            <div className="flex items-start gap-3 rounded-xl border border-gray-700 bg-gray-800/70 px-4 py-3">
                                <span className="mt-0.5 shrink-0 text-[10px] font-semibold uppercase tracking-[0.14em] text-gray-500">Sponsored</span>
                                <div className="min-w-0 flex-1">
                                    {sponsorMessage.sponsorName ? (
                                        <p className="text-[13px] font-medium text-gray-200">{sponsorMessage.sponsorName}</p>
                                    ) : null}
                                    <p className={`${sponsorMessage.sponsorName ? 'mt-0.5' : ''} text-[13px] leading-snug text-gray-400`}>{sponsorMessage.message}</p>
                                    {sponsorMessage.url ? (
                                        <button
                                            type="button"
                                            onClick={() => {
                                                void import('@tauri-apps/plugin-shell').then(({ open }) => open(sponsorMessage.url!));
                                            }}
                                            className="mt-2 text-xs font-medium text-blue-400 transition-colors hover:text-blue-300"
                                        >
                                            Learn more ↗
                                        </button>
                                    ) : null}
                                </div>
                                <button
                                    type="button"
                                    onClick={() => {
                                        void window.ipcRenderer.dismissSponsor(sponsorMessage.id).catch(() => undefined);
                                        setSponsorMessage(null);
                                    }}
                                    className="-mr-1 -mt-1 flex h-7 w-7 shrink-0 items-center justify-center rounded-lg text-gray-500 transition-colors hover:bg-gray-700 hover:text-gray-200"
                                    aria-label="Dismiss sponsored message"
                                    title="Dismiss"
                                >
                                    <svg className="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
                                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="m6 6 12 12M18 6 6 18" />
                                    </svg>
                                </button>
                            </div>
                        ) : null}
                    </div>

                    {/* Guides & Warnings Section */}
                    <div className="space-y-3">
                        <h3 className="text-xs font-semibold text-gray-400 uppercase tracking-widest px-1">Guides & Alerts</h3>

                        <div className="divide-y divide-gray-700/50 overflow-hidden rounded-2xl border border-gray-700 bg-gray-800">
                            <div className="flex items-center justify-between gap-4 p-4 transition-colors hover:bg-gray-750">
                                <div className="flex items-center gap-4">
                                    <RowIcon kind="warning" />
                                    <div>
                                        <p className="text-[15px] font-medium text-white">Deprecated mod warnings</p>
                                        <p className="mt-0.5 text-[13px] leading-snug text-gray-400">Show a red warning on deprecated mod icons.</p>
                                    </div>
                                </div>
                                <Toggle value={showDeprecatedWarnings} onChange={setShowDeprecatedWarnings} label="Show deprecated mod warnings" />
                            </div>
                            <div className="p-4 flex flex-col sm:flex-row sm:items-center justify-between gap-4 transition-colors hover:bg-gray-750">
                                <div className="flex items-center gap-4">
                                    <RowIcon kind="warning" />
                                    <div>
                                        <p className="text-[15px] font-medium text-white">Restore setup warnings</p>
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
                                    {restoringWarnings ? 'Restoring...' : 'Show again'}
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
                                        <p className="text-[15px] font-medium text-white">Clear app cache</p>
                                        <p className="text-[13px] text-gray-400 mt-0.5 leading-snug">
                                            Deletes cache files. Requires the app to relaunch to apply.
                                        </p>
                                    </div>
                                </div>
                                <button
                                    onClick={async () => {
                                        const confirmed = await window.ipcRenderer.confirm(
                                            'Clear app cache?',
                                            settings.legacy_install_mode
                                                ? 'This will delete cached profile files used by legacy installs and the Thunderstore package cache used for browsing. It will not uninstall mods from your games. Continue?'
                                                : 'This will delete the Thunderstore package cache used for browsing and any leftover profile cache. It will not uninstall mods from your games. Continue?'
                                        );
                                        if (confirmed) {
                                            const result = await window.ipcRenderer.clearProfileCache();
                                            const hadBrowserCache = localStorage.length > 0;
                                            localStorage.clear();

                                            const bytesFreed = result.bytes_freed ?? 0;
                                            const clearedEntries = result.cleared + (result.chunks_cleared ?? 0);
                                            const sizeMB = (bytesFreed / 1024 / 1024).toFixed(1);

                                            if (clearedEntries === 0 && bytesFreed === 0 && !hadBrowserCache) {
                                                await window.ipcRenderer.alert(
                                                    'No cache found',
                                                    'There was no profile cache, Thunderstore cache, or browser cache to clear.'
                                                );
                                                return;
                                            }

                                            const summary: string[] = [];
                                            if (result.cleared > 0) {
                                                summary.push(`Removed ${result.cleared} profile cache folder(s).`);
                                            }
                                            if ((result.chunks_cleared ?? 0) > 0) {
                                                summary.push(`Removed ${result.chunks_cleared} Thunderstore chunk file(s).`);
                                            }
                                            if (bytesFreed > 0) {
                                                summary.push(`Freed ${sizeMB} MB.`);
                                            }
                                            if (hadBrowserCache) {
                                                summary.push('Cleared browser cache entries.');
                                            }
                                            summary.push('The app will reload now.');

                                            await window.ipcRenderer.alert(
                                                'Cache cleared',
                                                summary.join('\n')
                                            );
                                            window.location.reload();
                                        }
                                    }}
                                    className="px-4 py-2 rounded-xl bg-red-500/10 hover:bg-red-500/20 text-red-500 hover:text-red-400 text-[13px] font-semibold border border-red-500/30 hover:border-red-500/50 transition-all duration-200 shrink-0 flex items-center justify-center gap-2"
                                >
                                    Clear cache
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
                        Save changes
                    </Button>
                </div>

            </div>
        </div>
    );
}
