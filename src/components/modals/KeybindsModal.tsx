import { useCallback, useEffect, useMemo, useState } from 'react';
import { Button } from '../ui';
import { AppIcon } from '../ui/icons';

import {
    DEFAULT_KEYBINDS,
    KEYBIND_ACTIONS,
    acceleratorFromEvent,
    findKeybindConflicts,
    formatAccelerator,
    isUsableAccelerator,
    type KeybindActionId,
    type KeybindGroup,
    type KeybindMap,
} from '../../utils/keybinds';

interface KeybindsModalProps {
    isOpen: boolean;
    keybinds: KeybindMap;
    onChange: (keybinds: KeybindMap) => void;
    onClose: () => void;
}

/** The groups in the order the panel lists them. */
const GROUPS: readonly KeybindGroup[] = ['General', 'Game', 'Profiles', 'Mods'];

function KeyCap({ children, tone }: { children: React.ReactNode; tone: 'set' | 'recording' | 'off' }) {
    const toneClass =
        tone === 'recording'
            ? 'border-blue-500 bg-blue-500/10 text-fg-accent ring-2 ring-blue-500/25'
            : tone === 'off'
              ? 'border-gray-700 bg-gray-900 text-gray-500'
              : 'border-gray-600 bg-gray-900 text-gray-100 group-hover/cap:border-gray-500';

    return (
        <span
            className={`inline-flex h-8 min-w-[84px] items-center justify-center rounded-lg border px-3 text-[13px] font-medium tracking-wide transition-colors ${toneClass}`}
        >
            {children}
        </span>
    );
}

/**
 * Editor for the keyboard shortcuts.
 *
 * Changes are handed up rather than written here: the panel opens from
 * Preferences, which saves the whole settings file in one go, and a second
 * writer would race the one already there.
 */
export function KeybindsModal({ isOpen, keybinds, onChange, onClose }: KeybindsModalProps) {
    const [recording, setRecording] = useState<KeybindActionId | null>(null);
    const [search, setSearch] = useState('');

    const conflicts = useMemo(() => findKeybindConflicts(keybinds), [keybinds]);

    const assign = useCallback(
        (id: KeybindActionId, accelerator: string) => {
            onChange({ ...keybinds, [id]: accelerator });
        },
        [keybinds, onChange]
    );

    // While recording, the app must not act on what is being pressed: capturing
    // the event and stopping it here is what keeps ⌘R from launching the game
    // as the user assigns it to something else.
    useEffect(() => {
        if (!isOpen || !recording) return;

        const onKeyDown = (event: KeyboardEvent) => {
            event.preventDefault();
            event.stopPropagation();

            if (event.key === 'Escape') {
                setRecording(null);
                return;
            }

            const accelerator = acceleratorFromEvent(event);
            if (!accelerator) return; // a modifier on its own; keep listening

            if (accelerator === 'Backspace' || accelerator === 'Delete') {
                assign(recording, '');
                setRecording(null);
                return;
            }

            if (!isUsableAccelerator(accelerator)) return; // needs a modifier

            assign(recording, accelerator);
            setRecording(null);
        };

        window.addEventListener('keydown', onKeyDown, true);
        return () => window.removeEventListener('keydown', onKeyDown, true);
    }, [isOpen, recording, assign]);

    // Escape closes the panel, but only when nothing is being recorded — there
    // Escape means "never mind this one shortcut".
    useEffect(() => {
        if (!isOpen || recording) return;
        const onKeyDown = (event: KeyboardEvent) => {
            if (event.key === 'Escape' && !event.defaultPrevented) {
                event.preventDefault();
                // Capture phase, because the app-wide Escape listener was
                // registered first and would otherwise close Preferences out
                // from under this panel.
                event.stopPropagation();
                onClose();
            }
        };
        window.addEventListener('keydown', onKeyDown, true);
        return () => window.removeEventListener('keydown', onKeyDown, true);
    }, [isOpen, recording, onClose]);

    // Adjusted during render rather than in an effect: closing the panel while
    // a key was being recorded must not leave it armed for the next opening.
    const [wasOpen, setWasOpen] = useState(isOpen);
    if (wasOpen !== isOpen) {
        setWasOpen(isOpen);
        if (recording !== null) setRecording(null);
        if (search !== '') setSearch('');
    }

    if (!isOpen) return null;

    const anyChanged = KEYBIND_ACTIONS.some((action) => keybinds[action.id] !== action.defaultAccelerator);
    const searchTerm = search.trim().toLowerCase();
    const visibleActions = KEYBIND_ACTIONS.filter((action) =>
        !searchTerm || [action.label, action.group, ...(action.keywords ?? [])]
            .some((value) => value.toLowerCase().includes(searchTerm))
    );

    return (
        <div className="fixed inset-0 z-[70] flex items-center justify-center p-4">
            <div className="absolute inset-0 bg-black/60 backdrop-blur-sm" onClick={onClose} />

            <div
                className="relative flex max-h-[85vh] w-full max-w-[640px] flex-col overflow-hidden rounded-2xl border border-gray-700 bg-gray-900 shadow-2xl"
                onClick={(e) => e.stopPropagation()}
            >
                <div className="shrink-0 border-b border-gray-800 px-7 pb-5 pt-6">
                    <div className="flex items-start justify-between gap-4">
                      <div>
                        <h2 className="text-2xl font-bold tracking-tight text-white">Keyboard shortcuts</h2>
                        <p className="mt-1 text-[13px] text-gray-400">
                            Select a key, then press a new combination. Delete turns it off.
                        </p>
                      </div>
                      <button
                          onClick={onClose}
                          className="rounded-xl p-2 text-gray-400 transition-all hover:bg-gray-800 hover:text-white active:scale-95 focus:outline-none focus:ring-2 focus:ring-gray-700"
                          aria-label="Close"
                      >
                          <svg className="h-5 w-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                          </svg>
                      </button>
                    </div>

                    <div className="relative mt-5">
                        <AppIcon name="search" className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-gray-500" />
                        <input
                            value={search}
                            onChange={(event) => setSearch(event.target.value)}
                            placeholder="Search actions..."
                            spellCheck={false}
                            className="h-10 w-full rounded-xl border border-gray-700 bg-gray-800 pl-10 pr-10 text-sm text-white placeholder-gray-500 outline-none transition-colors focus:border-blue-500 focus:ring-1 focus:ring-blue-500"
                        />
                        {search && (
                            <button
                                type="button"
                                onClick={() => setSearch('')}
                                className="absolute right-2 top-1/2 -translate-y-1/2 rounded-lg px-2 py-1 text-gray-500 hover:bg-gray-700 hover:text-white"
                                aria-label="Clear action search"
                            >
                                ×
                            </button>
                        )}
                    </div>
                </div>

                <div className="min-h-0 flex-1 space-y-6 overflow-y-auto p-6">
                    {GROUPS.filter((group) => visibleActions.some((action) => action.group === group)).map((group) => (
                        <div key={group} className="space-y-2.5">
                            <h3 className="px-1 text-xs font-semibold uppercase tracking-widest text-gray-400">
                                {group}
                            </h3>

                            <div className="divide-y divide-gray-700/50 overflow-hidden rounded-xl border border-gray-700 bg-gray-800">
                                {visibleActions.filter((action) => action.group === group).map((action) => {
                                    const accelerator = keybinds[action.id];
                                    const isRecording = recording === action.id;
                                    const clashes = !!accelerator && conflicts.has(accelerator);
                                    const changed = accelerator !== action.defaultAccelerator;

                                    return (
                                        <div
                                            key={action.id}
                                            className="flex items-center justify-between gap-4 px-3.5 py-3 transition-colors hover:bg-surface-hover"
                                        >
                                            <div className="flex min-w-0 items-center gap-3">
                                                <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg border border-gray-700 bg-gray-900 text-gray-400">
                                                    <AppIcon name={action.icon} className="h-[17px] w-[17px]" />
                                                </span>
                                                <div className="min-w-0">
                                                <p className="truncate text-[14px] font-medium text-white">{action.label}</p>
                                                {clashes && (
                                                    <p className="mt-0.5 text-[12px] leading-snug text-fg-warning">
                                                        Shortcut already in use
                                                    </p>
                                                )}
                                                </div>
                                            </div>

                                            <div className="flex shrink-0 items-center gap-2">
                                                {changed && !isRecording && (
                                                    <button
                                                        type="button"
                                                        onClick={() => assign(action.id, action.defaultAccelerator)}
                                                        className="rounded-lg p-1.5 text-gray-500 transition-colors hover:bg-gray-700 hover:text-gray-200"
                                                        title={action.defaultAccelerator
                                                            ? `Restore ${formatAccelerator(action.defaultAccelerator)}`
                                                            : 'Remove shortcut'}
                                                        aria-label={action.defaultAccelerator
                                                            ? `Restore the default for ${action.label}`
                                                            : `Remove the shortcut for ${action.label}`}
                                                    >
                                                        <svg
                                                            className="h-4 w-4"
                                                            fill="none"
                                                            stroke="currentColor"
                                                            strokeWidth={1.8}
                                                            viewBox="0 0 24 24"
                                                        >
                                                            <path
                                                                strokeLinecap="round"
                                                                strokeLinejoin="round"
                                                                d="M4 4v5h5M4.6 14a8 8 0 1 0 1.3-6.2L4 9"
                                                            />
                                                        </svg>
                                                    </button>
                                                )}

                                                <button
                                                    type="button"
                                                    onClick={() => setRecording(isRecording ? null : action.id)}
                                                    className={`group/cap rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500/40 ${
                                                        clashes && !isRecording ? 'ring-2 ring-amber-500/40' : ''
                                                    }`}
                                                    aria-label={`Change the shortcut for ${action.label}`}
                                                >
                                                    <KeyCap
                                                        tone={
                                                            isRecording ? 'recording' : accelerator ? 'set' : 'off'
                                                        }
                                                    >
                                                        {isRecording
                                                            ? 'Press keys'
                                                            : accelerator
                                                              ? formatAccelerator(accelerator)
                                                              : 'None'}
                                                    </KeyCap>
                                                </button>
                                            </div>
                                        </div>
                                    );
                                })}
                            </div>
                        </div>
                    ))}

                    {visibleActions.length === 0 && (
                        <div className="py-12 text-center text-sm text-gray-500">No matching actions</div>
                    )}

                </div>

                <div className="flex shrink-0 items-center justify-between gap-3 border-t border-gray-800 bg-gray-900 px-7 py-5">
                    <Button
                        type="button"
                        variant="dangerSecondary"
                        disabled={!anyChanged}
                        onClick={() => onChange({ ...DEFAULT_KEYBINDS })}
                        className="rounded-xl text-[13px]"
                    >
                        Restore defaults
                    </Button>
                    <Button type="button" variant="primary" onClick={onClose}>
                        Done
                    </Button>
                </div>
            </div>
        </div>
    );
}
