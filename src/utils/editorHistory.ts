/**
 * Undo and redo for a plain text editor.
 *
 * Lives apart from the component so it can be tested directly. The version that
 * lived inside the editor nested one state update inside another's updater —
 * an updater has to be pure — and the result was that every edit was silently
 * dropped: the text never changed, and the Save button never noticed one.
 */

/** Undo and redo stacks for the text being edited. */
export interface EditorHistory {
    undo: string[];
    redo: string[];
}

export const EMPTY_HISTORY: EditorHistory = { undo: [], redo: [] };

/** How many steps back a session keeps, so it cannot grow without limit. */
const HISTORY_LIMIT = 200;

/**
 * The three history moves, as pure functions.
 *
 * Kept out of the component so they can be tested directly: this logic is easy
 * to get subtly wrong, and the version before it lived here nested one state
 * update inside another's updater, which silently dropped every edit.
 */
export function pushEdit(
    history: EditorHistory,
    current: string,
    next: string
): { history: EditorHistory; source: string } {
    if (next === current) return { history, source: current };
    return {
        // A fresh edit invalidates anything that had been undone.
        history: { undo: [...history.undo, current].slice(-HISTORY_LIMIT), redo: [] },
        source: next,
    };
}

export function stepBack(
    history: EditorHistory,
    current: string
): { history: EditorHistory; source: string } {
    if (history.undo.length === 0) return { history, source: current };
    return {
        history: {
            undo: history.undo.slice(0, -1),
            redo: [...history.redo, current],
        },
        source: history.undo[history.undo.length - 1],
    };
}

export function stepForward(
    history: EditorHistory,
    current: string
): { history: EditorHistory; source: string } {
    if (history.redo.length === 0) return { history, source: current };
    return {
        history: {
            undo: [...history.undo, current],
            redo: history.redo.slice(0, -1),
        },
        source: history.redo[history.redo.length - 1],
    };
}
