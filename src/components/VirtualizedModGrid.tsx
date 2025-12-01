import { useVirtualizer } from '@tanstack/react-virtual';
import { useRef, useState, useEffect } from 'react';
import { ModCard } from './ModCard';
import type { Package } from '../types/thunderstore';

interface VirtualizedModGridProps {
    packages: Package[];
    onInstall: (pkg: Package) => void;
    onModClick: (pkg: Package) => void;
    onLoadMore?: () => void;
}

export function VirtualizedModGrid({ packages, onInstall, onModClick, onLoadMore }: VirtualizedModGridProps) {
    const parentRef = useRef<HTMLDivElement>(null);
    const [columnCount, setColumnCount] = useState(3);

    const COLUMN_WIDTH = 320;
    const GAP = 16;

    useEffect(() => {
        const updateColumnCount = () => {
            if (!parentRef.current) return;
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
    }, []);

    const rowCount = Math.ceil(packages.length / columnCount);

    const rowVirtualizer = useVirtualizer({
        count: rowCount,
        getScrollElement: () => parentRef.current,
        estimateSize: () => 280,
        overscan: 2,
    });

    // Infinite scroll detection
    useEffect(() => {
        if (!onLoadMore || !parentRef.current) return;

        const handleScroll = () => {
            const element = parentRef.current;
            if (!element) return;

            const { scrollTop, scrollHeight, clientHeight } = element;
            const scrollPercentage = (scrollTop + clientHeight) / scrollHeight;

            // Trigger load more when 80% scrolled
            if (scrollPercentage > 0.8) {
                onLoadMore();
            }
        };

        const element = parentRef.current;
        element.addEventListener('scroll', handleScroll);
        return () => element.removeEventListener('scroll', handleScroll);
    }, [onLoadMore]);

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
                                className="grid gap-4"
                                style={{
                                    gridTemplateColumns: `repeat(${columnCount}, minmax(0, 1fr))`,
                                }}
                            >
                                {rowPackages.map((pkg) => (
                                    <ModCard
                                        key={pkg.uuid4}
                                        mod={pkg.versions[0]}
                                        onInstall={() => onInstall(pkg)}
                                        onClick={() => onModClick(pkg)}
                                        isInstalled={false}
                                    />
                                ))}
                            </div>
                        </div>
                    );
                })}
            </div>
        </div>
    );
}
