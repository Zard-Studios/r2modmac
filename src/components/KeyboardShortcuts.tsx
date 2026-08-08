import { useEffect, useRef } from 'react';

import { useKeybindStore } from '../store/useKeybindStore';
import { acceleratorFromEvent, actionForEvent, isTextEditingAccelerator, type KeybindActionId } from '../utils/keybinds';

type Handlers = Partial<Record<KeybindActionId, () => void>>;

interface KeyboardShortcutsProps {
    /** Only the actions reachable from where this is rendered. */
    handlers: Handlers;
    /** False while a dialog owns the keyboard. */
    enabled?: boolean;
}

/**
 * Runs the keyboard shortcuts for whatever is on screen.
 *
 * A component rather than a hook in `App`, because the actions it fires are
 * defined per view: the launch and apply handlers only exist while a profile is
 * open, so the listener should only exist there too. Rendering it inside a view
 * is what ties the two together.
 */
export function KeyboardShortcuts({ handlers, enabled = true }: KeyboardShortcutsProps) {
    const keybinds = useKeybindStore((state) => state.keybinds);

    // Read through a ref so a handler that changes identity every render does
    // not tear down and re-add the listener on each pass.
    const handlersRef = useRef(handlers);
    useEffect(() => {
        handlersRef.current = handlers;
    }, [handlers]);

    useEffect(() => {
        if (!enabled) return;

        const onKeyDown = (event: KeyboardEvent) => {
            if (event.defaultPrevented) return;

            // Only the editing gestures are surrendered to a focused field.
            // Bailing out of every shortcut instead would strand the ones that
            // matter most on screens whose search box takes focus on arrival —
            // the game list being exactly that.
            const target = event.target as HTMLElement | null;
            const inTextField =
                !!target &&
                (target.tagName === 'INPUT' ||
                    target.tagName === 'TEXTAREA' ||
                    target.tagName === 'SELECT' ||
                    target.isContentEditable);

            const accelerator = acceleratorFromEvent(event);
            if (!accelerator) return;
            if (inTextField && isTextEditingAccelerator(accelerator)) return;

            const action = actionForEvent(event, keybinds);
            if (!action) return;

            const handler = handlersRef.current[action];
            if (!handler) return;

            event.preventDefault();
            handler();
        };

        window.addEventListener('keydown', onKeyDown);
        return () => window.removeEventListener('keydown', onKeyDown);
    }, [enabled, keybinds]);

    return null;
}
