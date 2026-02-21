import { useState, useEffect } from 'react';
import { GameSelector } from '../game/GameSelector';
import { MAC_SUPPORTED_GAMES } from '../../data/platforms';

export interface GameSelectionScreenProps {
    communities: any[];
    communityImages: Record<string, string>;
    loading: boolean;
    selectedCommunity: string | null;
    onSelectCommunity: (id: string) => void;
    onOpenPreferences: () => void;
}

export function GameSelectionScreen({
    communities,
    communityImages,
    loading,
    selectedCommunity,
    onSelectCommunity,
    onOpenPreferences
}: GameSelectionScreenProps) {
    const [gameSearchQuery, setGameSearchQuery] = useState('');
    const [showWindowsGame, setShowWindowsGame] = useState(true);
    const [showMacGame, setShowMacGame] = useState(true);
    const [favoriteGames, setFavoriteGames] = useState<string[]>([]);

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
            c.name.toLowerCase().includes(gameSearchQuery.toLowerCase()) ||
            c.identifier.toLowerCase().includes(gameSearchQuery.toLowerCase())
        )
        .filter(c => {
            const isMac = MAC_SUPPORTED_GAMES.includes(c.identifier);
            if (!showWindowsGame && !showMacGame) return false;
            if (showWindowsGame && !showMacGame) return !isMac;
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
        <div className="flex flex-col h-full bg-gray-900 p-8 overflow-y-auto w-full">
            <div className="max-w-4xl mx-auto w-full">
                <div className="text-center mb-12">
                    <h1 className="text-4xl font-bold text-white mb-4">Welcome to r2modmac</h1>
                    <p className="text-xl text-gray-400">Select a game to begin managing your mods</p>
                </div>

                <div className="flex gap-3 items-center w-full px-4 mb-8">
                    <div className="relative flex-1 flex items-center">
                        <input
                            className="w-full bg-gray-800 border border-gray-700 p-4 pr-24 rounded-xl text-lg text-white placeholder-gray-500 focus:outline-none focus:border-blue-500 transition-all shadow-lg"
                            placeholder="Search for a game..."
                            value={gameSearchQuery}
                            onChange={e => setGameSearchQuery(e.target.value)}
                            autoFocus
                        />
                        <div className="absolute right-3 flex items-center gap-2 text-gray-400">
                            <button
                                onClick={() => setShowWindowsGame(!showWindowsGame)}
                                className={`p-1.5 rounded-lg transition-colors flex items-center justify-center w-8 h-8 ${showWindowsGame ? 'text-white bg-gray-700 hover:bg-gray-600' : 'text-gray-600 hover:text-gray-400'}`}
                                title="Toggle Windows Games"
                            >
                                <svg xmlns="http://www.w3.org/2000/svg" className="w-[14px] h-[14px]" viewBox="0 0 24 24" fill="currentColor">
                                    <path d="M0 3.449L9.75 2.1v9.451H0m10.949-9.602L24 0v11.4H10.949M0 12.6h9.75v9.451L0 20.699M10.949 12.6H24V24l-12.9-1.801" />
                                </svg>
                            </button>
                            <div className="w-[1px] h-4 bg-gray-700"></div>
                            <button
                                onClick={() => setShowMacGame(!showMacGame)}
                                className={`p-1.5 rounded-lg transition-colors flex items-center justify-center w-8 h-8 ${showMacGame ? 'text-white bg-gray-700 hover:bg-gray-600' : 'text-gray-600 hover:text-gray-400'}`}
                                title="Toggle macOS Games"
                            >
                                <svg xmlns="http://www.w3.org/2000/svg" className="w-[12px] h-[14px]" viewBox="0 0 384 512" fill="currentColor">
                                    <path d="M318.7 268.7c-.2-36.7 16.4-64.4 50-84.8-18.8-26.9-47.2-41.7-84.7-44.6-35.5-2.8-74.3 20.7-88.5 20.7-15 0-49.4-19.7-76.4-19.7C63.3 141.2 4 184.8 4 273.5q0 39.3 14.4 81.2c12.8 36.7 59 126.7 107.2 125.2 25.2-.6 43-17.9 75.8-17.9 31.8 0 48.3 17.9 76.4 17.9 48.6-.7 90.4-82.5 102.6-119.3-65.2-30.7-61.7-90-61.7-91.9zm-56.6-164.2c27.3-32.4 24.8-61.9 24-72.5-24.1 1.4-52 16.4-67.9 34.9-17.5 19.8-27.8 44.3-25.6 71.9 26.1 2 49.9-11.4 69.5-34.3z" />
                                </svg>
                            </button>
                        </div>
                    </div>
                    <button
                        onClick={onOpenPreferences}
                        className="p-4 bg-gray-800 border border-gray-700 rounded-xl hover:bg-gray-700 hover:border-gray-600 transition-all text-gray-400 hover:text-white shadow-lg"
                        title="Preferences"
                    >
                        <svg className="w-7 h-7" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                        </svg>
                    </button>
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
                            favoriteGames={favoriteGames}
                            onToggleFavorite={toggleFavorite}
                        />
                    </div>
                )}
            </div>
        </div>
    );
}
