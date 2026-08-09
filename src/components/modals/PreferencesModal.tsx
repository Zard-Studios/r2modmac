import { useState, useEffect, useRef } from 'react';
import { Button } from '../ui';
import { Toggle } from '../ui/Toggle';
import { Slider } from '../ui/Slider';
import { AppIcon, type IconName } from '../ui/icons';
import { DefaultGamePickerModal } from './DefaultGamePickerModal';
import { ThemeEditorModal } from './ThemeEditorModal';
import { KeybindsModal } from './KeybindsModal';
import { UiPreviewLab } from './UiPreviewLab';
import { overridesFromKeybinds, resolveKeybinds, type KeybindMap } from '../../utils/keybinds';
import { useThemeStore } from '../../store/useThemeStore';
import type { Community, CommunityPlatformInfo } from '../../types/thunderstore';

const TROUBLESHOOTING_LOGS_EXPANDED_KEY = 'r2modmac:preferences:troubleshooting-logs-expanded';

function storedLogsExpanded(fallback: boolean): boolean {
    try {
        const stored = window.localStorage.getItem(TROUBLESHOOTING_LOGS_EXPANDED_KEY);
        if (stored === 'true') return true;
        if (stored === 'false') return false;
    } catch {
        // A restricted webview can reject storage. The panel still works for
        // the current mount; it simply falls back to its useful default.
    }
    return fallback;
}

function storeLogsExpanded(expanded: boolean): void {
    try {
        window.localStorage.setItem(TROUBLESHOOTING_LOGS_EXPANDED_KEY, String(expanded));
    } catch {
        // View state is non-critical and must never block Preferences.
    }
}

export interface PreferencesSettings {
    legacy_install_mode: boolean;
    ask_version_before_install: boolean;
    install_in_parallel: boolean;
    confirm_before_apply_to_game: boolean;
    write_debug_logs_to_game: boolean;
    verbose_logging: boolean;
    default_mod_view_mode: 'grid' | 'list';
    show_deprecated_warnings: boolean;
    stream_mode: boolean;
    sponsored_messages_enabled: boolean;
    sponsored_messages_scale: number;
    sponsored_messages_background_opacity: number;
    default_game: string | null;
    default_profile?: string | null;
    /** Only the shortcuts the user changed; see `src/utils/keybinds.ts`. */
    keybinds?: Record<string, string>;
}

export type PreferencesTarget =
    | 'theme'
    | 'keybinds'
    | 'updates'
    | 'default-game'
    | 'legacy-install'
    | 'ask-version'
    | 'parallel-downloads'
    | 'confirm-apply'
    | 'debug-logs'
    | 'verbose-logs'
    | 'open-logs'
    | 'default-view'
    | 'stream-mode'
    | 'sponsored-messages'
    | 'deprecated-warnings'
    | 'restore-warnings'
    | 'clear-cache';

interface PreferencesModalProps {
    isOpen: boolean;
    /**
     * Opened straight away on arrival, so a command can land on the panel it
     * names rather than on Preferences with the panel one click further on.
     */
    initialPanel?: PreferencesTarget | null;
    onClose: () => void;
    settings: PreferencesSettings;
    communities: Community[];
    communityImages: Record<string, string>;
    communityPlatforms: Record<string, CommunityPlatformInfo>;
    onSave: (settings: PreferencesSettings) => void;
    onSponsorPreferencesChange: (enabled: boolean) => Promise<void>;
    hasHiddenGuideWarnings: boolean;
    onRestoreGuideWarnings: () => Promise<void>;
    onCheckForUpdates: () => Promise<void>;
}

function IconBox({ children, colorClass }: { children: React.ReactNode; colorClass: string }) {
    return (
        <div className={`w-9 h-9 rounded-xl flex items-center justify-center bg-gray-700/60 border border-gray-700 flex-shrink-0 ${colorClass}`}>
            {children}
        </div>
    );
}

type PreferencesIconName = Extract<
    IconName,
    'install' | 'version' | 'parallel' | 'apply' | 'logs' | 'layout' | 'warning' |
    'cache' | 'stream' | 'update' | 'support' | 'folder' | 'game' | 'profile' |
    'theme' | 'keyboard'
>;

const ROW_ICON_COLORS: Record<PreferencesIconName, string> = {
    install: 'text-fg-accent',
    version: 'text-cyan-400',
    parallel: 'text-violet-400',
    apply: 'text-fg-success',
    logs: 'text-sky-400',
    layout: 'text-indigo-400',
    warning: 'text-fg-warning',
    cache: 'text-red-400',
    stream: 'text-fuchsia-400',
    update: 'text-fg-success',
    support: 'text-rose-400',
    folder: 'text-orange-400',
    game: 'text-teal-400',
    profile: 'text-purple-400',
    theme: 'text-pink-400',
    keyboard: 'text-amber-400',
};

function RowIcon({ kind }: { kind: PreferencesIconName }) {
    return (
        <IconBox colorClass={ROW_ICON_COLORS[kind]}>
            <AppIcon name={kind} className="h-5 w-5" strokeWidth={1.75} />
        </IconBox>
    );
}

export default function PreferencesModal({
    isOpen,
    initialPanel = null,
    onClose,
    settings,
    communities,
    communityImages,
    communityPlatforms,
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
    const [verboseLogging, setVerboseLogging] = useState(settings.verbose_logging);
    const [logsExpanded, setLogsExpanded] = useState(() => storedLogsExpanded(
        settings.write_debug_logs_to_game || settings.verbose_logging
    ));
    const [defaultModViewMode, setDefaultModViewMode] = useState<'grid' | 'list'>(settings.default_mod_view_mode);
    const [showDeprecatedWarnings, setShowDeprecatedWarnings] = useState(settings.show_deprecated_warnings);
    const [streamMode, setStreamMode] = useState(settings.stream_mode);
    const [sponsoredMessagesEnabled, setSponsoredMessagesEnabled] = useState(settings.sponsored_messages_enabled);
    const [sponsoredMessagesScale, setSponsoredMessagesScale] = useState(settings.sponsored_messages_scale ?? 80);
    const [sponsoredMessagesOpacity, setSponsoredMessagesOpacity] = useState(settings.sponsored_messages_background_opacity ?? 80);
    const [defaultGame, setDefaultGame] = useState<string | null>(settings.default_game ?? null);
    const [defaultProfile, setDefaultProfile] = useState<string | null>(settings.default_profile ?? null);
    const [showGamePicker, setShowGamePicker] = useState(false);
    const [showThemeEditor, setShowThemeEditor] = useState(false);
    const [showKeybinds, setShowKeybinds] = useState(false);
    const [showUiPreviewLab, setShowUiPreviewLab] = useState(false);
    const supportHeartClicks = useRef<number[]>([]);
    const [keybinds, setKeybinds] = useState<KeybindMap>(() => resolveKeybinds(settings.keybinds));
    const themes = useThemeStore((s) => s.themes);
    const activeThemeFileName = useThemeStore((s) => s.activeFileName);
    const activeThemeName =
        themes.find((t) => t.file_name === activeThemeFileName)?.name ?? 'Default';
    const [pickerInitialStep, setPickerInitialStep] = useState<'game' | 'profile'>('game');
    const defaultGameName = communities.find(c => c.identifier === defaultGame)?.name ?? null;
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
            setWriteDebugLogsToGame(settings.write_debug_logs_to_game ?? false);
            setVerboseLogging(settings.verbose_logging ?? false);
            const targetsLogs = initialPanel === 'debug-logs'
                || initialPanel === 'verbose-logs'
                || initialPanel === 'open-logs';
            setLogsExpanded(targetsLogs || storedLogsExpanded(
                settings.write_debug_logs_to_game || settings.verbose_logging
            ));
            setShowDeprecatedWarnings(settings.show_deprecated_warnings);
            setStreamMode(settings.stream_mode ?? false);
            setDefaultModViewMode(settings.default_mod_view_mode ?? 'grid');
            setSponsoredMessagesEnabled(settings.sponsored_messages_enabled);
            setSponsoredMessagesScale(settings.sponsored_messages_scale ?? 80);
            setSponsoredMessagesOpacity(settings.sponsored_messages_background_opacity ?? 80);
            setDefaultGame(settings.default_game ?? null);
            setDefaultProfile(settings.default_profile ?? null);
            setKeybinds(resolveKeybinds(settings.keybinds));
            setShowThemeEditor(initialPanel === 'theme');
            setShowKeybinds(initialPanel === 'keybinds');
        } else {
            setIsVisible(false);
        }
    }

    // Handle smooth backdrop animation
    useEffect(() => {
        if (isOpen) {
            const raf = requestAnimationFrame(() => {
                setIsVisible(true);
            });
            return () => cancelAnimationFrame(raf);
        }
    }, [isOpen]);

    useEffect(() => {
        if (!isOpen || !initialPanel || initialPanel === 'theme' || initialPanel === 'keybinds') return;
        const frame = requestAnimationFrame(() => {
            document.getElementById(`preference-${initialPanel}`)?.scrollIntoView({ block: 'center' });
        });
        return () => cancelAnimationFrame(frame);
    }, [initialPanel, isOpen]);

    useEffect(() => {
        if (!isOpen) return;
        let cancelled = false;
        let timeoutId: number | undefined;

        const requestNextSponsor = async () => {
            try {
                await window.ipcRenderer.requestSponsor('preferences-support');
            } catch {
                void 0;
            } finally {
                if (!cancelled) timeoutId = window.setTimeout(requestNextSponsor, 15_000);
            }
        };

        void requestNextSponsor();
        return () => {
            cancelled = true;
            if (timeoutId !== undefined) window.clearTimeout(timeoutId);
        };
    }, [isOpen]);

    if (!isOpen) return null;

    const currentSettings = (currentKeybinds: KeybindMap = keybinds): PreferencesSettings => ({
            legacy_install_mode: legacyMode,
            ask_version_before_install: askVersionBeforeInstall,
            install_in_parallel: installInParallel,
            confirm_before_apply_to_game: confirmBeforeApply,
            write_debug_logs_to_game: writeDebugLogsToGame,
            verbose_logging: verboseLogging,
            default_mod_view_mode: defaultModViewMode,
            show_deprecated_warnings: showDeprecatedWarnings,
            stream_mode: streamMode,
            sponsored_messages_enabled: sponsoredMessagesEnabled,
            sponsored_messages_scale: sponsoredMessagesScale,
            sponsored_messages_background_opacity: sponsoredMessagesOpacity,
            default_game: defaultGame,
            default_profile: defaultProfile,
            keybinds: overridesFromKeybinds(currentKeybinds),
    });

    const handleSave = () => {
        onSave(currentSettings());
        onClose();
    };

    const handleKeybindsClose = () => {
        onSave(currentSettings());
        setShowKeybinds(false);
        if (initialPanel === 'keybinds') onClose();
    };

    const persistSponsorPreferences = (enabled: boolean) => {
        setSponsoredMessagesEnabled(enabled);
        window.dispatchEvent(new CustomEvent('r2modmac:sponsor-preferences', { detail: { enabled, scale: sponsoredMessagesScale, opacity: sponsoredMessagesOpacity } }));
        void onSponsorPreferencesChange(enabled).catch(() => undefined);
    };

    const handleSupportHeartClick = () => {
        const now = Date.now();
        const recentClicks = [...supportHeartClicks.current.filter((time) => now - time < 1_800), now];
        if (recentClicks.length >= 5) {
            supportHeartClicks.current = [];
            setShowUiPreviewLab(true);
            return;
        }
        supportHeartClicks.current = recentClicks;
    };

    const logsActive = writeDebugLogsToGame || verboseLogging;
    const logsOpen = logsExpanded;
    const logsStatus = writeDebugLogsToGame && verboseLogging
        ? '2 active'
        : writeDebugLogsToGame || verboseLogging
            ? '1 active'
            : 'Off';

    const changeLogsExpanded = (expanded: boolean) => {
        setLogsExpanded(expanded);
        storeLogsExpanded(expanded);
    };

    const changeDebugLogs = (enabled: boolean) => {
        setWriteDebugLogsToGame(enabled);
        if (enabled) changeLogsExpanded(true);
        else if (!verboseLogging) changeLogsExpanded(false);
    };

    const changeVerboseLogging = (enabled: boolean) => {
        setVerboseLogging(enabled);
        if (enabled) changeLogsExpanded(true);
        else if (!writeDebugLogsToGame) changeLogsExpanded(false);
    };

    // Spotlight destinations are panels in their own right. Mounting the full
    // Preferences modal behind them creates two backdrops and, when settings
    // are persisted, can immediately reopen the child that was just closed.
    // A direct Spotlight launch therefore owns the whole modal layer.
    if (initialPanel === 'keybinds') {
        return (
            <KeybindsModal
                isOpen={isOpen}
                keybinds={keybinds}
                onChange={setKeybinds}
                onClose={handleKeybindsClose}
            />
        );
    }

    if (initialPanel === 'theme') {
        return <ThemeEditorModal isOpen={isOpen} onClose={onClose} />;
    }

    return (
        <>
        <div className={`fixed inset-0 z-50 flex items-center justify-center p-4 transition-all duration-300 ease-[cubic-bezier(0.4,0,0.2,1)] ${isVisible ? 'opacity-100 backdrop-blur-sm' : 'opacity-0 backdrop-blur-none'}`}>
            {/* Backdrop */}
            <div className="absolute inset-0 bg-black/60 transition-opacity" onClick={onClose} />

            {/* Modal Container */}
            <div className={`relative w-full max-w-[760px] max-h-[85vh] flex flex-col bg-gray-900 border border-gray-700 rounded-2xl shadow-2xl overflow-hidden transform transition-all duration-300 ease-[cubic-bezier(0.34,1.56,0.64,1)] ${isVisible ? 'scale-100 translate-y-0 opacity-100' : 'scale-95 translate-y-4 opacity-0'}`}>

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
                    <div id="preference-updates" className="space-y-3">
                        <h3 className="text-xs font-semibold text-gray-400 uppercase tracking-widest px-1">Updates</h3>

                        <div className="bg-gray-800 border border-gray-700 rounded-2xl overflow-hidden">
                            <div className="p-4 flex flex-col sm:flex-row sm:items-center justify-between gap-4 transition-colors hover:bg-surface-hover">
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
                                    className="px-4 py-2 rounded-xl text-[13px] font-medium transition-all duration-200 shrink-0 bg-green-600/10 hover:bg-green-600/20 active:bg-green-600/30 text-fg-success hover:text-fg-success active:text-fg-success border border-green-600/30 hover:border-green-600/40 active:border-green-600/50 active:scale-95 disabled:opacity-50 disabled:cursor-not-allowed"
                                >
                                    {checkingUpdates ? 'Checking...' : 'Check updates'}
                                </button>
                            </div>
                        </div>
                    </div>

                    {/* Appearance Section */}
                    <div className="space-y-3">
                        <h3 className="text-xs font-semibold text-gray-400 uppercase tracking-widest px-1">Appearance</h3>

                        <div className="bg-gray-800 border border-gray-700 rounded-2xl divide-y divide-gray-700/50 overflow-hidden">
                            <div id="preference-theme" className="p-4 flex items-center justify-between gap-4 transition-colors hover:bg-surface-hover">
                                <div className="flex items-center gap-4">
                                    <RowIcon kind="theme" />
                                    <div>
                                        <p className="text-[15px] font-medium text-white">Theme</p>
                                        <p className="text-[13px] text-gray-400 mt-0.5 leading-snug">Customise the background, surfaces, text and accent colours.</p>
                                    </div>
                                </div>
                                <div className="flex shrink-0 items-center gap-3">
                                    <span className="text-[13px] text-gray-400 max-w-[200px] truncate text-right">
                                        {activeThemeName}
                                    </span>
                                    <button
                                        type="button"
                                        onClick={() => setShowThemeEditor(true)}
                                        className="rounded-lg border border-gray-600 px-4 py-2 text-[13px] font-medium text-gray-200 transition-colors hover:border-gray-500 hover:bg-gray-700"
                                    >
                                        Customise
                                    </button>
                                </div>
                            </div>
                        </div>
                    </div>

                    {/* Keyboard Section */}
                    <div className="space-y-3">
                        <h3 className="text-xs font-semibold text-gray-400 uppercase tracking-widest px-1">Keyboard</h3>

                        <div className="bg-gray-800 border border-gray-700 rounded-2xl overflow-hidden">
                            <div id="preference-keybinds" className="p-4 flex items-center justify-between gap-4 transition-colors hover:bg-surface-hover">
                                <div className="flex items-center gap-4">
                                    <RowIcon kind="keyboard" />
                                    <div>
                                        <p className="text-[15px] font-medium text-white">Keyboard shortcuts</p>
                                        <p className="text-[13px] text-gray-400 mt-0.5 leading-snug">
                                            Launch, apply and switch profiles without reaching for the mouse.
                                        </p>
                                    </div>
                                </div>
                                <div className="flex shrink-0 items-center">
                                    <button
                                        type="button"
                                        onClick={() => setShowKeybinds(true)}
                                        className="rounded-lg border border-gray-600 px-4 py-2 text-[13px] font-medium text-gray-200 transition-colors hover:border-gray-500 hover:bg-gray-700"
                                    >
                                        Customise
                                    </button>
                                </div>
                            </div>
                        </div>
                    </div>

                    {/* Setup behavior Section */}
                    <div className="space-y-3">
                        <h3 className="text-xs font-semibold text-gray-400 uppercase tracking-widest px-1">Behavior</h3>

                        <div className="bg-gray-800 border border-gray-700 rounded-2xl divide-y divide-gray-700/50 overflow-hidden">
                            <div id="preference-default-game" className="p-4 flex items-center justify-between gap-4 transition-colors hover:bg-surface-hover">
                                <div className="flex items-center gap-4">
                                    <RowIcon kind="game" />
                                    <div>
                                        <p className="text-[15px] font-medium text-white">Default game</p>
                                        <p className="text-[13px] text-gray-400 mt-0.5 leading-snug">Skip game selection on startup and go straight to this game's profiles.</p>
                                    </div>
                                </div>
                                <div className="flex shrink-0 items-center gap-3">
                                    <span className="text-[13px] text-gray-400 max-w-[200px] truncate text-right">
                                        {defaultGameName ?? 'Not set'}
                                    </span>
                                    <button
                                        type="button"
                                        onClick={() => {
                                            setPickerInitialStep('game');
                                            setShowGamePicker(true);
                                        }}
                                        className="rounded-lg border border-gray-600 px-4 py-2 text-[13px] font-medium text-gray-200 transition-colors hover:border-gray-500 hover:bg-gray-700"
                                    >
                                        {defaultGame ? 'Change game' : 'Choose game'}
                                    </button>
                                </div>
                            </div>

                            {defaultGame && (
                                <div className="p-4 flex items-center justify-between gap-4 transition-colors hover:bg-surface-hover animate-[profile-update-action-enter_220ms_cubic-bezier(0.22,1,0.36,1)]">
                                    <div className="flex items-center gap-4">
                                        <RowIcon kind="profile" />
                                        <div>
                                            <p className="text-[15px] font-medium text-white">Default profile</p>
                                            <p className="text-[13px] text-gray-400 mt-0.5 leading-snug">
                                                Skip profile selection on startup and launch straight into this profile for {defaultGameName}.
                                            </p>
                                        </div>
                                    </div>
                                    <div className="flex shrink-0 items-center gap-3">
                                        <span className="text-[13px] text-gray-400 max-w-[200px] truncate text-right">
                                            {defaultProfile ?? 'Not set'}
                                        </span>
                                        <button
                                            type="button"
                                            onClick={() => {
                                                setPickerInitialStep('profile');
                                                setShowGamePicker(true);
                                            }}
                                            className="rounded-lg border border-gray-600 px-4 py-2 text-[13px] font-medium text-gray-200 transition-colors hover:border-gray-500 hover:bg-gray-700"
                                        >
                                            {defaultProfile ? 'Change profile' : 'Choose profile'}
                                        </button>
                                    </div>
                                </div>
                            )}

                            <div id="preference-legacy-install" className="p-4 flex items-center justify-between gap-4 transition-colors hover:bg-surface-hover">
                                <div className="flex items-center gap-4">
                                    <RowIcon kind="install" />
                                    <div>
                                        <p className="text-[15px] font-medium text-white">Legacy install mode</p>
                                        <p className="text-[13px] text-gray-400 mt-0.5 leading-snug">Download starts immediately on install. Uses more disk space.</p>
                                    </div>
                                </div>
                                <Toggle value={legacyMode} onChange={setLegacyMode} />
                            </div>

                            <div id="preference-ask-version" className="p-4 flex items-center justify-between gap-4 transition-colors hover:bg-surface-hover">
                                <div className="flex items-center gap-4">
                                    <RowIcon kind="version" />
                                    <div>
                                        <p className="text-[15px] font-medium text-white">Ask version before installing</p>
                                        <p className="text-[13px] text-gray-400 mt-0.5 leading-snug">Open mod details first to select an exact version for install.</p>
                                    </div>
                                </div>
                                <Toggle value={askVersionBeforeInstall} onChange={setAskVersionBeforeInstall} />
                            </div>

                            <div id="preference-parallel-downloads" className="p-4 flex items-center justify-between gap-4 transition-colors hover:bg-surface-hover">
                                <div className="flex items-center gap-4">
                                    <RowIcon kind="parallel" />
                                    <div>
                                        <p className="text-[15px] font-medium text-white">Download mods in parallel</p>
                                        <p className="text-[13px] text-gray-400 mt-0.5 leading-snug">Downloads mods in parallel for faster installation.</p>
                                    </div>
                                </div>
                                <Toggle value={installInParallel} onChange={setInstallInParallel} />
                            </div>

                            <div id="preference-confirm-apply" className="p-4 flex items-center justify-between gap-4 transition-colors hover:bg-surface-hover">
                                <div className="flex items-center gap-4">
                                    <RowIcon kind="apply" />
                                    <div>
                                        <p className="text-[15px] font-medium text-white">Confirm before apply to game</p>
                                        <p className="text-[13px] text-gray-400 mt-0.5 leading-snug">Require confirmation before making structural game file changes.</p>
                                    </div>
                                </div>
                                <Toggle value={confirmBeforeApply} onChange={setConfirmBeforeApply} />
                            </div>

                            <div id="preference-debug-logs">
                                <button
                                    type="button"
                                    aria-expanded={logsOpen}
                                    aria-controls="preference-log-details"
                                    onClick={() => changeLogsExpanded(!logsExpanded)}
                                    className="flex w-full items-center justify-between gap-4 p-4 text-left transition-colors hover:bg-surface-hover"
                                >
                                    <div className="flex min-w-0 items-center gap-4">
                                        <RowIcon kind="logs" />
                                        <div className="min-w-0">
                                            <p className="text-[15px] font-medium text-white">Troubleshooting logs</p>
                                            <p className="mt-0.5 text-[13px] leading-snug text-gray-400">
                                                {logsActive ? 'Diagnostic logging is enabled.' : 'Enable additional logging when diagnosing a problem.'}
                                            </p>
                                        </div>
                                    </div>
                                    <span className="flex shrink-0 items-center gap-3">
                                        <span className={`text-xs font-medium ${logsActive ? 'text-fg-warning' : 'text-gray-400'}`}>{logsStatus}</span>
                                        <svg className={`h-4 w-4 text-gray-400 transition-transform duration-200 ${logsOpen ? 'rotate-180' : ''}`} fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
                                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="m6 9 6 6 6-6" />
                                        </svg>
                                    </span>
                                </button>
                                <div
                                    id="preference-log-details"
                                    className={`grid transition-[grid-template-rows,opacity] duration-300 ease-[cubic-bezier(0.4,0,0.2,1)] ${logsOpen ? 'grid-rows-[1fr] opacity-100' : 'grid-rows-[0fr] opacity-0'}`}
                                    aria-hidden={!logsOpen}
                                    inert={!logsOpen}
                                >
                                    <div className="min-h-0 overflow-hidden">
                                        <div className="divide-y divide-gray-700/50 border-t border-gray-700/50">
                                            <div className="flex items-center justify-between gap-4 p-4 pl-8 transition-colors hover:bg-surface-hover">
                                                <div className="flex items-center gap-4">
                                                    <RowIcon kind="logs" />
                                                    <div>
                                                        <p className="text-[15px] font-medium text-white">Write debug logs to game folder</p>
                                                        <p className="mt-0.5 text-[13px] leading-snug text-gray-400">Writes bootstrap, dyld, and exec logs when launching supported games.</p>
                                                    </div>
                                                </div>
                                                <Toggle value={writeDebugLogsToGame} onChange={changeDebugLogs} />
                                            </div>

                                            <div id="preference-verbose-logs" className="flex items-center justify-between gap-4 p-4 pl-8 transition-colors hover:bg-surface-hover">
                                                <div className="flex items-center gap-4">
                                                    <RowIcon kind="logs" />
                                                    <div>
                                                        <p className="text-[15px] font-medium text-white">Verbose app logging</p>
                                                        <p className="mt-0.5 text-[13px] leading-snug text-gray-400">Records detailed per-mod and per-file tracing while reproducing a bug.</p>
                                                    </div>
                                                </div>
                                                <Toggle value={verboseLogging} onChange={changeVerboseLogging} />
                                            </div>

                                            <div id="preference-open-logs" className="flex items-center justify-between gap-4 p-4 pl-8 transition-colors hover:bg-surface-hover">
                                                <div className="flex items-center gap-4">
                                                    <RowIcon kind="folder" />
                                                    <div>
                                                        <p className="text-[15px] font-medium text-white">Open app logs folder</p>
                                                        <p className="mt-0.5 text-[13px] leading-snug text-gray-400">Open launch, Steam, and CrossOver/Wine diagnostics.</p>
                                                    </div>
                                                </div>
                                                <button
                                                    onClick={() => { void window.ipcRenderer.openAppLogsFolder(); }}
                                                    className="shrink-0 whitespace-nowrap rounded-lg bg-gray-700 px-4 py-2 text-sm font-medium text-white transition-all hover:bg-gray-600 active:scale-95"
                                                >
                                                    Open
                                                </button>
                                            </div>
                                        </div>
                                    </div>
                                </div>
                            </div>

                            <div id="preference-default-view" className="p-4 flex items-center justify-between gap-4 transition-colors hover:bg-surface-hover">
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

                            <div id="preference-stream-mode" className="p-4 flex items-center justify-between gap-4 transition-colors hover:bg-surface-hover">
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
                            <div id="preference-sponsored-messages" className="flex items-center justify-between gap-4 p-4 transition-colors hover:bg-surface-hover">
                                <div className="flex items-center gap-4">
                                    <span onClick={handleSupportHeartClick} className="shrink-0 select-none">
                                        <RowIcon kind="support" />
                                    </span>
                                    <div>
                                        <p className="text-[15px] font-medium text-white">Support r2modmac with sponsored messages</p>
                                        <p className="mt-0.5 text-[13px] leading-snug text-gray-400">Occasional short text messages help fund development. Enabled by default, they can be disabled at any time and never affect the application&apos;s functionality.</p>
                                    </div>
                                </div>
                                <Toggle value={sponsoredMessagesEnabled} onChange={persistSponsorPreferences} label="Enable sponsored messages" />
                            </div>
                            <div className={`grid transition-[grid-template-rows,opacity] duration-300 ease-[cubic-bezier(0.4,0,0.2,1)] ${sponsoredMessagesEnabled ? 'grid-rows-[1fr] opacity-100' : 'grid-rows-[0fr] opacity-0'}`} aria-hidden={!sponsoredMessagesEnabled}>
                                <div className="min-h-0 overflow-hidden">
                                    <div className="space-y-4 border-t border-gray-700/50 p-4">
                                        <label className="block text-[13px] text-gray-400">
                                            <span className="mb-1 flex items-center justify-between"><span>Ad size</span><span className="tabular-nums text-gray-300">{sponsoredMessagesScale}%</span></span>
                                            <Slider
                                                ariaLabel="Sponsored message size"
                                                value={sponsoredMessagesScale}
                                                min={70} max={100} step={1}
                                                onChange={(value) => {
                                                    setSponsoredMessagesScale(value);
                                                    window.dispatchEvent(new CustomEvent('r2modmac:sponsor-preferences', { detail: { enabled: sponsoredMessagesEnabled, scale: value, opacity: sponsoredMessagesOpacity } }));
                                                }}
                                            />
                                        </label>
                                        <label className="block text-[13px] text-gray-400">
                                            <span className="mb-1 flex items-center justify-between"><span>Background opacity</span><span className="tabular-nums text-gray-300">{sponsoredMessagesOpacity}%</span></span>
                                            <Slider
                                                ariaLabel="Sponsored message background opacity"
                                                value={sponsoredMessagesOpacity}
                                                min={0} max={100} step={1}
                                                onChange={(value) => {
                                                    setSponsoredMessagesOpacity(value);
                                                    window.dispatchEvent(new CustomEvent('r2modmac:sponsor-preferences', { detail: { enabled: sponsoredMessagesEnabled, scale: sponsoredMessagesScale, opacity: value } }));
                                                }}
                                            />
                                        </label>
                                    </div>
                                </div>
                            </div>
                        </div>

                        <p className="px-1 text-[13px] leading-snug text-gray-500">Text-only messages: no images or banners. They never interrupt installs, updates, Sync, Apply, warnings, dialogs, or your workflow.</p>
                        <button
                            type="button"
                            onClick={() => {
                                void import('@tauri-apps/plugin-shell').then(({ open }) => open('https://github.com/Zard-Studios/r2modmac/blob/main/docs/sponsored-messages.md'));
                            }}
                            className="px-1 text-[13px] font-medium text-fg-accent transition-colors hover:text-fg-accent"
                        >
                            Learn more about sponsored messages ↗
                        </button>
                    </div>

                    {/* Guides & Warnings Section */}
                    <div className="space-y-3">
                        <h3 className="text-xs font-semibold text-gray-400 uppercase tracking-widest px-1">Guides & Alerts</h3>

                        <div className="divide-y divide-gray-700/50 overflow-hidden rounded-2xl border border-gray-700 bg-gray-800">
                            <div id="preference-deprecated-warnings" className="flex items-center justify-between gap-4 p-4 transition-colors hover:bg-surface-hover">
                                <div className="flex items-center gap-4">
                                    <RowIcon kind="warning" />
                                    <div>
                                        <p className="text-[15px] font-medium text-white">Deprecated mod warnings</p>
                                        <p className="mt-0.5 text-[13px] leading-snug text-gray-400">Show a red warning on deprecated mod icons.</p>
                                    </div>
                                </div>
                                <Toggle value={showDeprecatedWarnings} onChange={setShowDeprecatedWarnings} label="Show deprecated mod warnings" />
                            </div>
                            <div id="preference-restore-warnings" className="p-4 flex flex-col sm:flex-row sm:items-center justify-between gap-4 transition-colors hover:bg-surface-hover">
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
                                    className={`px-4 py-2 rounded-xl text-[13px] font-medium transition-all duration-200 ease-in-out flex-shrink-0 ${hasHiddenGuideWarnings
                                        ? 'bg-amber-500/10 hover:bg-amber-500/20 text-fg-warning border border-amber-500/30'
                                        : 'bg-gray-700/40 text-gray-400 border border-gray-700 cursor-not-allowed opacity-50'
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
                            <div id="preference-clear-cache" className="p-4 flex flex-col sm:flex-row sm:items-center justify-between gap-4">
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
                                    className="px-4 py-2 rounded-xl bg-red-500/10 hover:bg-red-500/20 text-red-500 hover:text-fg-danger text-[13px] font-semibold border border-red-500/30 hover:border-red-500/50 transition-all duration-200 shrink-0 flex items-center justify-center gap-2"
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
        <DefaultGamePickerModal
            isOpen={showGamePicker}
            onClose={() => setShowGamePicker(false)}
            communities={communities}
            communityImages={communityImages}
            communityPlatforms={communityPlatforms}
            currentValue={defaultGame}
            initialStep={pickerInitialStep}
            onPick={(gameId, profileName) => {
                setDefaultGame(gameId);
                setDefaultProfile(profileName ?? null);
            }}
        />
        <KeybindsModal
            isOpen={showKeybinds}
            keybinds={keybinds}
            onChange={setKeybinds}
            onClose={handleKeybindsClose}
        />

        <ThemeEditorModal
            isOpen={showThemeEditor}
            onClose={() => setShowThemeEditor(false)}
        />
        <UiPreviewLab
            isOpen={showUiPreviewLab}
            onClose={() => setShowUiPreviewLab(false)}
        />
        </>
    );
}
