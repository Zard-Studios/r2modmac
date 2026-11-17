import { useVirtualizer } from '@tanstack/react-virtual';
import { memo, useRef, useState, useEffect, useLayoutEffect, useMemo, useCallback } from 'react';
import { flushSync } from 'react-dom';
import { ModCard } from './ModCard';
import { ModListItem } from './ModListItem';
import type { Package } from '../types/thunderstore';
import type { InstalledMod } from '../types/profile';
import { packageIdentityKey } from '../utils/modVersioning';

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
const GRID_REFLOW_DURATION = 260;
const GRID_REFLOW_EASING = 'cubic-bezier(0.32, 0.72, 0, 1)';

type InstallStatus = 'installed' | 'not_installed' | 'update_available';

interface ModEntryProps {
    pkg: Package;
    installStatus: InstallStatus;
    isBrowsing?: boolean;
    legacyInstallMode: boolean;
    onInstall: (pkg: Package) => void;
    onUninstall: (pkg: Package) => void;
    onModClick: (pkg: Package) => void;
}

// The cards used to receive `() => onInstall(pkg)` closures built inline by the
// grid, so every grid render handed every card brand-new props and React.memo on
// ModCard never skipped anything. These wrappers take the package plus handlers
// that are stable for the life of the grid, and only re-render when the package,
// its install status or a display flag actually changes.
const GridModCard = memo(function GridModCard({ pkg, installStatus, isBrowsing, legacyInstallMode, onInstall, onUninstall, onModClick }: ModEntryProps) {
    const install = useCallback(() => onInstall(pkg), [onInstall, pkg]);
    const uninstall = useCallback(() => onUninstall(pkg), [onUninstall, pkg]);
    const open = useCallback(() => onModClick(pkg), [onModClick, pkg]);
    return (
        <ModCard
            mod={pkg.versions[0]}
            likesCount={pkg.rating_score}
            totalDownloads={pkg.total_downloads}
            onInstall={install}
            onUninstall={uninstall}
            onClick={open}
            installStatus={installStatus}
            isBrowsing={isBrowsing}
            legacyInstallMode={legacyInstallMode}
        />
    );
});

const ListModItem = memo(function ListModItem({ pkg, installStatus, isBrowsing, legacyInstallMode, onInstall, onUninstall, onModClick }: ModEntryProps) {
    const install = useCallback(() => onInstall(pkg), [onInstall, pkg]);
    const uninstall = useCallback(() => onUninstall(pkg), [onUninstall, pkg]);
    const open = useCallback(() => onModClick(pkg), [onModClick, pkg]);
    return (
        <ModListItem
            mod={pkg.versions[0]}
            likesCount={pkg.rating_score}
            totalDownloads={pkg.total_downloads}
            onInstall={install}
            onUninstall={uninstall}
            onClick={open}
            installStatus={installStatus}
            isBrowsing={isBrowsing}
            legacyInstallMode={legacyInstallMode}
        />
    );
});

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

    // The parent hands over fresh handler functions on most renders. Route them
    // through one stable function each so memoised cards are not invalidated.
    const handlersRef = useRef({ onInstall, onUninstall, onModClick });
    useLayoutEffect(() => {
        handlersRef.current = { onInstall, onUninstall, onModClick };
    });
    const stableInstall = useCallback((pkg: Package) => handlersRef.current.onInstall(pkg), []);
    const stableUninstall = useCallback((pkg: Package) => handlersRef.current.onUninstall(pkg), []);
    const stableModClick = useCallback((pkg: Package) => handlersRef.current.onModClick(pkg), []);
    const [columnCount, setColumnCount] = useState(3);
    const columnCountRef = useRef(3);

    // Scroll to top when search query changes
    const prevSearchQuery = useRef(searchQuery);
    useEffect(() => {
        if (searchQuery !== prevSearchQuery.current && parentRef.current) {
            parentRef.current.scrollTop = 0;
        }
        prevSearchQuery.current = searchQuery;
    }, [searchQuery]);

    // Indexed once per change to the installed set rather than scanned per card.
    // With the grid virtualised there are only ~30 cards on screen, but each one
    // walked the whole installed list on every render, so a 300-mod profile paid
    // ~9000 string comparisons per frame while scrolling.
    //
    // The key is the app's own notion of "same mod" (name without version), the
    // one the profile store already dedupes by. It also drops a latent false
    // positive in the old prefix test, where "Author-Mod" matched an unrelated
    // "Author-ModExtra".
    const installedByIdentity = useMemo(() => {
        const index = new Map<string, InstalledMod>();
        for (const mod of installedMods) index.set(packageIdentityKey(mod.fullName), mod);
        return index;
    }, [installedMods]);

    const getInstallStatus = useCallback((pkg: Package): 'installed' | 'not_installed' | 'update_available' => {
        const installed = installedByIdentity.get(packageIdentityKey(pkg.full_name));
        if (!installed) return 'not_installed';
        if (installed.versionNumber !== pkg.versions[0].version_number) return 'update_available';
        return 'installed';
    }, [installedByIdentity]);

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
                const width = parentRef.current.offsetWidth - 32;
                const cols = Math.max(1, Math.min(6, Math.floor(width / (COLUMN_WIDTH + GAP))));
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
        const collectCardRects = () => {
            const rects = new Map<string, DOMRect>();
            const cards = parentRef.current?.querySelectorAll<HTMLElement>('[data-mod-grid-id]');
            cards?.forEach(card => {
                const id = card.dataset.modGridId;
                if (id) rects.set(id, card.getBoundingClientRect());
            });
            return rects;
        };

        const updateColumnCount = (animate: boolean) => {
            if (!parentRef.current) return;
            if (viewMode === 'list') {
                columnCountRef.current = 1;
                setColumnCount(1);
                return;
            }
            const width = parentRef.current.offsetWidth - 32;
            const cols = Math.max(1, Math.min(6, Math.floor(width / (COLUMN_WIDTH + GAP))));
            if (columnCountRef.current === cols) return;

            const shouldAnimate = animate
                && !window.matchMedia('(prefers-reduced-motion: reduce)').matches
                && typeof Element.prototype.animate === 'function';
            const previousRects = shouldAnimate ? collectCardRects() : new Map<string, DOMRect>();

            if (shouldAnimate) {
                parentRef.current.querySelectorAll<HTMLElement>('[data-mod-grid-id]').forEach(card => {
                    card.getAnimations().forEach(animation => animation.cancel());
                });
                flushSync(() => {
                    columnCountRef.current = cols;
                    setColumnCount(cols);
                });
            } else {
                columnCountRef.current = cols;
                setColumnCount(cols);
            }

            if (!shouldAnimate) return;

            parentRef.current.querySelectorAll<HTMLElement>('[data-mod-grid-id]').forEach(card => {
                const id = card.dataset.modGridId;
                const previousRect = id ? previousRects.get(id) : undefined;
                if (!previousRect) {
                    card.animate(
                        [
                            { opacity: 0, transform: 'translate3d(0, 8px, 0) scale(0.985)' },
                            { opacity: 1, transform: 'translate3d(0, 0, 0) scale(1)' },
                        ],
                        { duration: 180, easing: GRID_REFLOW_EASING },
                    );
                    return;
                }

                const nextRect = card.getBoundingClientRect();
                const translateX = previousRect.left - nextRect.left;
                const translateY = previousRect.top - nextRect.top;
                const scaleX = nextRect.width > 0 ? previousRect.width / nextRect.width : 1;
                const scaleY = nextRect.height > 0 ? previousRect.height / nextRect.height : 1;

                card.animate(
                    [
                        {
                            transform: `translate3d(${translateX}px, ${translateY}px, 0) scale(${scaleX}, ${scaleY})`,
                            transformOrigin: 'top left',
                        },
                        {
                            transform: 'translate3d(0, 0, 0) scale(1)',
                            transformOrigin: 'top left',
                        },
                    ],
                    { duration: GRID_REFLOW_DURATION, easing: GRID_REFLOW_EASING },
                );
            });
        };

        updateColumnCount(false);

        const resizeObserver = new ResizeObserver(() => {
            updateColumnCount(true);
        });

        if (parentRef.current) {
            resizeObserver.observe(parentRef.current);
        }

        return () => {
            resizeObserver.disconnect();
        };
    }, [viewMode]);

    const gridMaximumWidth = columnCount * 420 + GAP * (columnCount - 1);

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
            <div className="relative flex-1 min-h-0">
                <div
                    ref={parentRef}
                    className="h-full overflow-y-auto px-[clamp(1rem,3vw,3.125rem)] pt-[clamp(1rem,3vw,3.125rem)] pb-32"
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
                                            gridTemplateColumns: `repeat(${columnCount}, minmax(0, 1fr))`,
                                            justifyContent: 'center',
                                            marginInline: 'auto',
                                            maxWidth: `${gridMaximumWidth}px`,
                                            paddingBottom: GAP,
                                        }}
                                    >
                                        {rowPkgs.map((pkg) => (
                                            <div key={pkg.uuid4} data-mod-grid-id={pkg.uuid4} className="min-w-0">
                                                <GridModCard
                                                    pkg={pkg}
                                                    installStatus={getInstallStatus(pkg)}
                                                    isBrowsing={isBrowsing}
                                                    legacyInstallMode={legacyInstallMode}
                                                    onInstall={stableInstall}
                                                    onUninstall={stableUninstall}
                                                    onModClick={stableModClick}
                                                />
                                            </div>
                                        ))}
                                    </div>
                                )}
                            </div>
                        );
                    })}
                    </div>
                </div>
            </div>
        );
    }

    return (
        <div className="relative flex-1 min-h-0">
            <div
                ref={parentRef}
                className="h-full overflow-y-auto px-[clamp(1rem,3vw,3.125rem)] pt-[clamp(1rem,3vw,3.125rem)] pb-32"
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
                            <ListModItem
                                key={pkg.uuid4}
                                pkg={pkg}
                                installStatus={getInstallStatus(pkg)}
                                isBrowsing={isBrowsing}
                                legacyInstallMode={legacyInstallMode}
                                onInstall={stableInstall}
                                onUninstall={stableUninstall}
                                onModClick={stableModClick}
                            />
                        </div>
                    );
                })}
                </div>
            </div>
        </div>
    );
}
