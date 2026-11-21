import type { ReactNode } from 'react';
import { ArrowLeft, X } from 'lucide-react';

/**
 * The shell of a message-style dialog: a dimmed backdrop, a rounded panel with
 * a header (title, optional back arrow and badge, close button), a body that
 * scrolls, and an optional footer for the actions.
 *
 * The announcements panel and the update dialog share it so they look and
 * behave alike. It only lays things out: closing on Escape belongs to the
 * caller, because what Escape does differs (the announcements panel steps back
 * from a message to the list first).
 *
 * Use it for new dialogs of this kind instead of writing another backdrop and
 * panel by hand.
 */
export interface PanelDialogProps {
    /** The title in the header. */
    title: string;
    /** Something next to the title, for instance a version badge. */
    badge?: ReactNode;
    onClose: () => void;
    /** Shows a back arrow before the title. */
    onBack?: () => void;
    backLabel?: string;
    /** Buttons along the bottom edge. */
    footer?: ReactNode;
    /**
     * Classes for the scrolling body. The announcements list passes the one
     * that hides the scrollbar so its highlighted rows reach both edges.
     */
    bodyClassName?: string;
    children: ReactNode;
}

export function PanelDialog({
    title,
    badge,
    onClose,
    onBack,
    backLabel = 'Back',
    footer,
    bodyClassName = '',
    children,
}: PanelDialogProps) {
    return (
        <div className="fixed inset-0 z-[100] flex items-center justify-center bg-black/60 p-6" onClick={onClose}>
            <div
                role="dialog"
                aria-modal="true"
                aria-label={title}
                className="flex max-h-[80vh] w-full max-w-xl flex-col overflow-hidden rounded-2xl border border-gray-700 bg-gray-800 shadow-2xl"
                onClick={(event) => event.stopPropagation()}
            >
                <div className="flex items-center justify-between gap-3 border-b border-gray-700/60 px-5 py-4">
                    <div className="flex min-w-0 items-center gap-2">
                        {onBack && (
                            <button
                                type="button"
                                onClick={onBack}
                                className="-ml-1.5 rounded-lg p-1.5 text-gray-400 transition-colors hover:bg-gray-700 hover:text-white"
                                aria-label={backLabel}
                            >
                                <ArrowLeft className="h-5 w-5" aria-hidden="true" />
                            </button>
                        )}
                        <h2 className="text-lg font-semibold text-white">{title}</h2>
                        {badge}
                    </div>
                    <button
                        type="button"
                        onClick={onClose}
                        className="rounded-lg p-1.5 text-gray-400 transition-colors hover:bg-gray-700 hover:text-white"
                        aria-label="Close"
                    >
                        <X className="h-5 w-5" aria-hidden="true" />
                    </button>
                </div>

                {/* No reserved scrollbar gutter: it would keep highlighted rows from reaching the edges. */}
                <div className={`overflow-y-auto ${bodyClassName}`} style={{ scrollbarGutter: 'auto' }}>
                    {children}
                </div>

                {footer && (
                    <div className="flex items-center justify-end gap-2 border-t border-gray-700/60 px-5 py-4">{footer}</div>
                )}
            </div>
        </div>
    );
}
