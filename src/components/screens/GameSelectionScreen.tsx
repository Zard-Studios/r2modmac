import { useState, useEffect } from 'react';
import { GameSelector } from '../game/GameSelector';
import { SponsorSurface } from '../sponsors/SponsorSurface';
import type { CommunityPlatformInfo } from '../../types/thunderstore';

export interface GameSelectionScreenProps {
    communities: any[];
    communityImages: Record<string, string>;
    communityPlatforms: Record<string, CommunityPlatformInfo>;
    loading: boolean;
    selectedCommunity: string | null;
    onSelectCommunity: (id: string) => void;
    onOpenPreferences: () => void;
    searchQuery: string;
    onSearchQueryChange: (value: string) => void;
}

export function GameSelectionScreen({
    communities,
    communityImages,
    communityPlatforms,
    loading,
    selectedCommunity,
    onSelectCommunity,
    onOpenPreferences,
    searchQuery,
    onSearchQueryChange,
}: GameSelectionScreenProps) {
    const [showWindowsGame, setShowWindowsGame] = useState(true);
    const [showMacGame, setShowMacGame] = useState(false);
    const [favoriteGames, setFavoriteGames] = useState<string[]>([]);
    const currentYear = new Date().getFullYear();

    // Load favorites
    useEffect(() => {
        window.ipcRenderer.getSettings().then((settings: any) => {
            if (settings.favorite_games) {
                setFavoriteGames(settings.favorite_games);
            }
        }).catch((err: any) => console.error('Failed to load favorites', err));
    }, []);

    const toggleFavorite = async (identifier: string, e: React.MouseEvent) => {
        e.stopPropagation();
        const newFavorites = favoriteGames.includes(identifier)
            ? favoriteGames.filter(id => id !== identifier)
            : [...favoriteGames, identifier];

        setFavoriteGames(newFavorites);

        try {
            const settings = await window.ipcRenderer.getSettings();
            await window.ipcRenderer.saveSettings({ ...settings, favorite_games: newFavorites });
        } catch (e) {
            console.error("Failed to save favorites", e);
        }
    };

    const filteredCommunities = communities
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

    return (
        <div className="r2-app-backdrop relative flex h-full min-h-0 w-full bg-gray-900">
            <div className="h-full w-full overflow-y-auto p-8 pb-32">
                <div className="max-w-7xl mx-auto w-full min-h-full flex flex-col">
                <div className="text-center mb-10">
                    <h1 className="text-4xl font-bold text-white mb-3">Welcome to r2modmac</h1>
                    <p className="text-lg text-gray-400">Select a game to begin managing your mods</p>
                </div>

                <div className="w-full mb-8">
                    <div className="max-w-2xl mx-auto w-full mb-8">
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
                                    onChange={e => onSearchQueryChange(e.target.value)}
                                    spellCheck={false}
                                    autoCorrect="off"
                                    autoCapitalize="none"
                                    autoComplete="off"
                                    autoFocus
                                />

                                <div className="absolute right-1.5 top-1/2 -translate-y-1/2 flex bg-gray-900/60 rounded-lg p-1 border border-gray-700/60 backdrop-blur-sm overflow-hidden" title="Platform Filter">
                                    {/* Sliding Background */}
                                    <div
                                        className={`absolute top-1 left-1 w-9 h-9 bg-gray-700/90 rounded-md border border-gray-600/50 shadow-sm transition-transform duration-300 ease-[cubic-bezier(0.25,0.1,0.25,1)] ${showWindowsGame && !showMacGame ? 'translate-x-0' : 'translate-x-9'}`}
                                    />
                                    <button
                                        onClick={() => {
                                            setShowWindowsGame(true);
                                            setShowMacGame(false);
                                        }}
                                        className={`relative z-10 w-9 h-9 rounded-md items-center justify-center flex transition-colors ${showWindowsGame && !showMacGame ? 'text-white font-bold' : 'text-gray-400 hover:text-white'}`}
                                        title="Windows Games Only"
                                    >
                                        <svg xmlns="http://www.w3.org/2000/svg" className="w-[14px] h-[14px]" viewBox="0 0 24 24" fill="currentColor">
                                            <path d="M0 3.449L9.75 2.1v9.451H0m10.949-9.602L24 0v11.4H10.949M0 12.6h9.75v9.451L0 20.699M10.949 12.6H24V24l-12.9-1.801" />
                                        </svg>
                                    </button>
                                    <button
                                        onClick={() => {
                                            setShowMacGame(true);
                                            setShowWindowsGame(false);
                                        }}
                                        className={`relative z-10 w-9 h-9 rounded-md items-center justify-center flex transition-colors ${!showWindowsGame && showMacGame ? 'text-white font-bold' : 'text-gray-400 hover:text-white'}`}
                                        title="macOS Games Only"
                                    >
                                        <svg xmlns="http://www.w3.org/2000/svg" className="w-[12px] h-[14px]" viewBox="0 0 384 512" fill="currentColor">
                                            <path d="M318.7 268.7c-.2-36.7 16.4-64.4 50-84.8-18.8-26.9-47.2-41.7-84.7-44.6-35.5-2.8-74.3 20.7-88.5 20.7-15 0-49.4-19.7-76.4-19.7C63.3 141.2 4 184.8 4 273.5q0 39.3 14.4 81.2c12.8 36.7 59 126.7 107.2 125.2 25.2-.6 43-17.9 75.8-17.9 31.8 0 48.3 17.9 76.4 17.9 48.6-.7 90.4-82.5 102.6-119.3-65.2-30.7-61.7-90-61.7-91.9zm-56.6-164.2c27.3-32.4 24.8-61.9 24-72.5-24.1 1.4-52 16.4-67.9 34.9-17.5 19.8-27.8 44.3-25.6 71.9 26.1 2 49.9-11.4 69.5-34.3z" />
                                        </svg>
                                    </button>
                                </div>
                            </div>

                            <button
                                onClick={onOpenPreferences}
                                className="h-[52px] w-[52px] bg-gray-800 border border-gray-700 rounded-xl hover:bg-gray-700 hover:border-gray-600 active:scale-95 transition-all text-gray-400 hover:text-white shadow-lg flex-none flex items-center justify-center"
                                title="Preferences"
                            >
                                <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
                                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                                </svg>
                            </button>
                        </div>
                    </div>

                    {loading ? (
                        <div className="text-center text-gray-400 py-12">
                            <div className="flex justify-center">
                                <svg className="animate-spin h-10 w-10 text-blue-500 mb-4" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
                                    <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4"></circle>
                                    <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                                </svg>
                            </div>
                            <p>Loading games...</p>
                        </div>
                    ) : (
                        <div>
                            <GameSelector
                                communities={filteredCommunities}
                                selectedCommunity={selectedCommunity}
                                onSelect={onSelectCommunity}
                                communityImages={communityImages}
                                communityPlatforms={communityPlatforms}
                                favoriteGames={favoriteGames}
                                onToggleFavorite={toggleFavorite}
                                searchQuery={searchQuery}
                            />
                        </div>
                    )}
                </div>
                <footer className="max-w-7xl mx-auto w-full pt-8 pb-2">
                    <div className="border-t border-gray-800 pt-4">
                        <div className="text-center text-xs text-gray-500 flex flex-wrap items-center justify-center gap-2">
                            <span className="inline-flex items-center">Copyright © {currentYear} Zard Studios</span>
                            <span className="text-gray-700">·</span>
                            <a
                                href="https://ko-fi.com/zardstudios"
                                target="_blank"
                                rel="noopener noreferrer"
                                className="inline-flex items-center gap-1.5 leading-none text-gray-400 hover:text-white transition-colors"
                            >
                                <svg className="w-3.5 h-3.5" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
                                    <path d="M23.881 8.948c-.773-4.085-4.859-4.593-4.859-4.593H.723c-.604 0-.679.798-.679.798s-.082 7.324-.022 11.822c.164 2.424 2.586 2.672 2.586 2.672s8.267-.023 11.966-.049c2.438-.426 2.683-2.566 2.658-3.734 4.352.24 7.422-2.831 6.649-6.916zm-11.062 3.511c-1.246 1.453-4.011 3.976-4.011 3.976s-.121.119-.31.023c-.076-.057-.108-.09-.108-.09-.443-.441-3.368-3.049-4.034-3.954-.709-.965-1.041-2.7-.091-3.71.951-1.01 3.005-1.086 4.363.407 0 0 1.565-1.782 3.468-.963 1.904.82 1.832 3.011.723 4.311zm6.173.478c-.928.116-1.682.028-1.682.028V7.284h1.77s1.971.551 1.971 2.638c0 1.913-.985 2.667-2.059 3.015z" />
                                </svg>
                                support me
                            </a>
                            <span className="text-gray-700">·</span>
                            <a
                                href="https://github.com/Zard-Studios"
                                target="_blank"
                                rel="noopener noreferrer"
                                className="inline-flex items-center gap-1.5 leading-none text-gray-400 hover:text-white transition-colors"
                            >
                                <svg className="w-3.5 h-3.5" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
                                    <path d="M12 .5A12 12 0 0 0 8.2 23.9c.6.1.8-.2.8-.6v-2c-3.3.7-4-1.4-4-1.4-.6-1.4-1.3-1.8-1.3-1.8-1.1-.8.1-.8.1-.8 1.2.1 1.9 1.2 1.9 1.2 1.1 1.9 2.9 1.3 3.6 1 .1-.8.4-1.3.8-1.6-2.6-.3-5.3-1.3-5.3-5.7 0-1.2.4-2.2 1.2-3-.1-.3-.5-1.5.1-3.1 0 0 1-.3 3.2 1.2a11.1 11.1 0 0 1 5.8 0c2.2-1.5 3.2-1.2 3.2-1.2.6 1.6.2 2.8.1 3.1.8.8 1.2 1.8 1.2 3 0 4.4-2.7 5.4-5.3 5.7.4.3.8 1 .8 2.1v3.1c0 .4.2.7.8.6A12 12 0 0 0 12 .5Z" />
                                </svg>
                                projects
                            </a>
                        </div>
                    </div>
                </footer>
                </div>
            </div>
            <SponsorSurface placement="home-support" visible />
        </div>
    );
}
