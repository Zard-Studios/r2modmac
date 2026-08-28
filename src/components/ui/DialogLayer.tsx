import React, { useEffect, useRef } from 'react';

import { dialogStack, isPlainEscape } from '../../utils/dialogStack';

interface DialogLayerProps extends React.HTMLAttributes<HTMLDivElement> {
    onDismiss?: () => void;
    dismissible?: boolean;
}

const FOCUSABLE = [
    '[data-dialog-primary]:not(:disabled)',
    'button:not(:disabled)',
    'input:not(:disabled)',
    'select:not(:disabled)',
    'textarea:not(:disabled)',
    '[href]',
    '[tabindex]:not([tabindex="-1"])',
].join(',');

/** Shared dialog semantics: topmost Escape, initial focus and focus restore. */
export function DialogLayer({
    onDismiss,
    dismissible = true,
    tabIndex = -1,
    children,
    ...props
}: DialogLayerProps) {
    const ref = useRef<HTMLDivElement>(null);
    const dismissRef = useRef(onDismiss);
    const dismissibleRef = useRef(dismissible);

    useEffect(() => {
        dismissRef.current = onDismiss;
        dismissibleRef.current = dismissible;
    }, [dismissible, onDismiss]);

    useEffect(() => {
        const token = Symbol('dialog');
        const previousFocus = document.activeElement instanceof HTMLElement
            ? document.activeElement
            : null;
        dialogStack.register(token);

        const focusTarget = ref.current?.querySelector<HTMLElement>(FOCUSABLE) ?? ref.current;
        window.requestAnimationFrame(() => focusTarget?.focus({ preventScroll: true }));

        const onKeyDown = (event: KeyboardEvent) => {
            if (!dialogStack.isTop(token) || !isPlainEscape(event)) return;
            event.preventDefault();
            event.stopPropagation();
            if (dismissibleRef.current) dismissRef.current?.();
        };
        window.addEventListener('keydown', onKeyDown, true);

        return () => {
            const wasTop = dialogStack.isTop(token);
            window.removeEventListener('keydown', onKeyDown, true);
            dialogStack.unregister(token);
            if (wasTop && previousFocus?.isConnected) {
                window.requestAnimationFrame(() => previousFocus.focus({ preventScroll: true }));
            }
        };
    }, []);

    return (
        <div ref={ref} role="dialog" aria-modal="true" tabIndex={tabIndex} {...props}>
            {children}
        </div>
    );
}
