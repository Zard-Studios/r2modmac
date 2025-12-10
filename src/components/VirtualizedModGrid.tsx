import { useVirtualizer } from '@tanstack/react-virtual';
import React, { useRef, useState, useEffect, useLayoutEffect } from 'react';
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
}

export function VirtualizedModGrid({ packages, installedMods, onInstall, onUninstall, onModClick, onLoadMore, isLoadingMore, hasMore, viewMode = 'grid' }: VirtualizedModGridProps) {
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
            { threshold: 0.1 }
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
                                        />
                                    ) : (
                                        <ModListItem
                                            key={pkg.uuid4}
                                            mod={pkg.versions[0]}
                                            onInstall={() => onInstall(pkg)}
                                            onUninstall={() => onUninstall(pkg)}
                                            onClick={() => onModClick(pkg)}
                                            installStatus={getInstallStatus(pkg)}
                                        />
                                    )
                                ))}
                            </div>
                        </div>
                    );
                })}
            </div>

            {hasMore && (
                <div ref={loadMoreRef} className="h-20" />
            )}

            {isLoadingMore && hasMore && packages.length > 0 && (
                <div className="flex items-center justify-center py-6 gap-3">
                    <svg className="animate-spin h-5 w-5 text-blue-500" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
                        <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4"></circle>
                        <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                    </svg>
                    <span className="text-gray-400 text-sm">Loading more mods...</span>
                </div>
            )}
        </div>
    );
}
