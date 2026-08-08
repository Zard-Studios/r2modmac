import { useEffect, useMemo, useRef, useState } from 'react';

import type { Profile } from '../../types/profile';
import { fuzzyScore } from '../../utils/keybinds';
import { getProfileAvatarGradient, getProfileInitial } from '../../utils/profileAvatar';

interface ProfileFinderProps {
    isOpen: boolean;
    profiles: Profile[];
    /** Highlighted on open, so the finder says where you already are. */
    activeProfileId?: string | null;
    onSelect: (profileId: string) => void;
    onClose: () => void;
}

/**
 * Find a profile by typing, from the profile list or from inside a profile.
 *
 * Deliberately an overlay rather than a field on the list: the request was for
 * something reachable from both places, and a field only exists on one of them.
 */
export function ProfileFinder({ isOpen, profiles, activeProfileId, onSelect, onClose }: ProfileFinderProps) {
    const [query, setQuery] = useState('');
    const [highlighted, setHighlighted] = useState(0);
    const inputRef = useRef<HTMLInputElement>(null);
    const listRef = useRef<HTMLDivElement>(null);

    const matches = useMemo(() => {
        const scored = profiles
            .map((profile) => ({ profile, score: fuzzyScore(query, profile.name) }))
            .filter((entry): entry is { profile: Profile; score: number } => entry.score !== null);

        // With nothing typed every profile scores zero, so the tie-break is what
        // orders the list: most recently used first, as on the profile page.
        scored.sort((a, b) => b.score - a.score || (b.profile.lastUsed ?? 0) - (a.profile.lastUsed ?? 0));
        return scored.map((entry) => entry.profile);
    }, [profiles, query]);

    // Each opening starts from a clean query. Done during render rather than in
    // an effect so the first paint never shows the previous search.
    const [wasOpen, setWasOpen] = useState(isOpen);
    if (wasOpen !== isOpen) {
        setWasOpen(isOpen);
        setQuery('');
        setHighlighted(0);
    }

    useEffect(() => {
        if (!isOpen) return;
        // The field has to take focus for typing to land anywhere.
        const raf = requestAnimationFrame(() => inputRef.current?.focus());
        return () => cancelAnimationFrame(raf);
    }, [isOpen]);

    // A shrinking result list must not leave the highlight past the end.
    const clamped = Math.min(highlighted, Math.max(0, matches.length - 1));
    if (clamped !== highlighted) setHighlighted(clamped);

    useEffect(() => {
        listRef.current?.querySelector('[data-highlighted="true"]')?.scrollIntoView({ block: 'nearest' });
    }, [clamped, matches.length]);

    if (!isOpen) return null;

    const handleKeyDown = (event: React.KeyboardEvent) => {
        if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
            event.preventDefault();
            if (matches.length === 0) return;
            const step = event.key === 'ArrowDown' ? 1 : -1;
            setHighlighted((current) => (current + step + matches.length) % matches.length);
            return;
        }

        if (event.key === 'Enter') {
            event.preventDefault();
            const chosen = matches[clamped];
            if (chosen) onSelect(chosen.id);
            return;
        }

        if (event.key === 'Escape') {
            event.preventDefault();
            // Stopped here so the app-wide Escape handler does not also step
            // back out of the profile the user is standing in.
            event.stopPropagation();
            onClose();
        }
    };

    return (
        <div className="fixed inset-0 z-[80] flex items-start justify-center p-4 pt-[15vh]">
            <div className="absolute inset-0 bg-black/60 backdrop-blur-sm" onClick={onClose} />

            <div
                className="relative flex max-h-[60vh] w-full max-w-[560px] flex-col overflow-hidden rounded-2xl border border-gray-700 bg-gray-900 shadow-2xl"
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
                    <input
                        ref={inputRef}
                        value={query}
                        onChange={(e) => {
                            setQuery(e.target.value);
                            setHighlighted(0);
                        }}
                        onKeyDown={handleKeyDown}
                        placeholder="Find a profile..."
                        className="w-full bg-transparent text-[15px] text-white placeholder-gray-500 focus:outline-none"
                        aria-label="Find a profile"
                    />
                    <span className="shrink-0 rounded-md border border-gray-700 px-1.5 py-0.5 text-[11px] text-gray-500">
                        esc
                    </span>
                </div>

                <div ref={listRef} className="min-h-0 flex-1 overflow-y-auto p-2">
                    {matches.length === 0 ? (
                        <p className="px-3 py-8 text-center text-[13px] text-gray-500">
                            {profiles.length === 0 ? 'No profiles for this game yet.' : 'No profile matches that.'}
                        </p>
                    ) : (
                        matches.map((profile, index) => {
                            const isHighlighted = index === clamped;
                            return (
                                <button
                                    key={profile.id}
                                    type="button"
                                    data-highlighted={isHighlighted}
                                    onMouseMove={() => setHighlighted(index)}
                                    onClick={() => onSelect(profile.id)}
                                    className={`flex w-full items-center gap-3 rounded-xl px-3 py-2.5 text-left transition-colors ${
                                        isHighlighted ? 'bg-gray-800' : 'hover:bg-gray-800/60'
                                    }`}
                                >
                                    <span
                                        className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg text-[15px] font-bold text-[#ffffff]"
                                        style={{ backgroundImage: getProfileAvatarGradient(profile.name, profile.id) }}
                                        aria-hidden="true"
                                    >
                                        {getProfileInitial(profile.name)}
                                    </span>

                                    <span className="min-w-0 flex-1">
                                        <span className="block truncate text-[14px] font-medium text-white">
                                            {profile.name}
                                        </span>
                                        <span className="block text-[12px] text-gray-500">
                                            {profile.mods.length} {profile.mods.length === 1 ? 'mod' : 'mods'}
                                            {profile.is_vanilla ? ' · vanilla' : ''}
                                        </span>
                                    </span>

                                    {profile.id === activeProfileId && (
                                        <span className="shrink-0 rounded-md border border-gray-700 bg-gray-800 px-2 py-0.5 text-[11px] text-gray-400">
                                            open
                                        </span>
                                    )}
                                </button>
                            );
                        })
                    )}
                </div>
            </div>
        </div>
    );
}
