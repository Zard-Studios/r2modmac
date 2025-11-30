import type { Community } from '../types/thunderstore';

interface GameSelectorProps {
    communities: Community[];
    selectedCommunity: string | null;
    onSelect: (identifier: string) => void;
    communityImages: Record<string, string>;
}

export function GameSelector({ communities, selectedCommunity, onSelect, communityImages }: GameSelectorProps) {
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
            'from-blue-500 to-indigo-500',
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

    return (
        <div className="p-4 pt-0 space-y-4 h-full flex flex-col">
            <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6 gap-4 overflow-y-auto pr-2 pb-4 pt-2">
                {communities.map(community => {
                    const isSelected = selectedCommunity === community.identifier;
                    const gradient = getGradient(community.name);

                    // Get the real image URL from scraped data or fallback to gradient
                    const imageUrl = communityImages[community.identifier];

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
                            <div className={`aspect-[3/4] w-full relative overflow-hidden bg-gradient-to-br ${gradient}`}>
                                {/* Image with fallback logic */}
                                {imageUrl ? (
                                    <img
                                        src={imageUrl}
                                        alt={community.name}
                                        className="w-full h-full object-cover opacity-0 transition-opacity duration-500"
                                        onLoad={(e) => e.currentTarget.classList.remove('opacity-0')}
                                        onError={(e) => {
                                            // If real image fails, hide it to show gradient
                                            e.currentTarget.style.display = 'none';
                                        }}
                                    />
                                ) : null}

                                {/* Initials Fallback (Always rendered behind img, visible if img fails or loads transparent) */}
                                <div className="absolute inset-0 flex items-center justify-center -z-10">
                                    <span className="text-4xl font-black text-white/20 select-none transform group-hover:scale-110 transition-transform duration-500">
                                        {getInitials(community.name)}
                                    </span>
                                </div>

                                {/* Overlay Gradient for text readability - only visible on hover */}
                                <div className="absolute inset-x-0 bottom-0 h-2/3 bg-gradient-to-t from-gray-900 via-gray-900/60 to-transparent opacity-0 group-hover:opacity-90 transition-opacity duration-300" />

                                {/* Selection Indicator */}
                                {isSelected && (
                                    <div className="absolute top-2 right-2 bg-blue-500 text-white p-1 rounded-full shadow-lg z-10">
                                        <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" viewBox="0 0 20 20" fill="currentColor">
                                            <path fillRule="evenodd" d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z" clipRule="evenodd" />
                                        </svg>
                                    </div>
                                )}
                            </div>

                            {/* Card Content - Overlay on bottom of image, hidden by default, visible on hover */}
                            <div className="absolute bottom-0 left-0 right-0 p-3 z-10 opacity-0 group-hover:opacity-100 transition-opacity duration-300">
                                <h3 className="font-bold text-sm leading-tight line-clamp-2 text-white drop-shadow-md">
                                    {community.name}
                                </h3>
                            </div>
                        </button>
                    );
                })}
            </div>
        </div>
    );
}
