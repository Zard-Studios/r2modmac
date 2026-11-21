import { useCallback, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from 'react';
import { readScrollFades, ScrollFades } from './ScrollFades';

/** One choice in a {@link PickerMenu}. */
export interface PickerItem {
    key: string;
    /** What is shown for it, usually an icon and a name. */
    content: ReactNode;
    /** A small note on the right, such as "Loading…". */
    hint?: ReactNode;
    /** Tooltip. */
    title?: string;
    disabled?: boolean;
}

const TRIGGER_SIZES = {
    md: 'gap-2.5 px-2.5 py-1.5 text-sm',
    sm: 'gap-2 px-2 py-1 text-xs',
};
const ITEM_SIZES = {
    md: 'px-3 py-2 text-sm',
    sm: 'px-2.5 py-1.5 text-xs',
};

/**
 * A drop-down for choosing one of a few options, each with an icon and a name:
 * the version of a mod and the source of its README both use it. It closes on a
 * click outside or on Escape, and fades the edges when the list scrolls.
 */
export function PickerMenu({
    trigger,
    triggerLabel,
    items,
    selectedKey,
    onSelect,
    size = 'md',
}: {
    /** What the closed menu shows for the current choice. */
    trigger: ReactNode;
    triggerLabel: string;
    items: PickerItem[];
    selectedKey: string;
    onSelect: (key: string) => void;
    size?: keyof typeof TRIGGER_SIZES;
}) {
    const [isOpen, setIsOpen] = useState(false);
    const rootRef = useRef<HTMLDivElement>(null);
    const scrollRef = useRef<HTMLDivElement>(null);
    const [scrollFades, setScrollFades] = useState({ top: false, bottom: false });

    const updateScrollFades = useCallback((element: HTMLDivElement) => {
        const next = readScrollFades(element);
        setScrollFades(current => current.top === next.top && current.bottom === next.bottom ? current : next);
    }, []);

    useLayoutEffect(() => {
        if (!isOpen || !scrollRef.current) return;
        const element = scrollRef.current;
        updateScrollFades(element);
        const resizeObserver = new ResizeObserver(() => updateScrollFades(element));
        resizeObserver.observe(element);
        return () => resizeObserver.disconnect();
    }, [isOpen, items.length, updateScrollFades]);

    useEffect(() => {
        if (!isOpen) return;
        const close = (event: PointerEvent) => {
            if (!rootRef.current?.contains(event.target as Node)) setIsOpen(false);
        };
        const closeOnEscape = (event: KeyboardEvent) => {
            if (event.key === 'Escape') setIsOpen(false);
        };
        document.addEventListener('pointerdown', close);
        document.addEventListener('keydown', closeOnEscape);
        return () => {
            document.removeEventListener('pointerdown', close);
            document.removeEventListener('keydown', closeOnEscape);
        };
    }, [isOpen]);

    return (
        <div ref={rootRef} className="relative inline-block w-fit max-w-full">
            <button
                type="button"
                onClick={() => setIsOpen(open => !open)}
                className={`flex max-w-full items-center rounded-lg border border-gray-700 bg-gray-800 text-gray-200 transition-colors hover:border-gray-600 focus:border-blue-500 focus:outline-none ${TRIGGER_SIZES[size]}`}
                aria-haspopup="listbox"
                aria-expanded={isOpen}
                aria-label={triggerLabel}
            >
                <span className="flex items-center gap-2">{trigger}</span>
                <svg className={`h-4 w-4 flex-shrink-0 text-gray-500 transition-transform ${isOpen ? 'rotate-180' : ''}`} viewBox="0 0 20 20" fill="currentColor" aria-hidden="true">
                    <path fillRule="evenodd" d="M5.23 7.21a.75.75 0 0 1 1.06.02L10 11.19l3.71-3.96a.75.75 0 1 1 1.1 1.02l-4.25 4.54a.75.75 0 0 1-1.1 0L5.21 8.25a.75.75 0 0 1 .02-1.04Z" clipRule="evenodd" />
                </svg>
            </button>
            {isOpen && (
                <div
                    className="absolute left-0 top-full z-30 w-max min-w-full max-w-[min(22rem,calc(100vw-3rem))] overflow-hidden rounded-lg border border-gray-700 bg-gray-800 shadow-xl"
                    role="listbox"
                >
                    <div
                        ref={scrollRef}
                        onScroll={event => updateScrollFades(event.currentTarget)}
                        className="mod-version-scroll w-full overflow-y-auto overscroll-contain"
                        style={{ maxHeight: 'min(22rem, 45dvh)' }}
                    >
                        {items.map(item => {
                            const active = item.key === selectedKey;
                            return (
                                <button
                                    type="button"
                                    key={item.key}
                                    disabled={item.disabled}
                                    onClick={() => {
                                        onSelect(item.key);
                                        setIsOpen(false);
                                    }}
                                    className={`flex w-full items-center gap-2 whitespace-nowrap text-left transition-colors ${ITEM_SIZES[size]} ${
                                        item.disabled
                                            ? 'cursor-not-allowed text-gray-500'
                                            : active
                                                ? 'bg-blue-500/15 text-fg-accent'
                                                : 'text-gray-200 hover:bg-gray-700'
                                    }`}
                                    role="option"
                                    aria-selected={active}
                                    title={item.title}
                                >
                                    {item.content}
                                    {item.hint && <span className="ml-auto pl-4 text-xs text-gray-500">{item.hint}</span>}
                                </button>
                            );
                        })}
                    </div>
                    <ScrollFades {...scrollFades} surface="gray-800" />
                </div>
            )}
        </div>
    );
}
