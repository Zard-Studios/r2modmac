import { useEffect, useMemo, useRef, useState } from 'react';

import { collectCommands, useCommandStore } from '../store/useCommandStore';
import { useKeybindStore } from '../store/useKeybindStore';
import { actionForEvent } from '../utils/keybinds';
import { HoverMarquee } from './ui/HoverMarquee';
import { AppIcon } from './ui/icons';
import {
    buildSections,
    findShortcutItem,
    flattenSections,
    moveHighlight,
    type CommandItem,
    type CommandScope,
} from '../utils/commandPalette';

/**
 * One search field for the whole app.
 *
 * Reachable from anywhere, including the home screen, because what it offers
 * comes from whichever views happen to be mounted rather than from props
 * threaded down from one place.
 */

/**
 * The row's artwork: a cover if there is one, a glyph otherwise, with an
 * optional badge over the corner for the profile a row belongs to.
 */
function ItemArtwork({ item }: { item: CommandItem }) {
    const { image, badge } = item;

    return (
        <span className="relative h-10 w-10 shrink-0">
            {image ? (
                <img
                    src={image}
                    alt=""
                    className="h-10 w-10 rounded-lg border border-gray-700 object-cover"
                />
            ) : (
                <span className="flex h-10 w-10 items-center justify-center rounded-lg border border-gray-700 bg-gray-800 text-gray-400">
                    <AppIcon name={item.icon ?? 'settings'} className="h-[18px] w-[18px]" strokeWidth={1.6} />
                </span>
            )}

            {badge && (
                // Ringed in the panel's own colour so the circle reads as
                // sitting on top of the cover rather than cut out of it.
                <span className="absolute -bottom-1 -right-1 h-[19px] w-[19px] overflow-hidden rounded-full ring-2 ring-gray-900">
                    {badge.image ? (
                        <img src={badge.image} alt="" className="h-full w-full object-cover" />
                    ) : (
                        <span
                            className="flex h-full w-full items-center justify-center text-[10px] font-bold leading-none text-[#ffffff]"
                            style={{ backgroundImage: badge.gradient }}
                        >
                            {badge.initial}
                        </span>
                    )}
                </span>
            )}
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
    const setScope = useCommandStore((state) => state.setScope);
    const keybinds = useKeybindStore((state) => state.keybinds);

    const [query, setQuery] = useState('');
    const [highlighted, setHighlighted] = useState(0);
    const [undoScopes, setUndoScopes] = useState<Array<CommandScope | null>>([]);
    const [redoScopes, setRedoScopes] = useState<Array<CommandScope | null>>([]);
    const inputRef = useRef<HTMLInputElement>(null);
    const listRef = useRef<HTMLDivElement>(null);

    // Sources are asked for their items only while the palette is up, so a
    // closed palette costs nothing on every render of the app behind it.
    // Providers deliberately keep a stable registry entry while their data
    // changes. Collect on each open render so newly loaded games/profiles do
    // not get trapped behind a stale useMemo result.
    const offeredItems = isOpen ? collectCommands(providers, scope) : [];
    const sections = isOpen ? buildSections(offeredItems, query, scope) : [];
    const flat = useMemo(() => flattenSections(sections), [sections]);

    // Each opening starts clean, decided during render so the first paint never
    // shows the previous search.
    const [wasOpen, setWasOpen] = useState(isOpen);
    if (wasOpen !== isOpen) {
        setWasOpen(isOpen);
        setQuery('');
        setHighlighted(0);
        setUndoScopes([]);
        setRedoScopes([]);
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

    const changeScope = (nextScope: CommandScope | null) => {
        setUndoScopes((history) => [...history, scope].slice(-30));
        setRedoScopes([]);
        setScope(nextScope);
        setQuery('');
        setHighlighted(0);
        requestAnimationFrame(() => inputRef.current?.focus());
    };

    const undoScopeChange = () => {
        const previous = undoScopes[undoScopes.length - 1];
        if (previous === undefined && undoScopes.length === 0) return false;
        setUndoScopes((history) => history.slice(0, -1));
        setRedoScopes((history) => [...history, scope].slice(-30));
        setScope(previous ?? null);
        setQuery('');
        setHighlighted(0);
        return true;
    };

    const redoScopeChange = () => {
        const next = redoScopes[redoScopes.length - 1];
        if (next === undefined && redoScopes.length === 0) return false;
        setRedoScopes((history) => history.slice(0, -1));
        setUndoScopes((history) => [...history, scope].slice(-30));
        setScope(next ?? null);
        setQuery('');
        setHighlighted(0);
        return true;
    };

    const applyItemScope = (item: CommandItem) => {
        if (!item.nextScope) return;
        changeScope(item.nextScope);
    };

    const handleKeyDown = (event: React.KeyboardEvent) => {
        const commandModifier = event.metaKey || event.ctrlKey;
        const key = event.key.toLowerCase();
        if (query === '' && commandModifier && (key === 'z' || key === 'y')) {
            const isRedo = key === 'y' || event.shiftKey;
            const changed = isRedo ? redoScopeChange() : undoScopeChange();
            if (changed) event.preventDefault();
            return;
        }

        const shortcutAction = actionForEvent(event.nativeEvent, keybinds);
        const shortcutItem = shortcutAction
            ? findShortcutItem(offeredItems, shortcutAction, scope)
            : undefined;
        if (shortcutItem) {
            event.preventDefault();
            event.stopPropagation();
            runItem(shortcutItem);
            return;
        }

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

        if (event.key === 'Tab') {
            event.preventDefault();
            const chosen = flat[clamped];
            if (chosen) applyItemScope(chosen);
            return;
        }

        if (event.key === 'Backspace' && query === '' && scope) {
            // Deleting past the start of an empty field removes the tag, which
            // is what "delete everything" means when a tag is the first thing
            // in the field. The palette widens to the whole app rather than
            // closing, so one keystroke goes from "this game" to everything.
            event.preventDefault();
            if (scope.profile) {
                changeScope({ ...scope, profile: undefined });
            } else {
                changeScope(null);
            }
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
            <div
                className="command-palette-backdrop absolute inset-0 bg-black/60 backdrop-blur-sm"
                onClick={close}
            />

            <div
                className="command-palette-panel relative flex max-h-[64vh] w-full max-w-[600px] flex-col overflow-hidden rounded-2xl border border-gray-700 bg-gray-900 shadow-2xl"
                onClick={(e) => e.stopPropagation()}
            >
                <div className="flex shrink-0 items-center gap-3 border-b border-gray-800 px-5 py-4">
                    <svg
                        className={`h-5 w-5 shrink-0 transition-colors ${query ? 'text-blue-500' : 'text-gray-500'}`}
                        fill="none"
                        stroke="currentColor"
                        strokeWidth={2}
                        viewBox="0 0 24 24"
                    >
                        <path strokeLinecap="round" strokeLinejoin="round" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
                    </svg>
                    {scope?.game && !scope.profile && (
                        <span className="flex shrink-0 items-center gap-2 rounded-lg border border-gray-700 bg-gray-800 py-1 pl-1 pr-2">
                            {scope.game.image && (
                                <img src={scope.game.image} alt="" className="h-6 w-6 rounded object-cover" />
                            )}
                            <span className="max-w-[160px] truncate text-[13px] font-medium text-gray-200">
                                {scope.game.name}
                            </span>
                            <button
                                type="button"
                                onClick={() => {
                                    changeScope(null);
                                }}
                                className="rounded p-0.5 text-gray-500 transition-colors hover:bg-gray-700 hover:text-white"
                                aria-label={`Search everything instead of just ${scope.game.name}`}
                            >
                                <svg className="h-3.5 w-3.5" fill="none" stroke="currentColor" strokeWidth={2} viewBox="0 0 24 24">
                                    <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
                                </svg>
                            </button>
                        </span>
                    )}
                    {scope?.game && scope.profile && (
                        <span
                            className="command-scope-stack flex min-w-0 max-w-[190px] shrink items-center gap-2 rounded-lg border border-gray-700 bg-gray-800 py-1 pl-1 pr-2"
                            title={`${scope.game.name} · ${scope.profile.name}`}
                        >
                            <span className="relative h-7 w-9 shrink-0" aria-hidden="true">
                                {scope.game.image && (
                                    <img
                                        src={scope.game.image}
                                        alt=""
                                        className="absolute left-0 top-0 h-6 w-6 -rotate-3 rounded object-cover opacity-80 ring-1 ring-gray-700"
                                    />
                                )}
                                <span className="absolute bottom-0 right-0 h-6 w-6 overflow-hidden rounded-full ring-2 ring-gray-800">
                                    {scope.profile.image ? (
                                        <img src={scope.profile.image} alt="" className="h-full w-full object-cover" />
                                    ) : (
                                        <span
                                            className="flex h-full w-full items-center justify-center text-[10px] font-bold text-white"
                                            style={{ backgroundImage: scope.profile.gradient }}
                                        >
                                            {scope.profile.initial}
                                        </span>
                                    )}
                                </span>
                            </span>
                            <HoverMarquee
                                text={scope.profile.name}
                                className="min-w-0 flex-1 text-[13px] font-medium text-gray-200"
                            />
                            <span className="sr-only">for {scope.game.name}</span>
                            <button
                                type="button"
                                onClick={() => {
                                    changeScope({ ...scope, profile: undefined });
                                }}
                                className="shrink-0 rounded p-0.5 text-gray-500 transition-colors hover:bg-gray-700 hover:text-white"
                                aria-label={`Remove ${scope.profile.name} and search profiles for ${scope.game.name}`}
                            >
                                <svg className="h-3.5 w-3.5" fill="none" stroke="currentColor" strokeWidth={2} viewBox="0 0 24 24">
                                    <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
                                </svg>
                            </button>
                        </span>
                    )}
                    <input
                        ref={inputRef}
                        value={query}
                        onChange={(e) => {
                            setQuery(e.target.value);
                            setHighlighted(0);
                        }}
                        onKeyDown={handleKeyDown}
                        placeholder={
                            scope?.profile
                                ? 'Search profile actions...'
                                : scope?.group === 'Profiles'
                                ? 'Find a profile or action...'
                                : 'Search games, profiles, actions and settings'
                        }
                        spellCheck={false}
                        autoCorrect="off"
                        autoCapitalize="none"
                        autoComplete="off"
                        className="w-full bg-transparent text-[15px] text-white placeholder-gray-500 focus:outline-none"
                        aria-label="Search"
                    />
                </div>

                <div ref={listRef} className="command-palette-reveal min-h-0 flex-1 overflow-y-auto p-2">
                    {sections.length === 0 ? (
                        <p className="px-3 py-10 text-center text-[13px] text-gray-500">
                            {scope?.profile
                                  ? 'No action matches that.'
                                  : scope?.group === 'Profiles'
                                  ? 'No profile matches that.'
                                  : 'Nothing matches that.'}
                        </p>
                    ) : (
                        sections.map((section) => (
                            <div key={section.group} className="mb-1 last:mb-0">
                                {(!scope || sections.length > 1) && (
                                    <h3 className="px-3 pb-1 pt-2 text-[11px] font-semibold uppercase tracking-widest text-gray-500">
                                        {section.group}
                                    </h3>
                                )}

                                {section.items.map((item) => {
                                    index += 1;
                                    const isHighlighted = index === clamped;
                                    const position = index;

                                    return (
                                        <div
                                            key={item.id}
                                            data-highlighted={isHighlighted}
                                            onMouseMove={() => setHighlighted(position)}
                                            className={`group flex w-full items-center rounded-xl text-left transition-colors ${
                                                isHighlighted ? 'bg-gray-800' : 'hover:bg-gray-800/60'
                                            }`}
                                        >
                                            <button
                                                type="button"
                                                onClick={() => runItem(item)}
                                                className="flex min-w-0 flex-1 items-center gap-3 px-3 py-2.5 text-left"
                                            >
                                                <ItemArtwork item={item} />

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
                                                {item.hint && <Hint>{item.hint}</Hint>}
                                            </button>

                                            {item.nextScope && (
                                                <button
                                                    type="button"
                                                    onClick={(event) => {
                                                        event.stopPropagation();
                                                        applyItemScope(item);
                                                    }}
                                                    className="mr-3 flex h-7 w-7 shrink-0 items-center justify-center rounded-md border border-gray-700 text-gray-400 transition-colors hover:border-gray-600 hover:bg-gray-700 hover:text-white"
                                                    aria-label={`Add ${item.title} as a filter`}
                                                >
                                                    <AppIcon name="plus" className="h-4 w-4" strokeWidth={2} />
                                                </button>
                                            )}
                                        </div>
                                    );
                                })}
                            </div>
                        ))
                    )}
                </div>

                <div className="command-palette-reveal flex shrink-0 items-center gap-4 border-t border-gray-800 px-5 py-2.5 text-[11px] text-gray-500">
                    <span className="flex items-center gap-1.5">
                        <Hint>↩</Hint> open
                    </span>
                    <span className="flex items-center gap-1.5">
                        <Hint>↑↓</Hint> navigate
                    </span>
                    {flat.some((item) => item.nextScope) && (
                        <span className="flex items-center gap-1.5">
                            <Hint>tab</Hint> add filter
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
