import { useEffect, useMemo, useRef, useState } from 'react';

import { collectCommands, useCommandStore } from '../store/useCommandStore';
import {
    buildSections,
    flattenSections,
    moveHighlight,
    parseQuery,
    type CommandItem,
} from '../utils/commandPalette';

/**
 * One search field for the whole app.
 *
 * Reachable from anywhere, including the home screen, because what it offers
 * comes from whichever views happen to be mounted rather than from props
 * threaded down from one place.
 */

function ItemIcon({ kind }: { kind: CommandItem['icon'] }) {
    const paths: Record<NonNullable<CommandItem['icon']>, React.ReactNode> = {
        play: <path strokeLinecap="round" strokeLinejoin="round" d="M6.5 5.5v13l11-6.5-11-6.5z" />,
        stop: <rect x="6.5" y="6.5" width="11" height="11" rx="2" />,
        apply: <path strokeLinecap="round" strokeLinejoin="round" d="M5 13l4 4L19 7" />,
        profile: (
            <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M12 12a4 4 0 100-8 4 4 0 000 8zm-7 8a7 7 0 0114 0"
            />
        ),
        game: (
            <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M7 9v4m-2-2h4m6 0h.01M17 13h.01M6.5 7h11a3.5 3.5 0 013.46 4l-.8 5a3.5 3.5 0 01-6.1 1.7l-.9-1a2 2 0 00-2.9 0l-.9 1A3.5 3.5 0 013.84 16l-.8-5A3.5 3.5 0 016.5 7z"
            />
        ),
        settings: (
            <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M10.3 4.3a1 1 0 011-.8h1.4a1 1 0 011 .8l.2 1.2a6.6 6.6 0 011.6.9l1.1-.4a1 1 0 011.2.4l.7 1.2a1 1 0 01-.2 1.3l-1 .8a6.6 6.6 0 010 1.8l1 .8a1 1 0 01.2 1.3l-.7 1.2a1 1 0 01-1.2.4l-1.1-.4a6.6 6.6 0 01-1.6.9l-.2 1.2a1 1 0 01-1 .8h-1.4a1 1 0 01-1-.8l-.2-1.2a6.6 6.6 0 01-1.6-.9l-1.1.4a1 1 0 01-1.2-.4l-.7-1.2a1 1 0 01.2-1.3l1-.8a6.6 6.6 0 010-1.8l-1-.8a1 1 0 01-.2-1.3l.7-1.2a1 1 0 011.2-.4l1.1.4a6.6 6.6 0 011.6-.9l.2-1.2zM12 14.2a2.2 2.2 0 100-4.4 2.2 2.2 0 000 4.4z"
            />
        ),
        theme: (
            <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M12 3a9 9 0 000 18h1.5a2 2 0 001.6-3.2 2 2 0 011.6-3.2H19a2 2 0 002-2A9 9 0 0012 3z"
            />
        ),
        keyboard: (
            <>
                <rect x="2.5" y="6" width="19" height="12" rx="2" />
                <path strokeLinecap="round" d="M6 9.5h.01M9.5 9.5h.01M13 9.5h.01M16.5 9.5h.01M8 15.5h8" />
            </>
        ),
        plus: <path strokeLinecap="round" strokeLinejoin="round" d="M12 5v14m7-7H5" />,
        copy: (
            <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M9 9V6a2 2 0 012-2h7a2 2 0 012 2v7a2 2 0 01-2 2h-3M6 9h7a2 2 0 012 2v7a2 2 0 01-2 2H6a2 2 0 01-2-2v-7a2 2 0 012-2z"
            />
        ),
        browse: (
            <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M4 5.5A1.5 1.5 0 015.5 4h3A1.5 1.5 0 0110 5.5v3A1.5 1.5 0 018.5 10h-3A1.5 1.5 0 014 8.5v-3zm10 0A1.5 1.5 0 0115.5 4h3A1.5 1.5 0 0120 5.5v3A1.5 1.5 0 0118.5 10h-3A1.5 1.5 0 0114 8.5v-3zM4 15.5A1.5 1.5 0 015.5 14h3a1.5 1.5 0 011.5 1.5v3A1.5 1.5 0 018.5 20h-3A1.5 1.5 0 014 18.5v-3zm10 0a1.5 1.5 0 011.5-1.5h3a1.5 1.5 0 011.5 1.5v3a1.5 1.5 0 01-1.5 1.5h-3a1.5 1.5 0 01-1.5-1.5v-3z"
            />
        ),
    };

    return (
        <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg border border-gray-700 bg-gray-800 text-gray-400">
            <svg className="h-[18px] w-[18px]" fill="none" stroke="currentColor" strokeWidth={1.6} viewBox="0 0 24 24">
                {paths[kind ?? 'settings']}
            </svg>
        </span>
    );
}

function Hint({ children }: { children: React.ReactNode }) {
    return <span className="rounded-md border border-gray-700 px-1.5 py-0.5 text-[11px] text-gray-500">{children}</span>;
}

export function CommandPalette() {
    const isOpen = useCommandStore((state) => state.isOpen);
    const close = useCommandStore((state) => state.close);
    const providers = useCommandStore((state) => state.providers);
    const scope = useCommandStore((state) => state.scope);

    const [query, setQuery] = useState('');
    const [highlighted, setHighlighted] = useState(0);
    const inputRef = useRef<HTMLInputElement>(null);
    const listRef = useRef<HTMLDivElement>(null);

    // Sources are asked for their items only while the palette is up, so a
    // closed palette costs nothing on every render of the app behind it.
    const sections = useMemo(
        () => (isOpen ? buildSections(collectCommands(providers), query, scope) : []),
        [isOpen, providers, query, scope]
    );
    const flat = useMemo(() => flattenSections(sections), [sections]);
    // Scoped searching has no commands, so a leading slash is just a character.
    const slashMode = !scope && parseQuery(query).slashMode;

    // Each opening starts clean, decided during render so the first paint never
    // shows the previous search.
    const [wasOpen, setWasOpen] = useState(isOpen);
    if (wasOpen !== isOpen) {
        setWasOpen(isOpen);
        setQuery('');
        setHighlighted(0);
    }

    useEffect(() => {
        if (!isOpen) return;
        const raf = requestAnimationFrame(() => inputRef.current?.focus());
        return () => cancelAnimationFrame(raf);
    }, [isOpen]);

    // A shrinking result list must not leave the highlight past the end.
    const clamped = Math.min(highlighted, Math.max(0, flat.length - 1));
    if (clamped !== highlighted) setHighlighted(clamped);

    useEffect(() => {
        listRef.current?.querySelector('[data-highlighted="true"]')?.scrollIntoView({ block: 'nearest' });
    }, [clamped, flat.length]);

    if (!isOpen) return null;

    const runItem = (item: CommandItem) => {
        // Closed first, so an action that opens a panel is not drawn behind it.
        close();
        item.run();
    };

    const handleKeyDown = (event: React.KeyboardEvent) => {
        if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
            event.preventDefault();
            setHighlighted((current) => moveHighlight(current, event.key === 'ArrowDown' ? 1 : -1, flat.length));
            return;
        }

        if (event.key === 'Enter') {
            event.preventDefault();
            const chosen = flat[clamped];
            if (chosen) runItem(chosen);
            return;
        }

        if (event.key === 'Escape') {
            event.preventDefault();
            // Stopped here so the app-wide Escape does not also step back out of
            // whatever is behind the palette.
            event.stopPropagation();
            close();
        }
    };

    let index = -1;

    return (
        <div className="fixed inset-0 z-[80] flex items-start justify-center p-4 pt-[14vh]">
            <div className="absolute inset-0 bg-black/60 backdrop-blur-sm" onClick={close} />

            <div
                className="relative flex max-h-[64vh] w-full max-w-[600px] flex-col overflow-hidden rounded-2xl border border-gray-700 bg-gray-900 shadow-2xl"
                onClick={(e) => e.stopPropagation()}
            >
                <div className="flex shrink-0 items-center gap-3 border-b border-gray-800 px-5 py-4">
                    {/* One magnifier, tinted when the query has turned into a
                        command. Swapping it for a slash glyph put a second
                        slash beside the one being typed. */}
                    <svg
                        className={`h-5 w-5 shrink-0 transition-colors ${
                            slashMode ? 'text-fg-accent' : query ? 'text-blue-500' : 'text-gray-500'
                        }`}
                        fill="none"
                        stroke="currentColor"
                        strokeWidth={2}
                        viewBox="0 0 24 24"
                    >
                        <path strokeLinecap="round" strokeLinejoin="round" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
                    </svg>
                    <input
                        ref={inputRef}
                        value={query}
                        onChange={(e) => {
                            setQuery(e.target.value);
                            setHighlighted(0);
                        }}
                        onKeyDown={handleKeyDown}
                        placeholder={
                            scope === 'Profiles'
                                ? 'Find a profile...'
                                : 'Search games, profiles and settings, or type / for commands'
                        }
                        spellCheck={false}
                        autoCorrect="off"
                        autoCapitalize="none"
                        autoComplete="off"
                        className="w-full bg-transparent text-[15px] text-white placeholder-gray-500 focus:outline-none"
                        aria-label="Search"
                    />
                </div>

                <div ref={listRef} className="min-h-0 flex-1 overflow-y-auto p-2">
                    {sections.length === 0 ? (
                        <p className="px-3 py-10 text-center text-[13px] text-gray-500">
                            {slashMode
                                ? 'No command by that name.'
                                : scope === 'Profiles'
                                  ? 'No profile matches that.'
                                  : 'Nothing matches that.'}
                        </p>
                    ) : (
                        sections.map((section) => (
                            <div key={section.group} className="mb-1 last:mb-0">
                                {!scope && (
                                    <h3 className="px-3 pb-1 pt-2 text-[11px] font-semibold uppercase tracking-widest text-gray-500">
                                        {section.group}
                                    </h3>
                                )}

                                {section.items.map((item) => {
                                    index += 1;
                                    const isHighlighted = index === clamped;
                                    const position = index;

                                    return (
                                        <button
                                            key={item.id}
                                            type="button"
                                            data-highlighted={isHighlighted}
                                            onMouseMove={() => setHighlighted(position)}
                                            onClick={() => runItem(item)}
                                            className={`flex w-full items-center gap-3 rounded-xl px-3 py-2.5 text-left transition-colors ${
                                                isHighlighted ? 'bg-gray-800' : 'hover:bg-gray-800/60'
                                            }`}
                                        >
                                            <ItemIcon kind={item.icon} />

                                            <span className="min-w-0 flex-1">
                                                <span className="block truncate text-[14px] font-medium text-white">
                                                    {item.title}
                                                </span>
                                                {item.subtitle && (
                                                    <span className="block truncate text-[12px] text-gray-500">
                                                        {item.subtitle}
                                                    </span>
                                                )}
                                            </span>

                                            {item.current && <Hint>open</Hint>}
                                            {/* In slash mode the command word is
                                                what was typed, so the shortcut
                                                would only be noise. */}
                                            {slashMode && item.slash ? (
                                                <Hint>/{item.slash}</Hint>
                                            ) : (
                                                item.hint && <Hint>{item.hint}</Hint>
                                            )}
                                        </button>
                                    );
                                })}
                            </div>
                        ))
                    )}
                </div>

                <div className="flex shrink-0 items-center gap-4 border-t border-gray-800 px-5 py-2.5 text-[11px] text-gray-500">
                    <span className="flex items-center gap-1.5">
                        <Hint>↩</Hint> open
                    </span>
                    <span className="flex items-center gap-1.5">
                        <Hint>↑↓</Hint> navigate
                    </span>
                    {!scope && (
                        <span className="flex items-center gap-1.5">
                            <Hint>/</Hint> commands
                        </span>
                    )}
                    <span className="ml-auto flex items-center gap-1.5">
                        <Hint>esc</Hint> close
                    </span>
                </div>
            </div>
        </div>
    );
}
