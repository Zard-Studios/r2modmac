import { useLayoutEffect } from 'react';
import { create } from 'zustand';

import type { CommandItem, CommandScope } from '../utils/commandPalette';

/** Builds only the items needed by the palette's current context. */
export type CommandProvider = (scope: CommandScope | null) => CommandItem[];

interface CommandState {
    isOpen: boolean;
    /**
     * Narrows the palette. The magnifier on a game's profile page opens it
     * pinned to profiles of that game; the keyboard shortcut opens it whole.
     */
    scope: CommandScope | null;
    open: (scope?: CommandScope) => void;
    close: () => void;
    /** What the shortcut does: the same key that opened it puts it away. */
    toggle: (scope?: CommandScope) => void;
    /** Replaces the active context while leaving the palette open. */
    setScope: (scope: CommandScope | null) => void;
    /** Drops the narrowing, widening the palette to everything. */
    clearScope: () => void;

    /** Providers by source id, in registration order. */
    providers: Record<string, CommandProvider>;
    setProvider: (id: string, provider: CommandProvider) => void;
    clearProvider: (id: string) => void;
}

/**
 * The command palette's state and its sources.
 *
 * Views register what they know how to do instead of handing callbacks up to
 * `App`: launching a game only exists while a profile is open, and the palette
 * itself renders at the top level so it can be reached from the home screen.
 * A registry is what lets those two facts coexist.
 */
export const useCommandStore = create<CommandState>((set) => ({
    isOpen: false,
    scope: null,
    open: (scope) => set({ isOpen: true, scope: scope ?? null }),
    close: () => set({ isOpen: false }),
    toggle: (scope) =>
        set((state) => (state.isOpen ? { isOpen: false } : { isOpen: true, scope: scope ?? null })),
    setScope: (scope) => set({ scope }),
    clearScope: () => set({ scope: null }),

    providers: {},
    setProvider: (id, provider) =>
        set((state) => ({ providers: { ...state.providers, [id]: provider } })),
    clearProvider: (id) =>
        set((state) => {
            const rest = { ...state.providers };
            delete rest[id];
            return { providers: rest };
        }),
}));

/**
 * Contribute commands for as long as the calling component is mounted.
 *
 * Registration happens in a layout effect so a screen's newest context is in
 * the registry before Spotlight can be opened from that painted screen.
 */
export function useCommandSource(id: string, provider: CommandProvider) {
    useLayoutEffect(() => {
        const { setProvider, clearProvider } = useCommandStore.getState();
        setProvider(id, provider);
        return () => clearProvider(id);
    }, [id, provider]);
}

/** Everything currently on offer, from every mounted source. */
export function collectCommands(
    providers: Record<string, CommandProvider>,
    scope: CommandScope | null = null
): CommandItem[] {
    return Object.values(providers).flatMap((provider) => {
        try {
            return provider(scope);
        } catch (error) {
            // One broken source must not take the whole palette down with it —
            // but it must not disappear quietly either. Swallowing this once
            // turned a crash into a palette that simply found nothing, which is
            // far harder to diagnose than the error itself.
            console.error('[command-palette] a source failed to build its items', error);
            return [];
        }
    });
}
