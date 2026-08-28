import { Button, DialogLayer } from '../ui';
import { AppIcon } from '../ui/icons';

export type ConfirmTone = 'default' | 'danger';

export interface ConfirmRequest {
    title: string;
    message: string;
    confirmLabel?: string;
    cancelLabel?: string;
    tone?: ConfirmTone;
}

interface ConfirmModalProps {
    request: ConfirmRequest | null;
    onResolve: (confirmed: boolean) => void;
}

/**
 * The app's own replacement for the operating system's confirmation dialog.
 *
 * A native dialog steals the window, ignores the chosen theme and looks like a
 * different program on every platform, so every confirmation the app asks for
 * goes through this modal instead.
 */
export function ConfirmModal({ request, onResolve }: ConfirmModalProps) {
    if (!request) return null;

    const isDanger = request.tone === 'danger';

    return (
        <div
            className="fixed inset-0 z-[90] flex items-center justify-center bg-black/70 p-4 backdrop-blur-sm"
            onMouseDown={(event) => { if (event.target === event.currentTarget) onResolve(false); }}
        >
            <DialogLayer
                onDismiss={() => onResolve(false)}
                aria-labelledby="confirm-modal-title"
                className="w-full max-w-md overflow-hidden rounded-2xl border border-gray-700 bg-gray-800 shadow-2xl"
            >
                <form onSubmit={(event) => { event.preventDefault(); onResolve(true); }}>
                    <div className="flex items-start gap-4 border-b border-gray-700 px-5 py-4">
                        <span className={`flex h-11 w-11 shrink-0 items-center justify-center rounded-xl border ${isDanger ? 'border-fg-danger/30 bg-fg-danger/10 text-fg-danger' : 'border-gray-600 bg-gray-700/60 text-sky-400'}`}>
                            <AppIcon name={isDanger ? 'warning' : 'apply'} className="h-6 w-6" />
                        </span>
                        <div className="min-w-0">
                            <h2 id="confirm-modal-title" className="text-lg font-bold text-white">{request.title}</h2>
                            <p className="mt-1 whitespace-pre-line text-sm leading-relaxed text-gray-400">{request.message}</p>
                        </div>
                    </div>
                    <div className="grid grid-cols-2 gap-3 p-4">
                        <Button type="button" variant="secondary" className="rounded-xl py-2.5" onClick={() => onResolve(false)}>
                            {request.cancelLabel || 'Cancel'}
                        </Button>
                        <Button
                            type="submit"
                            data-dialog-primary
                            variant={isDanger ? 'danger' : 'primary'}
                            className="rounded-xl py-2.5"
                        >
                            {request.confirmLabel || 'Continue'}
                        </Button>
                    </div>
                </form>
            </DialogLayer>
        </div>
    );
}
