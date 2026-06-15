import { useVirtualizer } from '@tanstack/react-virtual';
import { useRef, useState, useEffect, useLayoutEffect, useMemo, useCallback } from 'react';
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
    viewMode?: 'grid' | 'list';
    isBrowsing?: boolean;
    searchQuery?: string; // For scroll-to-top on search
    legacyInstallMode?: boolean;
    onLoadMore?: () => void;
    hasMore?: boolean;
    isLoadingMore?: boolean;
    totalCount?: number;
}

const COLUMN_WIDTH = 320;
const GAP = 16;
const GRID_ROW_HEIGHT = 224;
const LIST_ROW_HEIGHT = 80;

// Split flat array into rows of `cols` items
function chunkIntoRows<T>(items: T[], cols: number): T[][] {
    const rows: T[][] = [];
    for (let i = 0; i < items.length; i += cols) {
        rows.push(items.slice(i, i + cols));
    }
    return rows;
}

export function VirtualizedModGrid({ packages, installedMods, onInstall, onUninstall, onModClick, viewMode = 'grid', isBrowsing, searchQuery, legacyInstallMode = false, onLoadMore, hasMore, isLoadingMore }: VirtualizedModGridProps) {
    const parentRef = useRef<HTMLDivElement>(null);
    const [columnCount, setColumnCount] = useState(3);
    const [availableWidth, setAvailableWidth] = useState(0);

    // Scroll to top when search query changes
    const prevSearchQuery = useRef(searchQuery);
    useEffect(() => {
        if (searchQuery !== prevSearchQuery.current && parentRef.current) {
            parentRef.current.scrollTop = 0;
        }
        prevSearchQuery.current = searchQuery;
    }, [searchQuery]);

    // Helper to check install status
    const getInstallStatus = useCallback((pkg: Package): 'installed' | 'not_installed' | 'update_available' => {
        const installed = installedMods.find(m => m.fullName.startsWith(pkg.full_name));
        if (!installed) return 'not_installed';
        if (installed.versionNumber !== pkg.versions[0].version_number) return 'update_available';
        return 'installed';
    }, [installedMods]);

    // Scroll Synchronization on viewMode changes
    const prevViewMode = useRef(viewMode);

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
                const width = parentRef.current.offsetWidth - 100;
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
                setAvailableWidth(0);
                return;
            }
            const width = parentRef.current.offsetWidth - 100;
            const cols = Math.max(1, Math.min(3, Math.floor(width / (COLUMN_WIDTH + GAP))));
            setColumnCount(cols);
            setAvailableWidth(width);
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

    const gridColumnWidth = viewMode === 'grid' && columnCount > 0
        ? Math.min(420, Math.floor((availableWidth - GAP * (columnCount - 1)) / columnCount))
        : COLUMN_WIDTH;

    // For grid: chunk packages into rows so we can virtualize row-by-row
    const gridRows = useMemo(
        () => viewMode === 'grid' ? chunkIntoRows(packages, columnCount) : [],
        [packages, columnCount, viewMode]
    );

    const gridRowCount = viewMode === 'grid' ? gridRows.length + (hasMore ? 1 : 0) : 0;
    const gridRowVirtualizer = useVirtualizer({
        count: gridRowCount,
        getScrollElement: () => parentRef.current,
        estimateSize: () => GRID_ROW_HEIGHT + GAP,
        overscan: 3,
        measureElement: (element) =>
            element?.getBoundingClientRect().height ?? GRID_ROW_HEIGHT + GAP,
    });

    const listRowCount = viewMode === 'list' ? packages.length + (hasMore ? 1 : 0) : 0;
    const listRowVirtualizer = useVirtualizer({
        count: listRowCount,
        getScrollElement: () => parentRef.current,
        estimateSize: () => LIST_ROW_HEIGHT,
        overscan: 5,
        measureElement: (element) =>
            element?.getBoundingClientRect().height ?? LIST_ROW_HEIGHT,
    });

    const virtualItems = viewMode === 'grid' ? gridRowVirtualizer.getVirtualItems() : listRowVirtualizer.getVirtualItems();
    useEffect(() => {
        const lastItem = virtualItems[virtualItems.length - 1];
        if (!lastItem) return;

        const isNearEnd = lastItem.index >= (viewMode === 'grid' ? gridRows.length - 1 : packages.length - 1);
        if (isNearEnd && hasMore && !isLoadingMore && onLoadMore) {
            onLoadMore();
        }
    }, [virtualItems, hasMore, isLoadingMore, onLoadMore, viewMode, gridRows.length, packages.length]);

    if (viewMode === 'grid') {
        return (
            <div
                ref={parentRef}
                className="flex-1 h-full overflow-y-auto px-[50px] pt-[50px] pb-0"
            >
                {/* Virtual grid: only visible rows are in the DOM */}
                <div style={{ height: `${gridRowVirtualizer.getTotalSize()}px`, position: 'relative' }}>
                    {gridRowVirtualizer.getVirtualItems().map((virtualRow) => {
                        const isLoaderRow = virtualRow.index >= gridRows.length;
                        const rowPkgs = isLoaderRow ? [] : gridRows[virtualRow.index];
                        return (
                            <div
                                key={virtualRow.key}
                                ref={gridRowVirtualizer.measureElement}
                                data-index={virtualRow.index}
                                style={{
                                    position: 'absolute',
                                    top: 0,
                                    left: 0,
                                    width: '100%',
                                    transform: `translateY(${virtualRow.start}px)`,
                                }}
                            >
                                {isLoaderRow ? (
                                    <div className="flex justify-center items-center py-8">
                                        <div className="w-8 h-8 border-4 border-blue-500 border-t-transparent rounded-full animate-spin"></div>
                                    </div>
                                ) : (
                                    <div
                                        className="grid gap-4"
                                        style={{
                                            gridTemplateColumns: `repeat(${columnCount}, minmax(0, ${gridColumnWidth}px))`,
                                            justifyContent: 'start',
                                            paddingBottom: GAP,
                                        }}
                                    >
                                        {rowPkgs.map((pkg) => (
                                            <ModCard
                                                key={pkg.uuid4}
                                                mod={pkg.versions[0]}
                                                likesCount={pkg.rating_score}
                                                onInstall={() => onInstall(pkg)}
                                                onUninstall={() => onUninstall(pkg)}
                                                onClick={() => onModClick(pkg)}
                                                installStatus={getInstallStatus(pkg)}
                                                isBrowsing={isBrowsing}
                                                legacyInstallMode={legacyInstallMode}
                                            />
                                        ))}
                                    </div>
                                )}
                            </div>
                        );
                    })}
                </div>
            </div>
        );
    }

    return (
        <div
            ref={parentRef}
            className="flex-1 h-full overflow-y-auto px-[50px] pt-[50px] pb-0"
        >
            <div
                style={{
                    height: `${listRowVirtualizer.getTotalSize()}px`,
                    width: '100%',
                    position: 'relative',
                }}
            >
                {listRowVirtualizer.getVirtualItems().map((virtualRow) => {
                    const isLoaderRow = virtualRow.index >= packages.length;
                    if (isLoaderRow) {
                        return (
                            <div
                                key={virtualRow.key}
                                ref={listRowVirtualizer.measureElement}
                                data-index={virtualRow.index}
                                style={{
                                    position: 'absolute',
                                    top: 0,
                                    left: 0,
                                    width: '100%',
                                    transform: `translateY(${virtualRow.start}px)`,
                                    paddingBottom: '8px',
                                }}
                            >
                                <div className="flex justify-center items-center py-4">
                                    <div className="w-6 h-6 border-2 border-blue-500 border-t-transparent rounded-full animate-spin"></div>
                                </div>
                            </div>
                        );
                    }
                    const pkg = packages[virtualRow.index];
                    return (
                        <div
                            key={virtualRow.key}
                            ref={listRowVirtualizer.measureElement}
                            data-index={virtualRow.index}
                            style={{
                                position: 'absolute',
                                top: 0,
                                left: 0,
                                width: '100%',
                                transform: `translateY(${virtualRow.start}px)`,
                                paddingBottom: '8px',
                            }}
                        >
                            <ModListItem
                                key={pkg.uuid4}
                                mod={pkg.versions[0]}
                                likesCount={pkg.rating_score}
                                onInstall={() => onInstall(pkg)}
                                onUninstall={() => onUninstall(pkg)}
                                onClick={() => onModClick(pkg)}
                                installStatus={getInstallStatus(pkg)}
                                isBrowsing={isBrowsing}
                                legacyInstallMode={legacyInstallMode}
                            />
                        </div>
                    );
                })}
            </div>
        </div>
    );
}
