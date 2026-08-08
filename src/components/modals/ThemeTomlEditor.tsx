import { useCallback, useEffect, useRef, useState } from 'react';

import { Button } from '../ui';
import {
    EMPTY_HISTORY,
    pushEdit,
    stepBack,
    stepForward,
    type EditorHistory,
} from '../../utils/editorHistory';

/**
 * The theme file, edited as text.
 *
 * Deliberately the same shape as the mod config editor: a line-number gutter
 * beside a monospaced textarea, undo and redo, a dirty marker, save on Cmd+S
 * and a way out to the folder. Someone who has edited a mod's `.cfg` in this
 * app already knows how to use this, and that familiarity is worth more than
 * anything a bespoke design would buy.
 *
 * It edits the raw file rather than a parsed model on purpose: comments,
 * ordering and anything the app does not understand survive a round trip.
 */
interface ThemeTomlEditorProps {
    /** File being edited; null for a built-in, which has none. */
    fileName: string | null;
    /** Read-only view of what a built-in would look like as a file. */
    readOnlySource?: string;
    editable: boolean;
    /** Called after a successful write so the app can reload the theme. */
    onSaved: () => void | Promise<void>;
}

const LINE_HEIGHT = 20;



export function ThemeTomlEditor({
    fileName,
    readOnlySource,
    editable,
    onSaved,
}: ThemeTomlEditorProps) {
    const [source, setSource] = useState('');
    const [savedSource, setSavedSource] = useState('');
    const [loading, setLoading] = useState(false);
    const [saving, setSaving] = useState(false);
    const [error, setError] = useState<string | null>(null);

    const gutterRef = useRef<HTMLDivElement>(null);
    const textareaRef = useRef<HTMLTextAreaElement>(null);
    // State rather than a ref: the undo and redo buttons enable and disable
    // from these, and a ref would not re-render to update them.
    const [history, setHistory] = useState<EditorHistory>(EMPTY_HISTORY);

    const dirty = editable && source !== savedSource;

    // Which file the state below belongs to. Adjusting during render — rather
    // than from an effect — means the first paint already shows the right file
    // instead of briefly showing the previous one.
    const [loadedFor, setLoadedFor] = useState<string | null | undefined>(undefined);
    if (fileName !== loadedFor) {
        setLoadedFor(fileName);
        setHistory(EMPTY_HISTORY);
        setError(null);
        setSource('');
        setSavedSource('');
        setLoading(!!fileName);
    }

    // The read itself is genuinely external, so it belongs in an effect. A
    // built-in has no file: its text is rendered straight from the prop.
    useEffect(() => {
        if (!fileName) return;
        let cancelled = false;
        void window.ipcRenderer
            .readThemeSource(fileName)
            .then((text) => {
                if (cancelled) return;
                setSource(text);
                setSavedSource(text);
            })
            .catch((e) => {
                if (!cancelled) setError(String(e));
            })
            .finally(() => {
                if (!cancelled) setLoading(false);
            });
        return () => { cancelled = true; };
    }, [fileName]);

    /** What the textarea shows: the file being edited, or a built-in's text. */
    const shown = fileName ? (loading ? '' : source) : (readOnlySource ?? '');
    const lineCount = shown.split('\n').length || 1;

    /** Apply one history move to both pieces of state. */
    const applyMove = useCallback(
        (move: { history: EditorHistory; source: string }) => {
            setHistory(move.history);
            setSource(move.source);
        },
        []
    );

    const edit = useCallback(
        (next: string) => applyMove(pushEdit(history, source, next)),
        [applyMove, history, source]
    );
    const undo = useCallback(
        () => applyMove(stepBack(history, source)),
        [applyMove, history, source]
    );
    const redo = useCallback(
        () => applyMove(stepForward(history, source)),
        [applyMove, history, source]
    );

    const save = useCallback(async () => {
        if (!fileName || !editable || !dirty) return;
        setSaving(true);
        setError(null);
        try {
            await window.ipcRenderer.writeTheme(fileName, source);
            setSavedSource(source);
            await onSaved();
        } catch (e) {
            setError(String(e));
        } finally {
            setSaving(false);
        }
    }, [fileName, editable, dirty, source, onSaved]);

    // Keep the gutter aligned with the text as it scrolls or grows.
    const syncScroll = useCallback(() => {
        if (textareaRef.current && gutterRef.current) {
            gutterRef.current.scrollTop = textareaRef.current.scrollTop;
        }
    }, []);
    useEffect(() => { syncScroll(); }, [source, syncScroll]);

    // Shortcuts, scoped to the editor so they cannot fight the rest of the app.
    const onKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
        const meta = e.metaKey || e.ctrlKey;
        if (!meta) return;
        const key = e.key.toLowerCase();
        if (key === 's') { e.preventDefault(); void save(); }
        else if (key === 'z' && !e.shiftKey) { e.preventDefault(); undo(); }
        else if ((key === 'z' && e.shiftKey) || key === 'y') { e.preventDefault(); redo(); }
    };

    const iconButton =
        'rounded-lg p-2 text-gray-400 transition-colors hover:bg-gray-700 hover:text-white disabled:cursor-not-allowed disabled:opacity-40';

    return (
        <div className="flex min-h-0 flex-1 flex-col">
            {/* Toolbar */}
            <div className="flex shrink-0 items-center justify-between gap-3 border-b border-gray-800 px-6 py-3">
                <div className="min-w-0">
                    <p className="truncate font-mono text-[12px] text-gray-300">
                        {fileName ?? 'Built-in theme — no file on disk'}
                    </p>
                    {dirty && <p className="text-[11px] text-fg-warning">Unsaved changes</p>}
                </div>

                <div className="flex shrink-0 items-center gap-1">
                    <button
                        type="button"
                        onClick={undo}
                        disabled={!editable || history.undo.length === 0}
                        title="Undo (⌘Z)"
                        aria-label="Undo"
                        className={iconButton}
                    >
                        <svg className="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 10h10a5 5 0 010 10h-3M3 10l4-4M3 10l4 4" />
                        </svg>
                    </button>
                    <button
                        type="button"
                        onClick={redo}
                        disabled={!editable || history.redo.length === 0}
                        title="Redo (⇧⌘Z)"
                        aria-label="Redo"
                        className={iconButton}
                    >
                        <svg className="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 10H11a5 5 0 000 10h3M21 10l-4-4M21 10l-4 4" />
                        </svg>
                    </button>

                    <span className="mx-1 h-5 w-px bg-gray-700" />

                    <button
                        type="button"
                        onClick={() => void window.ipcRenderer.openThemesFolder()}
                        title="Open themes folder"
                        aria-label="Open themes folder"
                        className={iconButton}
                    >
                        <svg className="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.8} d="M3 7a2 2 0 012-2h4l2 2h8a2 2 0 012 2v8a2 2 0 01-2 2H5a2 2 0 01-2-2V7z" />
                        </svg>
                    </button>

                    <Button
                        variant="primary"
                        size="sm"
                        onClick={() => void save()}
                        disabled={!dirty || saving}
                        className="ml-1"
                    >
                        {saving ? 'Saving…' : 'Save'}
                    </Button>
                </div>
            </div>

            {!editable && (
                <p className="shrink-0 border-b border-gray-800 bg-fg-accent/10 px-6 py-2.5 text-[12px] text-fg-accent">
                    This is a built-in theme, shown as it would be written. Duplicate it to get a
                    file you can edit.
                </p>
            )}

            {error && (
                <p className="shrink-0 border-b border-gray-800 bg-fg-danger/10 px-6 py-2.5 text-[12px] text-fg-danger">
                    {error}
                </p>
            )}

            {/* Gutter + text */}
            <div className="flex min-h-0 flex-1 overflow-hidden">
                <div
                    ref={gutterRef}
                    aria-hidden="true"
                    className="w-12 shrink-0 select-none overflow-hidden border-r border-gray-800/80 bg-gray-900/30 py-3 pr-2.5 text-right font-mono text-[11px] text-gray-400"
                >
                    {Array.from({ length: lineCount }).map((_, i) => (
                        <div key={i} style={{ height: LINE_HEIGHT, lineHeight: `${LINE_HEIGHT}px` }}>
                            {i + 1}
                        </div>
                    ))}
                </div>

                <textarea
                    ref={textareaRef}
                    value={shown}
                    onChange={(e) => edit(e.target.value)}
                    onScroll={syncScroll}
                    onKeyDown={onKeyDown}
                    readOnly={!editable}
                    spellCheck={false}
                    autoCorrect="off"
                    autoCapitalize="none"
                    aria-label="Theme file contents"
                    style={{ lineHeight: `${LINE_HEIGHT}px`, tabSize: 2 }}
                    className="min-h-0 flex-1 resize-none bg-transparent py-3 pl-3 pr-6 font-mono text-[12px] text-white outline-none read-only:text-gray-300"
                />
            </div>
        </div>
    );
}
