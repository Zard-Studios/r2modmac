import type { MouseEvent } from 'react';
import { memo, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import type { Community, CommunityPlatformInfo } from '../../types/thunderstore';
import { LazyImage } from '../LazyImage';

interface GameSelectorProps {
    communities: Community[];
    selectedCommunity: string | null;
    onSelect: (identifier: string) => void;
    communityImages: Record<string, string>;
    communityPlatforms: Record<string, CommunityPlatformInfo>;
    favoriteGames: string[];
    onToggleFavorite: (identifier: string, e: React.MouseEvent) => void;
    searchQuery: string;
    gridClassName?: string;
    containerClassName?: string;
}


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

const getInitials = (name: string) => {
    return name.split(' ').map(word => word[0]).join('').slice(0, 2).toUpperCase();
};

const getGradient = (str: string) => {
    let hash = 0;
    for (let i = 0; i < str.length; i++) {
        hash = str.charCodeAt(i) + ((hash << 5) - hash);
    }
    return gradients[Math.abs(hash) % gradients.length];
};

// A few Thunderstore communities reuse a game title/slug that does not have a
// cover in the communities index. Keep these fallbacks local and lightweight;
// they are only used when Thunderstore did not return an image URL.
const COMMUNITY_IMAGE_FALLBACKS: Record<string, string> = {
    superhot: 'https://cdn.cloudflare.steamstatic.com/steam/apps/690040/library_600x900_2x.jpg',
    'superhot-mind-control-delete': 'https://cdn.cloudflare.steamstatic.com/steam/apps/690040/library_600x900_2x.jpg',
    'slip-skid': 'https://cdn.cloudflare.steamstatic.com/steam/apps/2651350/library_600x900_2x.jpg',
    'big-walk': 'https://cdn.cloudflare.steamstatic.com/steam/apps/1478500/library_600x900_2x.jpg',
};

const getCommunityImage = (community: Community, images: Record<string, string>) => {
    const direct = images[community.identifier];
    if (direct) return direct;

    const normalizedIdentifier = community.identifier.toLowerCase().trim();
    const normalizedName = community.name.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');
    return COMMUNITY_IMAGE_FALLBACKS[normalizedIdentifier] ?? COMMUNITY_IMAGE_FALLBACKS[normalizedName];
};

interface GameCardProps {
    community: Community;
    isSelected: boolean;
    isFavorite: boolean;
    imageUrl?: string;
    platform?: CommunityPlatformInfo;
    eager?: boolean;
    searchQuery: string;
    onSelect: (identifier: string) => void;
    onToggleFavorite: (identifier: string, e: MouseEvent) => void;
}

export const GameCard = memo(function GameCard({
    community, isSelected, isFavorite, imageUrl, platform, eager = false, searchQuery, onSelect, onToggleFavorite,
}: GameCardProps) {
    const gradient = getGradient(community.name);
    const isWindowsCompatible = platform?.windows ?? true;
    const isMacCompatible = platform?.mac ?? false;
    const showFavoriteAlways = searchQuery.trim().length > 0;
    // The badges below blur whatever is behind them, and that backdrop is the
    // cover art. Swapping the artwork on a card that stays mounted — which is
    // what the theme editor's preview picker does — leaves the blurred layer
    // sampling the picture that was there before, so a badge can sit on a green
    // cover still tinted by the red one. Keying them to the artwork rebuilds the
    // layer, which is the only thing that makes it sample afresh.
    const backdropKey = imageUrl ?? 'no-cover';

    return (
        <button
            onClick={() => onSelect(community.identifier)}
            className={`group relative flex flex-col rounded-xl overflow-hidden text-left border ${isSelected
                ? 'ring-2 ring-blue-500 border-transparent shadow-md shadow-blue-500/20 scale-[1.02] hover:scale-[1.04] hover:-translate-y-1'
                : 'border-gray-700/80 hover:border-gray-500 hover:shadow-xl hover:-translate-y-1'
                } transition-all duration-300 transform-gpu`}
        >
            {/* contain:paint — browser can skip painting off-screen subtrees */}
            <div className="aspect-[3/4] w-full relative overflow-hidden bg-gray-950" style={{ contain: 'paint' }}>
                <div className={`absolute inset-0 bg-gradient-to-br ${gradient} opacity-40`} />

                {imageUrl ? (
                    <LazyImage src={imageUrl} alt={community.name} eager={eager} className="absolute inset-0 z-10" />
                ) : null}

                <div className="absolute inset-0 flex items-center justify-center">
                    <span className="text-4xl font-black text-on-media/20 select-none transform group-hover:scale-110 transition-transform duration-500">
                        {getInitials(community.name)}
                    </span>
                </div>

                <div className="absolute inset-x-0 bottom-0 h-2/3 bg-gradient-to-t from-scrim via-scrim/60 to-transparent opacity-0 group-hover:opacity-90 transition-opacity duration-300 z-20 pointer-events-none" />

                {/* Platform badge — fixed scrim: it sits on cover art, not on a surface. */}
                <div key={backdropKey} className="absolute top-2 right-2 flex flex-col gap-1 z-30 pointer-events-none items-end">
                    <div className={`bg-scrim/75 text-on-media p-1 rounded-full shadow-lg border border-on-media/15 backdrop-blur-sm flex items-center justify-center gap-2 h-7 ${isMacCompatible ? 'px-2' : 'w-7 px-0'}`}>
                        {isWindowsCompatible && (
                            <span title="Windows Compatible" className="flex items-center justify-center w-3.5 h-3.5 shrink-0">
                                <svg xmlns="http://www.w3.org/2000/svg" className="w-[14px] h-[14px] shrink-0" viewBox="0 0 24 24" fill="currentColor">
                                    <path d="M0 3.449L9.75 2.1v9.451H0m10.949-9.602L24 0v11.4H10.949M0 12.6h9.75v9.451L0 20.699M10.949 12.6H24V24l-12.9-1.801" />
                                </svg>
                            </span>
                        )}
                        {isMacCompatible && (
                            <>
                                {isWindowsCompatible && <div className="w-[1px] h-3.5 bg-on-media/25 shrink-0" />}
                                <span title="MacOS Compatible" className="flex items-center justify-center w-3 h-3.5 shrink-0">
                                    <svg xmlns="http://www.w3.org/2000/svg" className="w-[12px] h-[14px] shrink-0" viewBox="0 0 384 512" fill="currentColor">
                                        <path d="M318.7 268.7c-.2-36.7 16.4-64.4 50-84.8-18.8-26.9-47.2-41.7-84.7-44.6-35.5-2.8-74.3 20.7-88.5 20.7-15 0-49.4-19.7-76.4-19.7C63.3 141.2 4 184.8 4 273.5q0 39.3 14.4 81.2c12.8 36.7 59 126.7 107.2 125.2 25.2-.6 43-17.9 75.8-17.9 31.8 0 48.3 17.9 76.4 17.9 48.6-.7 90.4-82.5 102.6-119.3-65.2-30.7-61.7-90-61.7-91.9zm-56.6-164.2c27.3-32.4 24.8-61.9 24-72.5-24.1 1.4-52 16.4-67.9 34.9-17.5 19.8-27.8 44.3-25.6 71.9 26.1 2 49.9-11.4 69.5-34.3z" />
                                    </svg>
                                </span>
                            </>
                        )}
                    </div>
                    {isSelected && (
                        <div className="bg-blue-500 text-on-accent p-1 rounded-full shadow-lg border border-blue-400 z-30 shrink-0">
                            <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4 shrink-0" viewBox="0 0 20 20" fill="currentColor">
                                <path fillRule="evenodd" d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z" clipRule="evenodd" />
                            </svg>
                        </div>
                    )}
                </div>

                {/* Favorite star — fixed scrim, for the same reason as the platform badge. */}
                <div
                    key={backdropKey}
                    className={`absolute top-2 left-2 p-1.5 rounded-full transition-all duration-200 z-20 shadow-md ${isFavorite
                        ? 'bg-scrim/70 border border-on-media/15 backdrop-blur-sm text-yellow-400 opacity-100 hover:scale-110'
                        : showFavoriteAlways
                            ? 'bg-scrim/55 border border-on-media/10 backdrop-blur-sm text-on-media/80 opacity-100 hover:scale-110 hover:text-yellow-400 hover:border-yellow-400/50'
                            : 'bg-scrim/55 border border-on-media/10 backdrop-blur-sm text-on-media/80 opacity-0 group-hover:opacity-100 hover:text-yellow-400 hover:border-yellow-400/50 hover:scale-110'
                        }`}
                    onClick={(e) => onToggleFavorite(community.identifier, e)}
                    title={isFavorite ? "Remove from favorites" : "Add to favorites"}
                >
                    <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5 drop-shadow-sm shrink-0" viewBox="0 0 20 20" fill="currentColor">
                        <path d="M9.049 2.927c.3-.921 1.603-.921 1.902 0l1.07 3.292a1 1 0 00.95.69h3.462c.969 0 1.371 1.24.588 1.81l-2.8 2.034a1 1 0 00-.364 1.118l1.07 3.292c.3.921-.755 1.688-1.54 1.118l-2.8-2.034a1 1 0 00-1.175 0l-2.8 2.034c-.784.57-1.838-.197-1.539-1.118l1.07-3.292a1 1 0 00-.364-1.118L2.98 8.72c-.783-.57-.38-1.81.588-1.81h3.461a1 1 0 00.951-.69l1.07-3.292z" />
                    </svg>
                </div>
            </div>

            <div className="absolute bottom-0 left-0 right-0 p-3 z-30 opacity-0 group-hover:opacity-100 transition-opacity duration-300 pointer-events-none">
                <h3 className="font-bold text-sm leading-tight line-clamp-2 text-on-media drop-shadow-[0_2px_4px_rgba(0,0,0,0.9)]">
                    {community.name}
                </h3>
            </div>
        </button>
    );
});

const GAME_GRID_GAP = 16;

function responsiveColumnCount() {
    const width = window.innerWidth;
    if (width >= 1920) return 10;
    if (width >= 1536) return 8;
    if (width >= 1280) return 7;
    if (width >= 1024) return 6;
    if (width >= 768) return 5;
    if (width >= 640) return 4;
    return 3;
}

type GameSelectorRow =
    | { kind: 'favorites-heading' }
    | { kind: 'divider' }
    | { kind: 'games'; communities: Community[]; favorite: boolean };

function chunkCommunities(communities: Community[], columns: number, favorite: boolean): GameSelectorRow[] {
    const rows: GameSelectorRow[] = [];
    for (let index = 0; index < communities.length; index += columns) {
        rows.push({ kind: 'games', communities: communities.slice(index, index + columns), favorite });
    }
    return rows;
}

export function GameSelector({
    communities,
    selectedCommunity,
    onSelect,
    communityImages,
    communityPlatforms,
    favoriteGames,
    onToggleFavorite,
    searchQuery,
    gridClassName = "grid grid-cols-3 sm:grid-cols-4 md:grid-cols-5 lg:grid-cols-6 xl:grid-cols-7 2xl:grid-cols-8 3xl:grid-cols-10 gap-3.5 sm:gap-4",
    containerClassName = "p-4 pt-0 space-y-8",
}: GameSelectorProps) {
    const containerRef = useRef<HTMLDivElement>(null);
    const virtualListRef = useRef<HTMLDivElement>(null);
    const [columnCount, setColumnCount] = useState(responsiveColumnCount);
    const [containerWidth, setContainerWidth] = useState(0);
    const [scrollMargin, setScrollMargin] = useState(0);
    const favorites = useMemo(
        () => communities.filter(c => favoriteGames.includes(c.identifier)),
        [communities, favoriteGames]
    );
    const others = useMemo(
        () => communities.filter(c => !favoriteGames.includes(c.identifier)),
        [communities, favoriteGames]
    );

    const rows = useMemo<GameSelectorRow[]>(() => {
        const nextRows: GameSelectorRow[] = [];
        if (favorites.length > 0) {
            nextRows.push({ kind: 'favorites-heading' });
            nextRows.push(...chunkCommunities(favorites, columnCount, true));
            if (others.length > 0) nextRows.push({ kind: 'divider' });
        }
        nextRows.push(...chunkCommunities(others, columnCount, false));
        return nextRows;
    }, [columnCount, favorites, others]);

    const getScrollElement = () => (
        containerRef.current?.closest<HTMLElement>('[data-game-selector-scroll]') ?? null
    );

    useLayoutEffect(() => {
        const container = containerRef.current;
        if (!container) return;

        const updateLayout = () => {
            const virtualList = virtualListRef.current;
            const measuredElement = virtualList ?? container;
            const scrollElement = getScrollElement();
            setColumnCount(responsiveColumnCount());
            setContainerWidth(measuredElement.clientWidth);
            if (scrollElement) {
                const containerRect = measuredElement.getBoundingClientRect();
                const scrollRect = scrollElement.getBoundingClientRect();
                setScrollMargin(containerRect.top - scrollRect.top + scrollElement.scrollTop);
            }
        };

        updateLayout();
        const resizeObserver = new ResizeObserver(updateLayout);
        resizeObserver.observe(container);
        if (virtualListRef.current) resizeObserver.observe(virtualListRef.current);
        window.addEventListener('resize', updateLayout);
        return () => {
            resizeObserver.disconnect();
            window.removeEventListener('resize', updateLayout);
        };
    }, []);

    const cardRowHeight = containerWidth > 0
        ? ((containerWidth - GAME_GRID_GAP * (columnCount - 1)) / columnCount) * (4 / 3) + GAME_GRID_GAP
        : 320;
    const rowVirtualizer = useVirtualizer({
        count: rows.length,
        getScrollElement,
        estimateSize: index => {
            const row = rows[index];
            if (row?.kind === 'favorites-heading') return 40;
            if (row?.kind === 'divider') return 48;
            return cardRowHeight;
        },
        overscan: 3,
        scrollMargin,
        getItemKey: index => {
            const row = rows[index];
            if (!row) return index;
            if (row.kind !== 'games') return row.kind;
            return `${row.favorite ? 'favorite' : 'game'}:${row.communities.map(community => community.identifier).join(',')}`;
        },
    });

    return (
        <div ref={containerRef} className={containerClassName}>
            {communities.length === 0 && (
                <div className="text-center py-12 text-gray-500">
                    No games found matching your filters/search.
                </div>
            )}
            {communities.length > 0 && (
                <div ref={virtualListRef} className="relative w-full" style={{ height: `${rowVirtualizer.getTotalSize()}px` }}>
                    {rowVirtualizer.getVirtualItems().map(virtualRow => {
                        const row = rows[virtualRow.index];
                        return (
                            <div
                                key={virtualRow.key}
                                ref={rowVirtualizer.measureElement}
                                data-index={virtualRow.index}
                                className="absolute left-0 top-0 w-full"
                                style={{ transform: `translateY(${virtualRow.start - scrollMargin}px)` }}
                            >
                                {row.kind === 'favorites-heading' ? (
                                    <div className="flex items-center gap-3 pb-4 text-yellow-400">
                                        <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" viewBox="0 0 20 20" fill="currentColor">
                                            <path d="M9.049 2.927c.3-.921 1.603-.921 1.902 0l1.07 3.292a1 1 0 00.95.69h3.462c.969 0 1.371 1.24.588 1.81l-2.8 2.034a1 1 0 00-.364 1.118l1.07 3.292c.3.921-.755 1.688-1.54 1.118l-2.8-2.034a1 1 0 00-1.175 0l-2.8 2.034c-.784.57-1.838-.197-1.539-1.118l1.07-3.292a1 1 0 00-.364-1.118L2.98 8.72c-.783-.57-.38-1.81.588-1.81h3.461a1 1 0 00.951-.69l1.07-3.292z" />
                                        </svg>
                                        <h2 className="text-sm font-bold uppercase tracking-wider text-white/90">Favorites</h2>
                                    </div>
                                ) : row.kind === 'divider' ? (
                                    <div className="relative h-12">
                                        <div className="absolute inset-x-0 top-1/2 border-t border-gray-800" />
                                    </div>
                                ) : (
                                    <div className={`${gridClassName} pb-4`}>
                                        {row.communities.map(community => (
                                            <GameCard
                                                key={community.identifier}
                                                community={community}
                                                isSelected={selectedCommunity === community.identifier}
                                                isFavorite={row.favorite}
                                                imageUrl={getCommunityImage(community, communityImages)}
                                                platform={communityPlatforms[community.identifier]}
                                                eager
                                                searchQuery={searchQuery}
                                                onSelect={onSelect}
                                                onToggleFavorite={onToggleFavorite}
                                            />
                                        ))}
                                    </div>
                                )}
                            </div>
                        );
                    })}
                </div>
            )}
        </div>
    );
}
