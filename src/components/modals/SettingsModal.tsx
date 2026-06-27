import { useState, useEffect, useRef } from 'react';
import { Button } from '../ui';
import type { Profile, ProfilePlatform } from '../../types/profile';
import { ConfigEditorTab } from './ConfigEditorTab';
import { PathCensor, CensoredInput } from '../ui/PathCensor';
import { openFolderLabel } from '../../utils/platformUtils';


type SettingsTab = 'settings' | 'config-editor';

interface SettingsModalProps {
    isOpen: boolean;
    onClose: () => void;
    selectedGame?: string;
    activeProfile?: Profile | null;
}

export function SettingsModal({ isOpen, onClose, selectedGame, activeProfile }: SettingsModalProps) {
    const [activeTab, setActiveTab] = useState<SettingsTab>('settings');
    const [isAnimating, setIsAnimating] = useState(false);
    const [steamPath, setSteamPath] = useState<string>('');
    const [loading, setLoading] = useState(false);
    const [gamePath, setGamePath] = useState<string | null>(null);
    const [checkingGamePath, setCheckingGamePath] = useState(false);
    const [gameSource, setGameSource] = useState<'steam' | 'manual' | 'unknown'>('unknown');
    
    const contentWrapperRef = useRef<HTMLDivElement>(null);
    const settingsRef = useRef<HTMLDivElement>(null);
    const configEditorRef = useRef<HTMLDivElement>(null);
    const [tabHeight, setTabHeight] = useState<number | null>(null);
    const [enableHeightTransition, setEnableHeightTransition] = useState(false);


    const activeProfilePlatform: ProfilePlatform = activeProfile?.platform === 'mac' ? 'mac' : 'windows';
    const defaultMacSteamPath = '~/Library/Application Support/Steam';
    const getLegacyMacSteamPath = (legacyPath?: string | null) => {
        if (!legacyPath) return null;
        const lower = legacyPath.toLowerCase();
        if (lower.includes('drive_c') || lower.includes('crossover') || lower.includes('wine')) {
            return null;
        }
        return legacyPath;
    };

    const loadSettings = async () => {
        try {
            const settings = await window.ipcRenderer.getSettings();
            if (activeProfilePlatform === 'mac') {
                setSteamPath(settings.mac_steam_path || getLegacyMacSteamPath(settings.steam_path) || defaultMacSteamPath);
            } else {
                setSteamPath(settings.windows_steam_path || settings.steam_path || '');
            }
        } catch (e) {
            console.error("Failed to load settings", e);
        }
    };

    const checkGamePath = async () => {
        if (!selectedGame) return;
        setCheckingGamePath(true);
        try {
            const path = await window.ipcRenderer.getGamePath(selectedGame, activeProfilePlatform);
            setGamePath(path);
            const source = await window.ipcRenderer.getGameSource(selectedGame, activeProfilePlatform);
            setGameSource(source);
        } catch (e) {
            console.error("Failed to get game path", e);
            setGamePath(null);
            setGameSource('unknown');
        }
        setCheckingGamePath(false);
    };

    useEffect(() => {
        const init = async () => {
            if (isOpen) {
                await loadSettings();
                if (selectedGame) {
                    await checkGamePath();
                }
            }
        };
        init();
    }, [isOpen, selectedGame, activeProfilePlatform]);

    // Modal Entrance/Exit Animation State
    const [prevIsOpen, setPrevIsOpen] = useState(isOpen);
    const [shouldRender, setShouldRender] = useState(isOpen);
    const [isVisible, setIsVisible] = useState(false);

    if (isOpen !== prevIsOpen) {
        setPrevIsOpen(isOpen);
        if (isOpen) {
            setShouldRender(true);
        } else {
            setIsVisible(false);
        }
    }

    useEffect(() => {
        if (isOpen) {
            const timer = setTimeout(() => setIsVisible(true), 10);
            return () => clearTimeout(timer);
        } else {
            const timer = setTimeout(() => setShouldRender(false), 300);
            return () => clearTimeout(timer);
        }
    }, [isOpen]);

    // Handle tab initialization and height transition enablement on open/close
    useEffect(() => {
        let timer1: ReturnType<typeof setTimeout>;
        let timer2: ReturnType<typeof setTimeout>;

        if (isOpen) {
            timer1 = setTimeout(() => {
                if (settingsRef.current) {
                    setTabHeight(settingsRef.current.offsetHeight);
                    timer2 = setTimeout(() => {
                        setEnableHeightTransition(true);
                    }, 50);
                }
            }, 50);
        }

        return () => {
            clearTimeout(timer1);
            clearTimeout(timer2);
        };
    }, [isOpen]);

    // Reset tab and transitions asynchronously when closed to avoid cascading renders
    useEffect(() => {
        if (!isOpen) {
            const timer = setTimeout(() => {
                setActiveTab('settings');
                setTabHeight(null);
                setEnableHeightTransition(false);
            }, 0);
            return () => clearTimeout(timer);
        }
    }, [isOpen]);

    // Track and animate height of the active tab content for dynamic adjustments
    useEffect(() => {
        const wrapper = contentWrapperRef.current;
        if (!wrapper || !isOpen) return;

        const observer = new ResizeObserver(() => {
            const activeEl = wrapper.querySelector('.pointer-events-auto');
            if (activeEl) {
                const inner = activeEl.firstElementChild as HTMLElement;
                const h = inner ? inner.offsetHeight : (activeEl as HTMLElement).offsetHeight;
                if (h > 0) {
                    setTabHeight(h);
                }
            }
        });

        // Observe the wrapper container
        observer.observe(wrapper);

        // Also observe active child if it changes or mutates
        const activeEl = wrapper.querySelector('.pointer-events-auto');
        if (activeEl) {
            observer.observe(activeEl);
        }

        return () => observer.disconnect();
    }, [activeTab, activeProfile, isOpen]);

    const handleSave = async () => {
        setLoading(true);
        try {
            const currentSettings = await window.ipcRenderer.getSettings();
            await window.ipcRenderer.saveSettings({
                ...currentSettings,
                steam_path: activeProfilePlatform === 'windows' ? (steamPath || null) : currentSettings.steam_path,
                windows_steam_path: activeProfilePlatform === 'windows' ? (steamPath || null) : (currentSettings.windows_steam_path || null),
                mac_steam_path: activeProfilePlatform === 'mac'
                    ? ((steamPath && steamPath !== defaultMacSteamPath) ? steamPath : null)
                    : (currentSettings.mac_steam_path || null)
            });
            if (selectedGame) {
                await checkGamePath();
            }
            onClose();
        } catch (e) {
            console.error("Failed to save settings", e);
            alert("Failed to save settings");
        }
        setLoading(false);
    };

    const handleBrowse = async () => {
        try {
            const path = await window.ipcRenderer.selectFolder();
            if (path) setSteamPath(path);
        } catch (e) {
            console.error("Failed to select folder", e);
        }
    };

    const handleOpenGameFolder = async () => {
        if (!selectedGame) return;
        try {
            await window.ipcRenderer.openGameFolder(selectedGame, activeProfilePlatform);
        } catch (e: any) {
            alert(e.message || "Failed to open game directory");
        }
    };

    const handleManualGamePath = async () => {
        if (!selectedGame) return;
        try {
            const path = await window.ipcRenderer.selectFolder();
            if (path) {
                await window.ipcRenderer.setGamePath(selectedGame, path, activeProfilePlatform);
                await checkGamePath();
            }
        } catch (e) {
            console.error("Failed to set manual game path", e);
        }
    };

    const switchTab = (next: SettingsTab) => {
        if (next === activeTab || isAnimating) return;

        setActiveTab(next);
        setIsAnimating(true);
        setTimeout(() => setIsAnimating(false), 300);
    };

    if (!shouldRender) return null;

    const shouldShowSteamDirectory = activeProfilePlatform === 'windows' || gameSource !== 'manual';
    const hasConfigEditor = !!activeProfile;
    const modalWidth = activeTab === 'config-editor' ? 'max-w-4xl w-full' : 'max-w-md w-full';

    return (
        <div
            onClick={onClose}
            className={`fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4 transition-opacity duration-300 ease-[cubic-bezier(0.4,0,0.2,1)] ${isVisible ? 'opacity-100' : 'opacity-0'}`}
        >
            <div
                onClick={(e) => e.stopPropagation()}
                className={`bg-gray-900 border border-gray-700 rounded-xl shadow-2xl ${modalWidth} transform transition-all duration-300 ease-[cubic-bezier(0.4,0,0.2,1)] ${isVisible ? 'scale-100 translate-y-0' : 'scale-95 translate-y-4'}`}
                style={{ transitionProperty: 'max-width, width, transform, opacity' }}
            >
                {/* ── Header with Tabs ──────────────────────────────────────── */}
                <div
                    className="border-b border-gray-700 px-5 flex items-stretch gap-3 min-w-0"
                    style={{ height: '48px' }}
                >
                    {/* Title — vertically centered, same height as tabs */}
                    <h2 className="text-sm font-bold text-white whitespace-nowrap flex-shrink-0 flex items-center leading-none">
                        Profile Settings
                    </h2>
                    <div className="w-px bg-gray-700 flex-shrink-0 self-stretch my-2.5" />

                    {/* Tabs — bottom-underline style, text perfectly centered */}
                    <div className="flex items-stretch gap-0.5 flex-shrink-0">
                        <button
                            id="settings-tab-settings"
                            onClick={() => switchTab('settings')}
                            className={`px-3 text-sm font-medium whitespace-nowrap transition-colors flex items-center gap-1.5 border-b-2 relative ${
                                activeTab === 'settings'
                                    ? 'border-blue-500 text-blue-400'
                                    : 'border-transparent text-gray-400 hover:text-gray-200'
                            }`}
                            style={{ marginBottom: '-1px' }}
                        >
                            <svg className="h-3.5 w-3.5 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                            </svg>
                            Settings
                        </button>

                        {hasConfigEditor && (
                            <button
                                id="settings-tab-config-editor"
                                onClick={() => switchTab('config-editor')}
                                className={`px-3 text-sm font-medium whitespace-nowrap transition-colors flex items-center gap-1.5 border-b-2 ${
                                    activeTab === 'config-editor'
                                        ? 'border-blue-500 text-blue-400'
                                        : 'border-transparent text-gray-400 hover:text-gray-200'
                                }`}
                                style={{ marginBottom: '-1px' }}
                            >
                                <svg className="h-3.5 w-3.5 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
                                </svg>
                                Config Editor
                            </button>
                        )}
                    </div>
                </div>

                {/* ── Tab Content — iOS-style slide + fade ─────────────────── */}
                <div
                    ref={contentWrapperRef}
                    className="overflow-hidden rounded-b-xl relative"
                    style={{
                        height: tabHeight ? `${tabHeight}px` : 'auto',
                        transition: enableHeightTransition
                            ? 'height 0.3s cubic-bezier(0.4,0,0.2,1)'
                            : 'none',
                    }}
                >
                    {/* Settings Tab Content */}
                    <div
                        ref={settingsRef}
                        className={`transition-all duration-300 ease-out transform ${
                            activeTab === 'settings'
                                ? 'opacity-100 translate-x-0 pointer-events-auto'
                                : 'opacity-0 -translate-x-8 pointer-events-none absolute inset-0'
                        }`}
                    >
                        <div className={`p-6 ${isAnimating ? 'w-[28rem]' : 'w-full'}`}>
                            {shouldShowSteamDirectory && (
                                <div className="mb-6">
                                    <label className="block text-sm font-medium text-gray-400 mb-2">
                                        Steam Directory
                                    </label>
                                    <div className="flex gap-2">
                                        <CensoredInput
                                            value={steamPath}
                                            onChange={setSteamPath}
                                            className="w-full bg-gray-800 border border-gray-700 rounded-lg px-3 py-2 text-white text-sm focus:outline-none focus:border-blue-500"
                                            placeholder={activeProfilePlatform === 'mac'
                                                ? defaultMacSteamPath
                                                : "(BottleName)/drive_c/Program Files (x86)/Steam"}
                                        />
                                        <button
                                            onClick={handleBrowse}
                                            className="bg-gray-700 hover:bg-gray-600 text-white px-3 py-2 rounded-lg text-sm transition-colors"
                                        >
                                            Browse
                                        </button>
                                    </div>
                                    <p className="text-xs text-gray-500 mt-2">
                                        {activeProfilePlatform === 'mac'
                                            ? "Select your native macOS Steam folder (e.g., ~/Library/Application Support/Steam)."
                                            : "Select your Steam installation folder (e.g., C:/Program Files (x86)/Steam, or drive_c/Program Files (x86)/Steam inside a compatibility layer)."}
                                    </p>
                                </div>
                            )}

                            {selectedGame && (
                                <div className="mb-6">
                                    <label className="block text-sm font-medium text-gray-400 mb-2">
                                        Game Directory
                                    </label>
                                    {checkingGamePath ? (
                                        <div className="flex items-center gap-2 text-sm text-gray-400 py-2">
                                            <svg className="animate-spin h-4 w-4" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
                                                <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                                                <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
                                            </svg>
                                            Checking...
                                        </div>
                                    ) : gamePath ? (
                                        <div className="space-y-2">
                                            <div className="flex gap-2">
                                                <button
                                                    onClick={handleOpenGameFolder}
                                                    className="flex-1 bg-gray-800 hover:bg-gray-700 border border-gray-700 text-white px-3 py-2 rounded-lg text-sm transition-colors flex items-center justify-center gap-2"
                                                >
                                                    <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
                                                    </svg>
                                                    {openFolderLabel()}
                                                </button>
                                                <button
                                                    onClick={handleManualGamePath}
                                                    className="bg-gray-800 hover:bg-gray-700 border border-gray-700 text-gray-400 hover:text-white px-3 py-2 rounded-lg text-sm transition-colors"
                                                    title="Change Game Location"
                                                >
                                                    <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15.232 5.232l3.536 3.536m-2.036-5.036a2.5 2.5 0 113.536 3.536L6.5 21.036H3v-3.572L16.732 3.732z" />
                                                    </svg>
                                                </button>
                                            </div>
                                            <div className="flex items-start gap-2 text-xs">
                                                <svg className="h-3.5 w-3.5 text-green-500 mt-0.5 flex-shrink-0" fill="currentColor" viewBox="0 0 20 20">
                                                    <path fillRule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zm3.707-9.293a1 1 0 00-1.414-1.414L9 10.586 7.707 9.293a1 1 0 00-1.414 1.414l2 2a1 1 0 001.414 0l4-4z" clipRule="evenodd" />
                                                </svg>
                                                <PathCensor path={gamePath} className="text-gray-400 break-all" />
                                            </div>
                                        </div>
                                    ) : gameSource !== 'manual' ? (
                                        <div className="space-y-3">
                                            <div className="flex items-start gap-2 text-xs text-yellow-400 bg-yellow-900/20 border border-yellow-500/30 rounded-lg p-3">
                                                <svg className="h-4 w-4 mt-0.5 flex-shrink-0" fill="currentColor" viewBox="0 0 20 20">
                                                    <path fillRule="evenodd" d="M8.257 3.099c.765-1.36 2.722-1.36 3.486 0l5.58 9.92c.75 1.334-.213 2.98-1.742 2.98H4.42c-1.53 0-2.493-1.646-1.743-2.98l5.58-9.92zM11 13a1 1 0 11-2 0 1 1 0 012 0zm-1-8a1 1 0 00-1 1v3a1 1 0 002 0V6a1 1 0 00-1-1z" clipRule="evenodd" />
                                                </svg>
                                                <span>Game not detected inside Steam library folders.</span>
                                            </div>
                                            <button
                                                onClick={handleManualGamePath}
                                                className="w-full bg-gray-800 hover:bg-gray-700 border border-gray-700 text-white px-3 py-2 rounded-lg text-sm transition-colors flex items-center justify-center gap-2"
                                            >
                                                <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
                                                </svg>
                                                Manually Locate Game Folder
                                            </button>
                                        </div>
                                    ) : (
                                        <div className="space-y-3">
                                            <div className="text-xs text-gray-500 bg-gray-800 border border-gray-700 rounded-lg p-3">
                                                Set the game directory manually for non-Steam copies.
                                            </div>
                                            <button
                                                onClick={handleManualGamePath}
                                                className="w-full bg-gray-800 hover:bg-gray-700 border border-gray-700 text-white px-3 py-2 rounded-lg text-sm transition-colors flex items-center justify-center gap-2"
                                            >
                                                <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
                                                </svg>
                                                Choose Game Folder
                                            </button>
                                        </div>
                                    )}
                                </div>
                            )}

                            <div className="flex justify-end gap-3">
                                <Button variant="ghost" onClick={onClose}>Cancel</Button>
                                <Button variant="primary" onClick={handleSave} disabled={loading}>
                                    {loading ? 'Saving...' : 'Save'}
                                </Button>
                            </div>
                        </div>
                    </div>

                    {/* Config Editor Tab Content */}
                    {activeProfile && (
                        <div
                            ref={configEditorRef}
                            className={`transition-all duration-300 ease-out transform ${
                                activeTab === 'config-editor'
                                    ? 'opacity-100 translate-x-0 pointer-events-auto'
                                    : 'opacity-0 translate-x-8 pointer-events-none absolute inset-0'
                            }`}
                        >
                            <div className={`flex flex-col ${isAnimating ? 'w-[56rem]' : 'w-full'}`}>
                                <ConfigEditorTab
                                    profileId={activeProfile.id}
                                    gameIdentifier={selectedGame}
                                    platform={activeProfile.platform}
                                    mods={activeProfile.mods}
                                />
                                <div className="flex justify-end px-4 py-3 border-t border-gray-700">
                                    <Button variant="ghost" onClick={onClose}>Close</Button>
                                </div>
                            </div>
                        </div>
                    )}
                </div>
            </div>
        </div>
    );
}
