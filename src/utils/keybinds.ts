/**
 * Keyboard shortcuts.
 *
 * Actions are named here once, with a default combination each, and the user's
 * settings carry only the ones they changed. Storing overrides rather than the
 * whole table means a shortcut added in a later version arrives with its default
 * already in place instead of silently missing from everyone's saved settings.
 *
 * A combination is written as modifiers joined by `+` and ending in one key:
 * `Mod+Shift+R`. `Mod` is the platform's command modifier — Command on macOS,
 * Control on Windows and Linux — so one stored string means the right key
 * everywhere and a settings file stays portable between machines. Order is
 * canonical, so two spellings of the same combination compare equal.
 */

/**
 * Which key `Mod` stands for. Passed explicitly rather than read from the
 * environment inside each function, so both platforms can be exercised from
 * whichever machine happens to be running the tests.
 */
export type Platform = 'apple' | 'other';

export function detectPlatform(): Platform {
    // Matches how the rest of the app sniffs the platform (see platformUtils).
    if (typeof navigator === 'undefined') return 'apple';
    return navigator.userAgent.includes('Mac') ? 'apple' : 'other';
}

export type KeybindActionId =
    | 'apply-mods'
    | 'launch-modded'
    | 'launch-vanilla'
    | 'stop-game'
    | 'new-profile'
    | 'duplicate-profile'
    | 'open-search'
    | 'search-mods';

export interface KeybindAction {
    id: KeybindActionId;
    label: string;
    description: string;
    /** Groups the action under a heading in the settings panel. */
    group: 'Game' | 'Profiles';
    defaultAccelerator: string;
}

/**
 * Every bindable action, in the order the settings panel lists them.
 *
 * The defaults follow the combinations asked for in the feature request, with
 * two additions that had no suggestion attached: Return for applying mods,
 * because it reads as "commit what is on screen", and Command-period for
 * stopping the game, the system-wide gesture for calling something off.
 */
export const KEYBIND_ACTIONS: readonly KeybindAction[] = [
    {
        id: 'apply-mods',
        label: 'Apply mods to game',
        description: 'Write the active profile into the game folder.',
        group: 'Game',
        defaultAccelerator: 'Mod+Enter',
    },
    {
        id: 'launch-modded',
        label: 'Launch game (modded)',
        description: 'Start the game with the active profile.',
        group: 'Game',
        defaultAccelerator: 'Mod+R',
    },
    {
        id: 'launch-vanilla',
        label: 'Launch game (unmodded)',
        description: 'Start the game with no mods loaded.',
        group: 'Game',
        defaultAccelerator: 'Mod+Shift+R',
    },
    {
        id: 'stop-game',
        label: 'Quit game',
        description: 'Stop the running game.',
        group: 'Game',
        defaultAccelerator: 'Mod+.',
    },
    {
        id: 'new-profile',
        label: 'New profile',
        description: 'Create a profile for the current game.',
        group: 'Profiles',
        defaultAccelerator: 'Mod+N',
    },
    {
        id: 'duplicate-profile',
        label: 'Duplicate profile',
        description: 'Copy the current profile, mods and all.',
        group: 'Profiles',
        defaultAccelerator: 'Mod+D',
    },
    {
        id: 'open-search',
        label: 'Search',
        description: 'Find games, profiles, settings and commands from anywhere.',
        group: 'Profiles',
        defaultAccelerator: 'Mod+F',
    },
    {
        id: 'search-mods',
        label: 'Search mods in profile',
        description: 'Jump to the search field inside the open profile.',
        group: 'Profiles',
        defaultAccelerator: 'Mod+Shift+F',
    },
];

/** An empty accelerator means the user turned the shortcut off. */
export type KeybindMap = Record<KeybindActionId, string>;

export const DEFAULT_KEYBINDS: KeybindMap = Object.fromEntries(
    KEYBIND_ACTIONS.map((action) => [action.id, action.defaultAccelerator])
) as KeybindMap;

// ── Reading a combination off a key event ────────────────────────────────────

/**
 * Physical keys whose `code` names them directly. `key` cannot be used for the
 * letter and punctuation rows: it carries the *typed* character, so Shift+/
 * arrives as `?` and a Dvorak layout reports a different letter than the one the
 * user's fingers are on.
 */
const CODE_NAMES: Record<string, string> = {
    Period: '.',
    Comma: ',',
    Slash: '/',
    Backslash: '\\',
    Minus: '-',
    Equal: '=',
    Semicolon: ';',
    Quote: "'",
    Backquote: '`',
    BracketLeft: '[',
    BracketRight: ']',
    Space: 'Space',
    Enter: 'Enter',
    NumpadEnter: 'Enter',
    Escape: 'Escape',
    Tab: 'Tab',
    Backspace: 'Backspace',
    Delete: 'Delete',
    ArrowUp: 'Up',
    ArrowDown: 'Down',
    ArrowLeft: 'Left',
    ArrowRight: 'Right',
    Home: 'Home',
    End: 'End',
    PageUp: 'PageUp',
    PageDown: 'PageDown',
};

function keyNameFromCode(code: string): string | null {
    if (/^Key[A-Z]$/.test(code)) return code.slice(3);
    if (/^Digit[0-9]$/.test(code)) return code.slice(5);
    if (/^F([1-9]|1[0-9]|20)$/.test(code)) return code;
    return CODE_NAMES[code] ?? null;
}

/**
 * Storage puts Command first, matching how accelerators are written everywhere
 * else — `Mod+Shift+R` — so a hand-edited settings file reads naturally. macOS
 * *prints* modifiers in the opposite order, which is a display concern only.
 */
const STORAGE_ORDER = ['Mod', 'Ctrl', 'Alt', 'Shift'] as const;

/**
 * Display order differs by platform: macOS ends on Command (⌃⌥⇧⌘R) while
 * Windows and Linux lead with Control (Ctrl+Alt+Shift+R).
 */
const DISPLAY_ORDER: Record<Platform, readonly string[]> = {
    apple: ['Ctrl', 'Alt', 'Shift', 'Mod'],
    other: ['Mod', 'Ctrl', 'Alt', 'Shift'],
};

/**
 * The combination a key event represents, or null if the event is a modifier on
 * its own — holding Command is not yet a shortcut.
 */
export function acceleratorFromEvent(
    event: Pick<KeyboardEvent, 'code' | 'ctrlKey' | 'altKey' | 'shiftKey' | 'metaKey'>,
    platform: Platform = detectPlatform()
): string | null {
    const key = keyNameFromCode(event.code);
    if (!key) return null;

    const held = new Set<string>();
    // Off macOS the command modifier *is* Control, so Control produces `Mod`
    // and there is no separate `Ctrl` to record.
    if (platform === 'apple') {
        if (event.metaKey) held.add('Mod');
        if (event.ctrlKey) held.add('Ctrl');
    } else if (event.ctrlKey) {
        held.add('Mod');
    }
    if (event.altKey) held.add('Alt');
    if (event.shiftKey) held.add('Shift');
    return [...STORAGE_ORDER.filter((m) => held.has(m)), key].join('+');
}

/** Put a hand-written combination into canonical order so it compares equal. */
export function normalizeAccelerator(
    accelerator: string,
    platform: Platform = detectPlatform()
): string | null {
    const tokens = accelerator
        .split('+')
        .map((t) => t.trim())
        .filter(Boolean);
    if (tokens.length === 0) return null;

    const key = tokens[tokens.length - 1];
    const named = keyNameFromCode(key) ?? (/^[A-Za-z0-9]$/.test(key) ? key.toUpperCase() : null);
    const canonicalKey = named ?? (Object.values(CODE_NAMES).includes(key) ? key : null);
    if (!canonicalKey) return null;

    const modifiers = new Set(
        tokens.slice(0, -1).map((token) => {
            const lower = token.toLowerCase();
            if (lower === 'cmd' || lower === 'command' || lower === 'meta' || lower === 'mod') return 'Mod';
            if (lower === 'ctrl' || lower === 'control') return 'Ctrl';
            if (lower === 'alt' || lower === 'option' || lower === 'opt') return 'Alt';
            if (lower === 'shift') return 'Shift';
            return '';
        })
    );
    if (modifiers.has('')) return null;

    // One physical key cannot appear twice: off macOS, Control and the command
    // modifier are the same key, so `Ctrl+R` and `Mod+R` are one shortcut.
    if (platform !== 'apple' && modifiers.delete('Ctrl')) modifiers.add('Mod');

    return [...STORAGE_ORDER.filter((m) => modifiers.has(m)), canonicalKey].join('+');
}

/**
 * Whether a combination is usable as a shortcut.
 *
 * Something has to hold the key apart from ordinary typing, so a bare letter is
 * refused. Function keys carry no character and stand alone safely.
 */
export function isUsableAccelerator(
    accelerator: string,
    platform: Platform = detectPlatform()
): boolean {
    const canonical = normalizeAccelerator(accelerator, platform);
    if (!canonical) return false;
    const parts = canonical.split('+');
    if (parts.length > 1) return true;
    return /^F([1-9]|1[0-9]|20)$/.test(parts[0]);
}

// ── Display ──────────────────────────────────────────────────────────────────

/** macOS prints modifiers as glyphs and runs them together: `⇧⌘R`. */
const APPLE_SYMBOLS: Record<string, string> = {
    Mod: '⌘',
    Ctrl: '⌃',
    Alt: '⌥',
    Shift: '⇧',
    Enter: '↩',
    Escape: '⎋',
    Tab: '⇥',
    Backspace: '⌫',
    Delete: '⌦',
    Up: '↑',
    Down: '↓',
    Left: '←',
    Right: '→',
    Space: '␣',
};

/** Windows and Linux spell them out and join with `+`: `Ctrl+Shift+R`. */
const OTHER_NAMES: Record<string, string> = {
    Mod: 'Ctrl',
    Alt: 'Alt',
    Shift: 'Shift',
    Up: '↑',
    Down: '↓',
    Left: '←',
    Right: '→',
};

/**
 * Render a combination the way the platform's own menus do — `⇧⌘R` on macOS,
 * `Ctrl+Shift+R` elsewhere. Printing glyphs for keys a Windows keyboard does
 * not have would leave the panel describing a machine the user is not on.
 */
export function formatAccelerator(
    accelerator: string,
    platform: Platform = detectPlatform()
): string {
    const canonical = normalizeAccelerator(accelerator, platform);
    if (!canonical) return '';
    const parts = canonical.split('+');
    const key = parts[parts.length - 1];
    const held = new Set(parts.slice(0, -1));
    const ordered = [...DISPLAY_ORDER[platform].filter((m) => held.has(m)), key];

    if (platform === 'apple') {
        return ordered.map((part) => APPLE_SYMBOLS[part] ?? part).join('');
    }
    return ordered.map((part) => OTHER_NAMES[part] ?? part).join('+');
}

// ── Resolving what is actually bound ─────────────────────────────────────────

/**
 * Actions that were renamed, so a rebind saved under the old name survives.
 *
 * Without this the override would be dropped as unrecognised and the user would
 * silently get the default back — which is exactly the sort of quiet loss the
 * override-only storage was meant to avoid.
 */
const RENAMED_ACTIONS: Record<string, KeybindActionId> = {
    // The profile finder grew into a search across the whole app.
    'find-profile': 'open-search',
};

/**
 * Fold the user's overrides onto the defaults.
 *
 * Anything unrecognised is dropped rather than trusted: the settings file is
 * hand-editable, and a stale action id from an older version must not leave a
 * shortcut bound to nothing.
 */
export function resolveKeybinds(
    overrides: Record<string, string> | null | undefined,
    platform: Platform = detectPlatform()
): KeybindMap {
    const resolved = { ...DEFAULT_KEYBINDS };

    // Old names are folded in first so a file carrying both spellings resolves
    // to the current one, whatever order the keys happen to sit in.
    const byAction: Record<string, string> = {};
    for (const [key, value] of Object.entries(overrides ?? {})) {
        const renamed = RENAMED_ACTIONS[key];
        if (renamed) byAction[renamed] = value;
    }
    for (const [key, value] of Object.entries(overrides ?? {})) {
        if (!RENAMED_ACTIONS[key]) byAction[key] = value;
    }

    for (const action of KEYBIND_ACTIONS) {
        const override = byAction[action.id];
        if (override === undefined) continue;
        if (override === '') {
            resolved[action.id] = '';
            continue;
        }
        const canonical = normalizeAccelerator(override, platform);
        if (canonical && isUsableAccelerator(canonical, platform)) resolved[action.id] = canonical;
    }
    return resolved;
}

/** Only what differs from the defaults is worth writing to settings. */
export function overridesFromKeybinds(keybinds: KeybindMap): Record<string, string> {
    const overrides: Record<string, string> = {};
    for (const action of KEYBIND_ACTIONS) {
        if (keybinds[action.id] !== action.defaultAccelerator) {
            overrides[action.id] = keybinds[action.id];
        }
    }
    return overrides;
}

/**
 * Actions sharing a combination, keyed by that combination.
 *
 * Two actions on one shortcut is not rejected outright — the user may be midway
 * through a swap — but the panel has to be able to say so.
 */
export function findKeybindConflicts(keybinds: KeybindMap): Map<string, KeybindActionId[]> {
    const byAccelerator = new Map<string, KeybindActionId[]>();
    for (const action of KEYBIND_ACTIONS) {
        const accelerator = keybinds[action.id];
        if (!accelerator) continue;
        byAccelerator.set(accelerator, [...(byAccelerator.get(accelerator) ?? []), action.id]);
    }
    return new Map([...byAccelerator].filter(([, ids]) => ids.length > 1));
}

/** The action a key event triggers, if any. */
export function actionForEvent(
    event: Pick<KeyboardEvent, 'code' | 'ctrlKey' | 'altKey' | 'shiftKey' | 'metaKey'>,
    keybinds: KeybindMap,
    platform: Platform = detectPlatform()
): KeybindActionId | null {
    const accelerator = acceleratorFromEvent(event, platform);
    if (!accelerator) return null;
    const match = KEYBIND_ACTIONS.find((action) => keybinds[action.id] === accelerator);
    return match?.id ?? null;
}
