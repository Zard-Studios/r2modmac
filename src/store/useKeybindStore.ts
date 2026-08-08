import { create } from 'zustand';

import { DEFAULT_KEYBINDS, resolveKeybinds, type KeybindMap } from '../utils/keybinds';

interface KeybindState {
    keybinds: KeybindMap;
    /** Fold the overrides from settings onto the defaults. */
    hydrate: (overrides: Record<string, string> | null | undefined) => void;
}

/**
 * The shortcuts currently in force.
 *
 * A store rather than props: the views that fire shortcuts sit at different
 * depths — the profile list, the mod sidebar, the profile view — and threading
 * one table through every layer between them would touch components that have
 * nothing to do with the keyboard.
 */
export const useKeybindStore = create<KeybindState>((set) => ({
    keybinds: DEFAULT_KEYBINDS,
    hydrate: (overrides) => set({ keybinds: resolveKeybinds(overrides) }),
}));
