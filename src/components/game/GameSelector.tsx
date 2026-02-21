
import type { Community } from '../../types/thunderstore';
import { MAC_SUPPORTED_GAMES } from '../../data/platforms';

interface GameSelectorProps {
    communities: Community[];
    selectedCommunity: string | null;
    onSelect: (identifier: string) => void;
    communityImages: Record<string, string>;
    favoriteGames: string[];
    onToggleFavorite: (identifier: string, e: React.MouseEvent) => void;
}

export function GameSelector({ communities, selectedCommunity, onSelect, communityImages, favoriteGames, onToggleFavorite }: GameSelectorProps) {

    // Function to get initials from game name
    const getInitials = (name: string) => {
        return name
            .split(' ')
            .map(word => word[0])
            .join('')
            .slice(0, 2)
            .toUpperCase();
    };

    // Function to get a consistent gradient based on string
    const getGradient = (str: string) => {
        const gradients = [
            'from-red-500 to-orange-500',
            'from-orange-500 to-amber-500',
            'from-amber-500 to-yellow-500',
            'from-yellow-500 to-lime-500',
            'from-lime-500 to-green-500',
            'from-green-500 to-emerald-500',
            'from-emerald-500 to-teal-500',
            'from-teal-500 to-cyan-500',
            'from-cyan-500 to-sky-500',
            'from-sky-500 to-blue-500',
            'from-indigo-500 to-violet-500',
            'from-violet-500 to-purple-500',
            'from-purple-500 to-fuchsia-500',
            'from-fuchsia-500 to-pink-500',
            'from-pink-500 to-rose-500',
            'from-rose-500 to-red-500',
            'from-slate-500 to-gray-500',
            'from-gray-600 to-zinc-600',
            'from-zinc-700 to-neutral-700',
        ];
        let hash = 0;
        for (let i = 0; i < str.length; i++) {
            hash = str.charCodeAt(i) + ((hash << 5) - hash);
        }
        return gradients[Math.abs(hash) % gradients.length];
    };

    const renderCommunityCard = (community: Community) => {
        const isSelected = selectedCommunity === community.identifier;
        const isFavorite = favoriteGames.includes(community.identifier);
        const gradient = getGradient(community.name);
        const imageUrl = communityImages[community.identifier];
        const isMacCompatible = MAC_SUPPORTED_GAMES.includes(community.identifier);

        return (
            <button
                key={community.identifier}
                onClick={() => onSelect(community.identifier)}
                className={`group relative flex flex-col rounded-xl overflow-hidden transition-all duration-300 text-left border ${isSelected
                    ? 'ring-2 ring-blue-500 border-transparent shadow-md shadow-blue-500/20 scale-[1.02]'
                    : 'border-gray-800 hover:border-gray-600 hover:shadow-lg hover:shadow-black/30 hover:-translate-y-1'
                    }`}
            >
                {/* Card Image Area - 3:4 Aspect Ratio */}
                <div className="aspect-[3/4] w-full relative overflow-hidden bg-gray-950 isolate">
                    {/* Background Gradient Fallback - Only visible while loading or if missing */}
                    <div className={`absolute inset-0 bg-gradient-to-br ${gradient} opacity-40`} />

                    {/* Image with fallback logic */}
                    {imageUrl ? (
                        <img
                            src={imageUrl}
                            alt={community.name}
                            className="absolute inset-0 w-full h-full object-cover opacity-0 transition-opacity duration-500 z-10"
                            onLoad={(e) => e.currentTarget.classList.remove('opacity-0')}
                            onError={(e) => { e.currentTarget.style.display = 'none'; }}
                        />
                    ) : null}

                    {/* Initials - Centered fallback */}
                    <div className="absolute inset-0 flex items-center justify-center">
                        <span className="text-4xl font-black text-white/20 select-none transform group-hover:scale-110 transition-transform duration-500">
                            {getInitials(community.name)}
                        </span>
                    </div>

                    {/* Overlay Gradient for contrast */}
                    <div className="absolute inset-x-0 bottom-0 h-2/3 bg-gradient-to-t from-gray-900 via-gray-900/60 to-transparent opacity-0 group-hover:opacity-90 transition-opacity duration-300 z-20" />

                    {/* Platform Indicators */}
                    <div className="absolute top-2 right-2 flex flex-col gap-1 z-30 pointer-events-none items-end">
                        <div className={`bg-black/60 backdrop-blur-md text-white p-1 rounded-full shadow-lg border border-white/10 flex items-center justify-center gap-2 h-7 ${isMacCompatible ? 'px-2' : 'w-7 px-0'}`}>
                            <span title="Windows Compatible" className="flex items-center justify-center w-3.5 h-3.5">
                                <svg xmlns="http://www.w3.org/2000/svg" className="w-[14px] h-[14px]" viewBox="0 0 24 24" fill="currentColor">
                                    <path d="M0 3.449L9.75 2.1v9.451H0m10.949-9.602L24 0v11.4H10.949M0 12.6h9.75v9.451L0 20.699M10.949 12.6H24V24l-12.9-1.801" />
                                </svg>
                            </span>
                            {isMacCompatible && (
                                <>
                                    <div className="w-[1px] h-3.5 bg-white/20"></div>
                                    <span title="MacOS Compatible" className="flex items-center justify-center w-3 h-3.5">
                                        <svg xmlns="http://www.w3.org/2000/svg" className="w-[12px] h-[14px]" viewBox="0 0 384 512" fill="currentColor">
                                            <path d="M318.7 268.7c-.2-36.7 16.4-64.4 50-84.8-18.8-26.9-47.2-41.7-84.7-44.6-35.5-2.8-74.3 20.7-88.5 20.7-15 0-49.4-19.7-76.4-19.7C63.3 141.2 4 184.8 4 273.5q0 39.3 14.4 81.2c12.8 36.7 59 126.7 107.2 125.2 25.2-.6 43-17.9 75.8-17.9 31.8 0 48.3 17.9 76.4 17.9 48.6-.7 90.4-82.5 102.6-119.3-65.2-30.7-61.7-90-61.7-91.9zm-56.6-164.2c27.3-32.4 24.8-61.9 24-72.5-24.1 1.4-52 16.4-67.9 34.9-17.5 19.8-27.8 44.3-25.6 71.9 26.1 2 49.9-11.4 69.5-34.3z" />
                                        </svg>
                                    </span>
                                </>
                            )}
                        </div>
                        {isSelected && (
                            <div className="bg-blue-500 text-white p-1 rounded-full shadow-lg border border-blue-400 z-30">
                                <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" viewBox="0 0 20 20" fill="currentColor">
                                    <path fillRule="evenodd" d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z" clipRule="evenodd" />
                                </svg>
                            </div>
                        )}
                    </div>

                    {/* Favorite Star - Top Left */}
                    <div
                        className={`absolute top-2 left-2 p-1.5 rounded-full backdrop-blur-sm transition-all duration-200 z-20 ${isFavorite
                            ? 'bg-black/40 text-yellow-400 opacity-100 hover:scale-110'
                            : 'bg-black/20 text-gray-400 opacity-0 group-hover:opacity-100 hover:text-white hover:bg-blue-500/50'
                            }`}
                        onClick={(e) => onToggleFavorite(community.identifier, e)}
                        title={isFavorite ? "Remove from favorites" : "Add to favorites"}
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5 filter drop-shadow-sm" viewBox="0 0 20 20" fill="currentColor">
                            <path d="M9.049 2.927c.3-.921 1.603-.921 1.902 0l1.07 3.292a1 1 0 00.95.69h3.462c.969 0 1.371 1.24.588 1.81l-2.8 2.034a1 1 0 00-.364 1.118l1.07 3.292c.3.921-.755 1.688-1.54 1.118l-2.8-2.034a1 1 0 00-1.175 0l-2.8 2.034c-.784.57-1.838-.197-1.539-1.118l1.07-3.292a1 1 0 00-.364-1.118L2.98 8.72c-.783-.57-.38-1.81.588-1.81h3.461a1 1 0 00.951-.69l1.07-3.292z" />
                        </svg>
                    </div>
                </div>

                {/* Card Content */}
                <div className="absolute bottom-0 left-0 right-0 p-3 z-10 opacity-0 group-hover:opacity-100 transition-opacity duration-300">
                    <h3 className="font-bold text-sm leading-tight line-clamp-2 text-white drop-shadow-md">
                        {community.name}
                    </h3>
                </div>
            </button>
        );
    };

    const filteredCommunities = communities;

    const favorites = filteredCommunities.filter(c => favoriteGames.includes(c.identifier));
    const others = filteredCommunities.filter(c => !favoriteGames.includes(c.identifier));

    return (
        <div className="p-4 pt-0 space-y-8">

            {favorites.length > 0 && (
                <div className="space-y-4">
                    <div className="flex items-center gap-3 text-yellow-400">
                        <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" viewBox="0 0 20 20" fill="currentColor">
                            <path d="M9.049 2.927c.3-.921 1.603-.921 1.902 0l1.07 3.292a1 1 0 00.95.69h3.462c.969 0 1.371 1.24.588 1.81l-2.8 2.034a1 1 0 00-.364 1.118l1.07 3.292c.3.921-.755 1.688-1.54 1.118l-2.8-2.034a1 1 0 00-1.175 0l-2.8 2.034c-.784.57-1.838-.197-1.539-1.118l1.07-3.292a1 1 0 00-.364-1.118L2.98 8.72c-.783-.57-.38-1.81.588-1.81h3.461a1 1 0 00.951-.69l1.07-3.292z" />
                        </svg>
                        <h2 className="text-lg font-bold tracking-wide uppercase text-white/90">Favorites</h2>
                    </div>
                    <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6 gap-4">
                        {favorites.map(renderCommunityCard)}
                    </div>

                    {/* Divider */}
                    {others.length > 0 && (
                        <div className="relative pt-6 pb-2">
                            <div className="absolute inset-0 flex items-center" aria-hidden="true">
                                <div className="w-full border-t border-gray-800"></div>
                            </div>
                        </div>
                    )}
                </div>
            )}

            {others.length > 0 && (
                <div className="space-y-4">
                    {/* Show title only if there are favorites (to distinguish sections) */}
                    {favorites.length > 0 ? null : null}
                    <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6 gap-4">
                        {others.map(renderCommunityCard)}
                    </div>
                </div>
            )}

            {filteredCommunities.length === 0 && (
                <div className="text-center py-12 text-gray-500">
                    No games found matching your filters/search.
                </div>
            )}
        </div>
    );
}
