import { useEffect, useMemo, useState } from 'react';
import { GameSelector } from '../game/GameSelector';
import type { Community, CommunityPlatformInfo } from '../../types/thunderstore';
import type { Profile } from '../../types/profile';
import { HoverMarquee } from '../ui';
import { getProfileAvatarGradient } from '../../utils/profileAvatar';

interface DefaultGamePickerModalProps {
    isOpen: boolean;
    onClose: () => void;
    communities: Community[];
    communityImages: Record<string, string>;
    communityPlatforms: Record<string, CommunityPlatformInfo>;
    currentValue: string | null;
    initialStep?: 'game' | 'profile';
    onPick: (identifier: string | null, profileName?: string | null) => void;
}

let cachedFavoriteGames: string[] = [];

function getFirstLetter(name: string): string {
    if (!name) return '';
    try {
        const segmenter = new Intl.Segmenter(undefined, { granularity: 'grapheme' });
        const segments = segmenter.segment(name);
        const firstSegment = [...segments][0]?.segment;
        return firstSegment ? firstSegment.toUpperCase() : '';
    } catch {
        return ([...name][0] || '').toUpperCase();
    }
}

function formatLastPlayed(lastUsed: number): string {
    if (!lastUsed) {
        return 'Never played';
    }

    const now = new Date();
    const playedAt = new Date(lastUsed);
    const dayMs = 24 * 60 * 60 * 1000;
    const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
    const startOfPlayedDay = new Date(playedAt.getFullYear(), playedAt.getMonth(), playedAt.getDate()).getTime();
    const dayDiff = Math.floor((startOfToday - startOfPlayedDay) / dayMs);
    const diffMs = Math.max(0, now.getTime() - playedAt.getTime());
    const diffMinutes = Math.floor(diffMs / (60 * 1000));

    if (dayDiff === 0) {
        if (diffMinutes < 1) {
            return 'Just now';
        }
        if (diffMinutes < 60) {
            return `${diffMinutes} minute${diffMinutes === 1 ? '' : 's'} ago`;
        }

        const diffHours = Math.floor(diffMinutes / 60);
        return `${diffHours} hour${diffHours === 1 ? '' : 's'} ago`;
    }

    if (dayDiff === 1) {
        return 'Yesterday';
    }

    return playedAt.toLocaleDateString();
}

/**
 * Default Game Picker Modal — Uses the exact pixel-perfect search bar from
 * the homescreen (GameSelectionScreen), fast cached loading, and offers an
 * optional default profile selection step after picking a game.
 */
export function DefaultGamePickerModal({
    isOpen,
    onClose,
    communities,
    communityImages,
    communityPlatforms,
    currentValue,
    initialStep = 'game',
    onPick,
}: DefaultGamePickerModalProps) {
    const [searchQuery, setSearchQuery] = useState('');
    const [showWindowsGame, setShowWindowsGame] = useState(true);
    const [showMacGame, setShowMacGame] = useState(false);
    const [favoriteGames, setFavoriteGames] = useState<string[]>(cachedFavoriteGames);

    // Profile selection step state
    const [step, setStep] = useState<'game' | 'profile'>('game');
    const [selectedGameId, setSelectedGameId] = useState<string | null>(null);
    const [gameProfiles, setGameProfiles] = useState<Profile[]>([]);
    const [loadingProfiles, setLoadingProfiles] = useState(false);

    // Synchronously reset search and update state when isOpen changes
    const [prevIsOpen, setPrevIsOpen] = useState(isOpen);
    if (isOpen !== prevIsOpen) {
        setPrevIsOpen(isOpen);
        if (isOpen) {
            setSearchQuery('');
            if (initialStep === 'profile' && currentValue) {
                setStep('profile');
                setSelectedGameId(currentValue);
                setLoadingProfiles(true);
            } else {
                setStep('game');
                setSelectedGameId(null);
                setGameProfiles([]);
                setLoadingProfiles(false);
            }
        }
    }

    // Load profiles if launched directly into profile selection step
    useEffect(() => {
        if (!isOpen) return;

        if (initialStep === 'profile' && currentValue) {
            let isCancelled = false;
            window.ipcRenderer.getProfiles()
                .then((allProfiles: Profile[]) => {
                    if (isCancelled) return;
                    const matching = allProfiles.filter(p => p.gameIdentifier === currentValue);
                    setGameProfiles(matching);
                })
                .catch((err: any) => console.error('Failed to load profiles for initial step', err))
                .finally(() => {
                    if (!isCancelled) setLoadingProfiles(false);
                });

            return () => {
                isCancelled = true;
            };
        }
    }, [isOpen, initialStep, currentValue]);

    // Load and update favorites in background without blocking rendering
    useEffect(() => {
        if (!isOpen) return;

        let isMounted = true;
        window.ipcRenderer.getSettings()
            .then((settings: any) => {
                if (settings?.favorite_games) {
                    cachedFavoriteGames = settings.favorite_games;
                    if (isMounted) {
                        setFavoriteGames(settings.favorite_games);
                    }
                }
            })
            .catch((err: any) => console.error('Failed to load favorites', err));

        return () => {
            isMounted = false;
        };
    }, [isOpen]);

    const toggleFavorite = async (identifier: string, e: React.MouseEvent) => {
        e.stopPropagation();
        const next = favoriteGames.includes(identifier)
            ? favoriteGames.filter(id => id !== identifier)
            : [...favoriteGames, identifier];
        
        cachedFavoriteGames = next;
        setFavoriteGames(next);

        try {
            const settings = await window.ipcRenderer.getSettings();
            await window.ipcRenderer.saveSettings({ ...settings, favorite_games: next });
        } catch (err) {
            console.error('Failed to save favorites', err);
        }
    };

    // Handle game card click with optional profile prompt step
    const handleGameSelect = async (identifier: string) => {
        if (identifier === currentValue) {
            // Deselect default game
            onPick(null, null);
            try {
                const settings = await window.ipcRenderer.getSettings();
                await window.ipcRenderer.saveSettings({ ...settings, default_game: null, default_profile: null });
            } catch (err) {
                console.error('Failed to clear default game settings', err);
            }
            onClose();
            return;
        }

        setLoadingProfiles(true);
        try {
            const allProfiles = await window.ipcRenderer.getProfiles();
            const matching = allProfiles.filter((p: Profile) => p.gameIdentifier === identifier);
            if (matching.length > 0) {
                setSelectedGameId(identifier);
                setGameProfiles(matching);
                setStep('profile');
            } else {
                onPick(identifier, null);
                const settings = await window.ipcRenderer.getSettings();
                await window.ipcRenderer.saveSettings({ ...settings, default_game: identifier, default_profile: null });
                onClose();
            }
        } catch (err) {
            console.error('Failed to load profiles for game deselect', err);
            onPick(identifier, null);
            onClose();
        } finally {
            setLoadingProfiles(false);
        }
    };

    const handlePickProfile = async (profileName: string | null) => {
        if (!selectedGameId) return;
        onPick(selectedGameId, profileName);
        try {
            const settings = await window.ipcRenderer.getSettings();
            await window.ipcRenderer.saveSettings({
                ...settings,
                default_game: selectedGameId,
                default_profile: profileName,
            });
        } catch (err) {
            console.error('Failed to save default profile setting', err);
        }
        onClose();
    };

    // Filter communities identically to GameSelectionScreen (homescreen)
    const filteredCommunities = useMemo(() => {
        return communities
            .filter(c =>
                c.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
                c.identifier.toLowerCase().includes(searchQuery.toLowerCase())
            )
            .filter(c => {
                const platform = communityPlatforms[c.identifier];
                const isMac = platform?.mac ?? false;
                const isWindows = platform?.windows ?? true;
                if (!showWindowsGame && !showMacGame) return false;
                if (showWindowsGame && !showMacGame) return isWindows;
                if (!showWindowsGame && showMacGame) return isMac;
                return true;
            })
            .sort((a, b) => {
                const aFav = favoriteGames.includes(a.identifier);
                const bFav = favoriteGames.includes(b.identifier);
                if (aFav && !bFav) return -1;
                if (!aFav && bFav) return 1;
                return a.name.localeCompare(b.name);
            });
    }, [communities, searchQuery, communityPlatforms, showWindowsGame, showMacGame, favoriteGames]);

    const currentValueName = useMemo(() => {
        if (!currentValue) return null;
        return communities.find(c => c.identifier === currentValue)?.name ?? currentValue;
    }, [communities, currentValue]);

    const selectedGameName = useMemo(() => {
        if (!selectedGameId) return '';
        return communities.find(c => c.identifier === selectedGameId)?.name ?? selectedGameId;
    }, [communities, selectedGameId]);

    if (!isOpen) return null;

    return (
        <div className="fixed inset-0 z-[60] flex items-center justify-center p-4 md:p-6 backdrop-blur-sm bg-black/65">
            {/* Backdrop */}
            <div className="absolute inset-0" onClick={onClose} />

            {/* Modal Container */}
            <div
                className="relative w-full max-w-6xl h-[85vh] flex flex-col bg-gray-900 border border-gray-700/80 rounded-2xl shadow-2xl overflow-hidden"
                onClick={(e) => e.stopPropagation()}
            >
                {step === 'game' ? (
                    <>
                        {/* Modal Header */}
                        <div className="flex items-center justify-between px-7 py-5 border-b border-gray-800 shrink-0 bg-gray-900 z-10">
                            <div className="flex items-center gap-3">
                                <h2 className="text-xl font-bold text-white tracking-tight">Choose Default Game</h2>
                                {currentValueName ? (
                                    <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-blue-500/10 text-fg-accent border border-blue-500/20">
                                        Current: {currentValueName}
                                    </span>
                                ) : (
                                    <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-gray-800 text-gray-400 border border-gray-700">
                                        No default set
                                    </span>
                                )}
                            </div>
                            <button
                                onClick={onClose}
                                className="p-2 rounded-xl hover:bg-gray-800 text-gray-400 hover:text-white transition-all active:scale-95 focus:outline-none focus:ring-2 focus:ring-gray-700"
                                title="Close"
                            >
                                <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M6 18L18 6M6 6l12 12" />
                                </svg>
                            </button>
                        </div>

                        {/* Search & Platform Controls Bar (Exact 1:1 match with GameSelectionScreen) */}
                        <div className="px-7 py-4 border-b border-gray-800 bg-gray-900/90 shrink-0 z-10">
                            <div className="flex items-stretch gap-3 min-w-0">
                                <div className="relative flex-1 min-w-0">
                                    <div className="absolute inset-y-0 left-0 pl-4 flex items-center pointer-events-none text-gray-500">
                                        <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
                                        </svg>
                                    </div>
                                    <input
                                        className="w-full h-full min-h-[52px] bg-gray-800 border border-gray-700 pl-12 pr-24 py-3 rounded-xl text-base text-white placeholder-gray-500 focus:outline-none focus:border-blue-500 transition-all shadow-lg"
                                        placeholder="Search for a game..."
                                        value={searchQuery}
                                        onChange={e => setSearchQuery(e.target.value)}
                                        spellCheck={false}
                                        autoCorrect="off"
                                        autoCapitalize="none"
                                        autoComplete="off"
                                        autoFocus
                                    />

                                    {/* Platform Filter Switcher (Exact Homescreen Implementation) */}
                                    <div className="absolute right-1 top-1/2 -translate-y-1/2 flex bg-gray-800 rounded-lg p-1 border border-gray-700 overflow-hidden" title="Platform Filter">
                                        <div
                                            className={`absolute top-1 left-1 w-9 h-9 bg-gray-600 rounded-md transition-transform duration-300 ease-[cubic-bezier(0.25,0.1,0.25,1)] ${showWindowsGame && !showMacGame ? 'translate-x-0' : 'translate-x-9'}`}
                                        />
                                        <button
                                            type="button"
                                            onClick={() => {
                                                setShowWindowsGame(true);
                                                setShowMacGame(false);
                                            }}
                                            className={`relative z-10 w-9 h-9 rounded-md items-center justify-center flex transition-colors ${showWindowsGame && !showMacGame ? 'text-white' : 'text-gray-400 hover:text-white'}`}
                                            title="Windows Games Only"
                                        >
                                            <svg xmlns="http://www.w3.org/2000/svg" className="w-[14px] h-[14px]" viewBox="0 0 24 24" fill="currentColor">
                                                <path d="M0 3.449L9.75 2.1v9.451H0m10.949-9.602L24 0v11.4H10.949M0 12.6h9.75v9.451L0 20.699M10.949 12.6H24V24l-12.9-1.801" />
                                            </svg>
                                        </button>
                                        <button
                                            type="button"
                                            onClick={() => {
                                                setShowMacGame(true);
                                                setShowWindowsGame(false);
                                            }}
                                            className={`relative z-10 w-9 h-9 rounded-md items-center justify-center flex transition-colors ${!showWindowsGame && showMacGame ? 'text-white' : 'text-gray-400 hover:text-white'}`}
                                            title="macOS Games Only"
                                        >
                                            <svg xmlns="http://www.w3.org/2000/svg" className="w-[12px] h-[14px]" viewBox="0 0 384 512" fill="currentColor">
                                                <path d="M318.7 268.7c-.2-36.7 16.4-64.4 50-84.8-18.8-26.9-47.2-41.7-84.7-44.6-35.5-2.8-74.3 20.7-88.5 20.7-15 0-49.4-19.7-76.4-19.7C63.3 141.2 4 184.8 4 273.5q0 39.3 14.4 81.2c12.8 36.7 59 126.7 107.2 125.2 25.2-.6 43-17.9 75.8-17.9 31.8 0 48.3 17.9 76.4 17.9 48.6-.7 90.4-82.5 102.6-119.3-65.2-30.7-61.7-90-61.7-91.9zm-56.6-164.2c27.3-32.4 24.8-61.9 24-72.5-24.1 1.4-52 16.4-67.9 34.9-17.5 19.8-27.8 44.3-25.6 71.9 26.1 2 49.9-11.4 69.5-34.3z" />
                                            </svg>
                                        </button>
                                    </div>
                                </div>

                                {/* Action Button: Clear Default (Visible ONLY when a default game is selected, with Apple-style spring entrance) */}
                                {currentValue && (
                                    <button
                                        type="button"
                                        onClick={() => { handleGameSelect(currentValue); }}
                                        className="h-[52px] px-4 bg-red-500/10 hover:bg-red-500/20 active:bg-red-500/30 text-fg-danger hover:text-fg-danger active:text-fg-danger border border-red-500/30 hover:border-red-500/50 rounded-xl shadow-lg flex-none flex items-center justify-center gap-2 text-sm font-semibold transition-all duration-300 ease-[cubic-bezier(0.34,1.56,0.64,1)] transform active:scale-95 animate-[profile-update-action-enter_220ms_cubic-bezier(0.22,1,0.36,1)]"
                                        title="Clear default game"
                                    >
                                        <svg className="w-5 h-5 text-fg-danger" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M18.364 18.364A9 9 0 005.636 5.636m12.728 12.728A9 9 0 015.636 5.636m12.728 12.728L5.636 5.636" />
                                        </svg>
                                        <span>Clear default</span>
                                    </button>
                                )}
                            </div>
                        </div>

                        {/* Game Selector Content Container */}
                        <div className="flex-1 min-h-0 overflow-y-auto bg-gray-900 relative">
                            {loadingProfiles && (
                                <div className="absolute inset-0 bg-gray-900/60 backdrop-blur-xs z-40 flex items-center justify-center">
                                    <div className="flex items-center gap-3 bg-gray-800 border border-gray-700 px-5 py-3 rounded-xl shadow-xl">
                                        <svg className="animate-spin h-5 w-5 text-blue-500" fill="none" viewBox="0 0 24 24">
                                            <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                                            <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
                                        </svg>
                                        <span className="text-sm font-medium text-white">Checking profiles...</span>
                                    </div>
                                </div>
                            )}

                            <GameSelector
                                communities={filteredCommunities}
                                selectedCommunity={currentValue}
                                onSelect={handleGameSelect}
                                communityImages={communityImages}
                                communityPlatforms={communityPlatforms}
                                favoriteGames={favoriteGames}
                                onToggleFavorite={toggleFavorite}
                                searchQuery={searchQuery}
                                containerClassName="p-6 space-y-8"
                            />
                        </div>
                    </>
                ) : (
                    /* Step 2: Select Default Profile (Exact 1:1 UI parity with ProfileList.tsx) */
                    <div className="flex-1 flex flex-col min-h-0 animate-[profile-update-action-enter_220ms_cubic-bezier(0.22,1,0.36,1)]">
                        {/* Profile Screen Header */}
                        <div className="flex items-start justify-between p-8 pb-4 shrink-0 bg-gray-900 border-b border-gray-800/80 z-10">
                            <div className="min-w-0">
                                <h1 className="text-3xl font-bold text-white mb-2">Select Default Profile</h1>
                                <p className="text-gray-400">Choose a profile to open automatically for {selectedGameName}</p>
                            </div>

                            <div className="flex items-center gap-3 shrink-0">
                                <button
                                    onClick={() => setStep('game')}
                                    className="px-4 py-2 rounded-xl bg-gray-800 hover:bg-gray-700 border border-gray-700 text-sm font-medium text-gray-300 hover:text-white transition-all active:scale-95 flex items-center gap-2"
                                >
                                    <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 19l-7-7 7-7" />
                                    </svg>
                                    <span>Back to games</span>
                                </button>

                                <button
                                    onClick={onClose}
                                    className="p-2 rounded-xl hover:bg-gray-800 text-gray-400 hover:text-white transition-all active:scale-95 focus:outline-none focus:ring-2 focus:ring-gray-700"
                                    title="Close"
                                >
                                    <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M6 18L18 6M6 6l12 12" />
                                    </svg>
                                </button>
                            </div>
                        </div>

                        {/* Profiles Grid — 1:1 match of ProfileList.tsx card layout */}
                        <div className="flex-1 overflow-y-auto p-8 bg-gray-900">
                            <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
                                {gameProfiles.map(profile => (
                                    <div
                                        key={profile.id}
                                        onClick={() => handlePickProfile(profile.name)}
                                        className="bg-gray-800 border border-gray-700 rounded-xl p-6 hover:border-blue-500 transition-all cursor-pointer flex flex-col min-h-[200px] group relative overflow-hidden transform-gpu"
                                    >
                                        <div aria-hidden="true" className="pointer-events-none absolute right-0 top-0 z-10 h-24 w-48 bg-gradient-to-l from-gray-800/90 via-gray-800/50 to-transparent opacity-0 transition-opacity duration-200 group-hover:opacity-100" />

                                        <div className={`flex-1 flex flex-col transition-all duration-300 ${profile.is_vanilla ? 'grayscale opacity-60' : ''}`}>
                                            {profile.profileImageUrl ? (
                                                <img
                                                    src={profile.profileImageUrl}
                                                    alt={profile.name}
                                                    className="w-16 h-16 rounded-2xl mb-4 flex-shrink-0 object-cover bg-gray-800 shadow-md"
                                                />
                                            ) : (
                                                <div
                                                    style={{ backgroundImage: getProfileAvatarGradient(profile.name, profile.id) }}
                                                    className="w-16 h-16 rounded-2xl mb-4 flex-shrink-0 flex items-center justify-center text-2xl font-bold text-[#ffffff] shadow-md"
                                                >
                                                    {getFirstLetter(profile.name)}
                                                </div>
                                            )}

                                            <div className="flex min-w-0 items-center gap-2 mb-2">
                                                <HoverMarquee text={profile.name} className="text-xl font-bold text-white" />
                                                {profile.platform === 'mac' ? (
                                                    <span title="macOS profile" className="flex flex-col justify-center items-center flex-shrink-0 text-gray-400 w-4 h-4 pb-0.5">
                                                        <svg xmlns="http://www.w3.org/2000/svg" className="w-[12px] h-[14px]" viewBox="0 0 384 512" fill="currentColor">
                                                            <path d="M318.7 268.7c-.2-36.7 16.4-64.4 50-84.8-18.8-26.9-47.2-41.7-84.7-44.6-35.5-2.8-74.3 20.7-88.5 20.7-15 0-49.4-19.7-76.4-19.7C63.3 141.2 4 184.8 4 273.5q0 39.3 14.4 81.2c12.8 36.7 59 126.7 107.2 125.2 25.2-.6 43-17.9 75.8-17.9 31.8 0 48.3 17.9 76.4 17.9 48.6-.7 90.4-82.5 102.6-119.3-65.2-30.7-61.7-90-61.7-91.9zm-56.6-164.2c27.3-32.4 24.8-61.9 24-72.5-24.1 1.4-52 16.4-67.9 34.9-17.5 19.8-27.8 44.3-25.6 71.9 26.1 2 49.9-11.4 69.5-34.3z" />
                                                        </svg>
                                                    </span>
                                                ) : (
                                                    <span title="Windows/Wine profile" className="flex flex-col justify-center items-center flex-shrink-0 text-gray-400 w-4 h-4">
                                                        <svg xmlns="http://www.w3.org/2000/svg" className="w-[14px] h-[14px]" viewBox="0 0 24 24" fill="currentColor">
                                                            <path d="M0 3.449L9.75 2.1v9.451H0m10.949-9.602L24 0v11.4H10.949M0 12.6h9.75v9.451L0 20.699M10.949 12.6H24V24l-12.9-1.801" />
                                                        </svg>
                                                    </span>
                                                )}
                                            </div>

                                            <div className="mt-auto space-y-2">
                                                <div className="flex items-center text-sm text-gray-400 gap-2">
                                                    <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4" />
                                                    </svg>
                                                    {profile.is_vanilla ? (
                                                        <span className="text-gray-500 font-bold uppercase tracking-wider text-xs">DISABLED</span>
                                                    ) : (
                                                        <span>{profile.mods?.length ?? 0} mods in profile</span>
                                                    )}
                                                </div>
                                                <div className="flex items-center text-sm text-gray-400 gap-2">
                                                    <span title="Last played" aria-label="Last played" className="flex items-center justify-center cursor-help">
                                                        <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" aria-hidden="true">
                                                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
                                                        </svg>
                                                    </span>
                                                    <span>{formatLastPlayed(profile.lastUsed)}</span>
                                                </div>
                                            </div>
                                        </div>
                                    </div>
                                ))}

                                {/* Skip Profile Card (Dashed 1:1 action card) */}
                                <div
                                    className="bg-gray-800/50 border-2 border-dashed border-gray-700 rounded-xl p-6 flex flex-col items-center justify-center text-center hover:border-gray-500 hover:bg-gray-800 transition-all cursor-pointer min-h-[200px] group"
                                    onClick={() => handlePickProfile(null)}
                                >
                                    <div className="w-16 h-16 bg-gray-900 rounded-full flex items-center justify-center mb-4 group-hover:bg-gray-700/50 transition-colors">
                                        <svg xmlns="http://www.w3.org/2000/svg" className="h-8 w-8 text-gray-500 group-hover:text-gray-300 transition-colors" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 7l5 5m0 0l-5 5m5-5H6" />
                                        </svg>
                                    </div>
                                    <h3 className="text-lg font-bold text-white mb-1">Skip Profile</h3>
                                    <p className="text-sm text-gray-500">Only set default game</p>
                                </div>
                            </div>
                        </div>
                    </div>
                )}
            </div>
        </div>
    );
}




