import { useEffect, useRef } from 'react';
import { create } from 'zustand';

import type { CommandGroup, CommandItem } from '../utils/commandPalette';

/** Builds the items a view can offer, called when the palette needs them. */
export type CommandProvider = () => CommandItem[];

interface CommandState {
    isOpen: boolean;
    /**
     * Narrows the palette to one group. The magnifier on the profile page opens
     * it scoped to profiles; the keyboard shortcut opens it whole.
     */
    scope: CommandGroup | null;
    open: (scope?: CommandGroup) => void;
    close: () => void;
    /** What the shortcut does: the same key that opened it puts it away. */
    toggle: (scope?: CommandGroup) => void;

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
 * The provider is read through a ref and registered once, so a closure rebuilt
 * on every render does not churn the store — and the palette still sees the
 * latest one, because it calls the provider at the moment it needs items.
 */
export function useCommandSource(id: string, provider: CommandProvider) {
    const providerRef = useRef(provider);
    useEffect(() => {
        providerRef.current = provider;
    }, [provider]);

    useEffect(() => {
        const { setProvider, clearProvider } = useCommandStore.getState();
        setProvider(id, () => providerRef.current());
        return () => clearProvider(id);
    }, [id]);
}

/** Everything currently on offer, from every mounted source. */
export function collectCommands(providers: Record<string, CommandProvider>): CommandItem[] {
    return Object.values(providers).flatMap((provider) => {
        try {
            return provider();
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
