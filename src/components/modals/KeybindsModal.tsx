import { useCallback, useEffect, useMemo, useState } from 'react';

import {
    DEFAULT_KEYBINDS,
    KEYBIND_ACTIONS,
    acceleratorFromEvent,
    findKeybindConflicts,
    formatAccelerator,
    isUsableAccelerator,
    type KeybindActionId,
    type KeybindMap,
} from '../../utils/keybinds';

interface KeybindsModalProps {
    isOpen: boolean;
    keybinds: KeybindMap;
    onChange: (keybinds: KeybindMap) => void;
    onClose: () => void;
}

/** The groups in the order the panel lists them. */
const GROUPS = ['Game', 'Profiles'] as const;

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
    }

    if (!isOpen) return null;

    const anyChanged = KEYBIND_ACTIONS.some((action) => keybinds[action.id] !== action.defaultAccelerator);

    return (
        <div className="fixed inset-0 z-[70] flex items-center justify-center p-4">
            <div className="absolute inset-0 bg-black/60 backdrop-blur-sm" onClick={onClose} />

            <div
                className="relative flex max-h-[85vh] w-full max-w-[640px] flex-col overflow-hidden rounded-2xl border border-gray-700 bg-gray-900 shadow-2xl"
                onClick={(e) => e.stopPropagation()}
            >
                <div className="flex shrink-0 items-center justify-between border-b border-gray-800 px-7 py-6">
                    <div>
                        <h2 className="text-2xl font-bold tracking-tight text-white">Keyboard shortcuts</h2>
                        <p className="mt-1 text-[13px] text-gray-400">
                            Click a shortcut and press the keys you want. Delete clears it.
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

                <div className="min-h-0 flex-1 space-y-8 overflow-y-auto p-7">
                    {GROUPS.map((group) => (
                        <div key={group} className="space-y-3">
                            <h3 className="px-1 text-xs font-semibold uppercase tracking-widest text-gray-400">
                                {group}
                            </h3>

                            <div className="divide-y divide-gray-700/50 overflow-hidden rounded-2xl border border-gray-700 bg-gray-800">
                                {KEYBIND_ACTIONS.filter((action) => action.group === group).map((action) => {
                                    const accelerator = keybinds[action.id];
                                    const isRecording = recording === action.id;
                                    const clashes = !!accelerator && conflicts.has(accelerator);
                                    const changed = accelerator !== action.defaultAccelerator;

                                    return (
                                        <div
                                            key={action.id}
                                            className="flex items-center justify-between gap-4 p-4 transition-colors hover:bg-gray-750"
                                        >
                                            <div className="min-w-0">
                                                <p className="text-[15px] font-medium text-white">{action.label}</p>
                                                <p className="mt-0.5 text-[13px] leading-snug text-gray-400">
                                                    {clashes
                                                        ? 'Another action uses this combination.'
                                                        : action.description}
                                                </p>
                                            </div>

                                            <div className="flex shrink-0 items-center gap-2">
                                                {changed && !isRecording && (
                                                    <button
                                                        type="button"
                                                        onClick={() => assign(action.id, action.defaultAccelerator)}
                                                        className="rounded-lg p-1.5 text-gray-500 transition-colors hover:bg-gray-700 hover:text-gray-200"
                                                        title={`Restore ${formatAccelerator(action.defaultAccelerator)}`}
                                                        aria-label={`Restore the default for ${action.label}`}
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
                                                              : 'Off'}
                                                    </KeyCap>
                                                </button>
                                            </div>
                                        </div>
                                    );
                                })}
                            </div>
                        </div>
                    ))}

                    <p className="px-1 text-[12px] leading-relaxed text-gray-500">
                        A shortcut needs at least one modifier, so it cannot fire while you are typing. Shortcuts stay
                        out of the way while a text field has focus.
                    </p>
                </div>

                <div className="flex shrink-0 items-center justify-between gap-3 border-t border-gray-800 bg-gray-900 px-7 py-5">
                    <button
                        type="button"
                        disabled={!anyChanged}
                        onClick={() => onChange({ ...DEFAULT_KEYBINDS })}
                        className="rounded-xl px-4 py-2 text-[13px] font-medium text-gray-400 transition-colors hover:bg-gray-800 hover:text-white disabled:cursor-not-allowed disabled:opacity-40 disabled:hover:bg-transparent disabled:hover:text-gray-400"
                    >
                        Restore defaults
                    </button>
                    <button
                        type="button"
                        onClick={onClose}
                        className="rounded-xl border border-gray-600 px-5 py-2 text-[13px] font-medium text-gray-200 transition-colors hover:border-gray-500 hover:bg-gray-700"
                    >
                        Done
                    </button>
                </div>
            </div>
        </div>
    );
}
