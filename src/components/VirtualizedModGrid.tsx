import { useVirtualizer } from '@tanstack/react-virtual';
import { useRef, useState, useEffect, useLayoutEffect } from 'react';
import { ModCard } from './ModCard';
import { ModListItem } from './ModListItem';
import type { Package } from '../types/thunderstore';
import type { InstalledMod } from '../types/profile';

interface VirtualizedModGridProps {
    packages: Package[];
    installedMods: InstalledMod[];
    onInstall: (pkg: Package) => void;
    onUninstall: (pkg: Package) => void;
    onModClick: (pkg: Package) => void;
    onLoadMore?: () => void;
    isLoadingMore?: boolean;
    hasMore?: boolean;
    viewMode?: 'grid' | 'list';
    isBrowsing?: boolean;
}

export function VirtualizedModGrid({ packages, installedMods, onInstall, onUninstall, onModClick, onLoadMore, isLoadingMore, hasMore, viewMode = 'grid', isBrowsing }: VirtualizedModGridProps) {
    const parentRef = useRef<HTMLDivElement>(null);
    const [columnCount, setColumnCount] = useState(3);

    const COLUMN_WIDTH = 320;
    const GAP = 16;
    const GRID_ROW_HEIGHT = 280;
    const LIST_ROW_HEIGHT = 80;

    // Helper to check install status
    const getInstallStatus = (pkg: Package): 'installed' | 'not_installed' | 'update_available' => {
        const installed = installedMods.find(m => m.fullName.startsWith(pkg.full_name));
        if (!installed) return 'not_installed';

        // Compare versions
        if (installed.versionNumber !== pkg.versions[0].version_number) {
            return 'update_available';
        }

        return 'installed';
    };

    // Scroll Synchronization
    // We synchronize based on viewMode changes
    const prevViewMode = useRef(viewMode);

    // We capture/restore scroll synchronously in useLayoutEffect to avoid flicker
    useLayoutEffect(() => {
        if (prevViewMode.current !== viewMode && parentRef.current) {

            const scrollTop = parentRef.current.scrollTop;
            const oldMode = prevViewMode.current;
            const newMode = viewMode;

            let firstVisibleItemIndex = 0;

            if (oldMode === 'grid') {
                const rowIndex = Math.floor(scrollTop / GRID_ROW_HEIGHT);
                firstVisibleItemIndex = rowIndex * columnCount;
            } else {
                const rowIndex = Math.floor(scrollTop / LIST_ROW_HEIGHT);
                firstVisibleItemIndex = rowIndex;
            }

            let newScrollTop = 0;
            if (newMode === 'grid') {
                // Recalculate cols for safety
                const width = parentRef.current.offsetWidth - 48;
                const cols = Math.max(1, Math.min(3, Math.floor(width / (COLUMN_WIDTH + GAP))));

                const rowIndex = Math.floor(firstVisibleItemIndex / cols);
                newScrollTop = rowIndex * GRID_ROW_HEIGHT;
            } else {
                newScrollTop = firstVisibleItemIndex * LIST_ROW_HEIGHT;
            }

            parentRef.current.scrollTop = newScrollTop;
            prevViewMode.current = newMode;
        }
    }, [viewMode, columnCount]);

    useEffect(() => {
        const updateColumnCount = () => {
            if (!parentRef.current) return;
            if (viewMode === 'list') {
                setColumnCount(1);
                return;
            }
            const width = parentRef.current.offsetWidth - 48;
            const cols = Math.max(1, Math.min(3, Math.floor(width / (COLUMN_WIDTH + GAP))));
            setColumnCount(cols);
        };

        updateColumnCount();

        const resizeObserver = new ResizeObserver(() => {
            updateColumnCount();
        });

        if (parentRef.current) {
            resizeObserver.observe(parentRef.current);
        }

        return () => {
            resizeObserver.disconnect();
        };
    }, [viewMode]);

    const rowCount = Math.ceil(packages.length / columnCount);

    const rowVirtualizer = useVirtualizer({
        count: rowCount,
        getScrollElement: () => parentRef.current,
        estimateSize: () => viewMode === 'list' ? LIST_ROW_HEIGHT : GRID_ROW_HEIGHT,
        overscan: 5,
    });

    // Intersection Observer for infinite scroll
    const loadMoreRef = useRef<HTMLDivElement>(null);

    useEffect(() => {
        if (!onLoadMore || !loadMoreRef.current || !hasMore) return;

        const observer = new IntersectionObserver(
            (entries) => {
                if (entries[0].isIntersecting && hasMore) {
                    onLoadMore();
                }
            },
            { threshold: 0.1, rootMargin: '1000px' }
        );

        observer.observe(loadMoreRef.current);
        return () => observer.disconnect();
    }, [onLoadMore, hasMore]);

    return (
        <div
            ref={parentRef}
            className="flex-1 h-full overflow-y-auto p-6"
        >
            <div
                style={{
                    height: `${rowVirtualizer.getTotalSize()}px`,
                    width: '100%',
                    position: 'relative',
                }}
            >
                {rowVirtualizer.getVirtualItems().map((virtualRow) => {
                    const startIndex = virtualRow.index * columnCount;
                    const endIndex = Math.min(startIndex + columnCount, packages.length);
                    const rowPackages = packages.slice(startIndex, endIndex);

                    return (
                        <div
                            key={virtualRow.key}
                            style={{
                                position: 'absolute',
                                top: 0,
                                left: 0,
                                width: '100%',
                                transform: `translateY(${virtualRow.start}px)`,
                                paddingBottom: '16px',
                            }}
                        >
                            <div
                                className={`grid ${viewMode === 'grid' ? 'gap-4' : 'gap-2'}`}
                                style={{
                                    gridTemplateColumns: viewMode === 'grid'
                                        ? `repeat(${columnCount}, minmax(0, 1fr))`
                                        : 'minmax(0, 1fr)',
                                }}
                            >
                                {rowPackages.map((pkg) => (
                                    viewMode === 'grid' ? (
                                        <ModCard
                                            key={pkg.uuid4}
                                            mod={pkg.versions[0]}
                                            onInstall={() => onInstall(pkg)}
                                            onUninstall={() => onUninstall(pkg)}
                                            onClick={() => onModClick(pkg)}
                                            installStatus={getInstallStatus(pkg)}
                                            isBrowsing={isBrowsing}
                                        />
                                    ) : (
                                        <ModListItem
                                            key={pkg.uuid4}
                                            mod={pkg.versions[0]}
                                            onInstall={() => onInstall(pkg)}
                                            onUninstall={() => onUninstall(pkg)}
                                            onClick={() => onModClick(pkg)}
                                            installStatus={getInstallStatus(pkg)}
                                            isBrowsing={isBrowsing}
                                        />
                                    )
                                ))}
                            </div>
                        </div>
                    );
                })}
            </div>

            <div
                ref={loadMoreRef}
                className="h-10 w-full flex justify-center p-4 min-h-[50px]"
            >
                {isLoadingMore && hasMore && (
                    <div className="flex items-center gap-2 text-zinc-400">
                        <div className="w-4 h-4 border-2 border-zinc-600 border-t-zinc-400 rounded-full animate-spin" />
                        <span>Loading more mods...</span>
                    </div>
                )}
                {!hasMore && packages.length > 0 && (
                    <div className="text-zinc-500 text-sm py-4">
                        You've reached the end :O
                    </div>
                )}
            </div>
        </div>
    );
}
