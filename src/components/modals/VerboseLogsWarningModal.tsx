import { useState } from 'react';

import { Button, Checkbox } from '../ui';
import { AppIcon } from '../ui/icons';
import { Toggle } from '../ui/Toggle';

interface VerboseLogsWarningModalProps {
    isOpen: boolean;
    bytes: number;
    verboseLogging: boolean;
    onVerboseLoggingChange: (enabled: boolean) => void | Promise<void>;
    onClearLogs: () => void | Promise<void>;
    onClose: () => void;
    onDontShowAgain: () => void | Promise<void>;
}

function formatSize(bytes: number): string {
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function VerboseLogsWarningModal({
    isOpen,
    bytes,
    verboseLogging,
    onVerboseLoggingChange,
    onClearLogs,
    onClose,
    onDontShowAgain,
}: VerboseLogsWarningModalProps) {
    const [dontShowAgain, setDontShowAgain] = useState(false);
    const [clearing, setClearing] = useState(false);

    if (!isOpen) return null;

    const close = () => {
        if (dontShowAgain) void onDontShowAgain();
        onClose();
    };

    const clearLogs = async () => {
        setClearing(true);
        try {
            await onClearLogs();
        } finally {
            setClearing(false);
        }
    };

    return (
        <div
            className="fixed inset-0 z-[70] flex items-center justify-center bg-black/80 p-4 backdrop-blur-sm"
            onClick={close}
        >
            <div
                className="w-full max-w-xl overflow-hidden rounded-2xl border border-amber-500/30 bg-gray-900 shadow-2xl"
                onClick={(event) => event.stopPropagation()}
            >
                <div className="flex items-start justify-between gap-4 border-b border-gray-800 p-6">
                    <div className="flex min-w-0 items-start gap-4">
                        <span className="flex h-11 w-11 shrink-0 items-center justify-center rounded-xl border border-amber-500/30 bg-amber-500/10 text-fg-warning">
                            <AppIcon name="warning" className="h-6 w-6" />
                        </span>
                        <div>
                            <h2 className="text-xl font-bold text-white">Verbose logging is still enabled</h2>
                            <p className="mt-1 text-sm leading-relaxed text-gray-400">
                                The app logs now use {formatSize(bytes)}. Verbose logging is intended for troubleshooting and can be disabled after reproducing the problem.
                            </p>
                        </div>
                    </div>
                    <button
                        type="button"
                        onClick={close}
                        className="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl text-gray-400 transition-colors hover:bg-gray-800 hover:text-white"
                        aria-label="Close"
                    >
                        <AppIcon name="close" className="h-5 w-5" />
                    </button>
                </div>

                <div className="space-y-4 p-6">
                    <div className="flex items-center justify-between gap-4 rounded-2xl border border-gray-700 bg-gray-800 p-4">
                        <div className="flex min-w-0 items-center gap-4">
                            <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl border border-gray-700 bg-gray-700/60 text-sky-400">
                                <AppIcon name="logs" className="h-5 w-5" />
                            </span>
                            <div>
                                <p className="text-[15px] font-medium text-white">Verbose app logging</p>
                                <p className="mt-0.5 text-[13px] leading-snug text-gray-400">Turn it off if you have finished collecting troubleshooting details.</p>
                            </div>
                        </div>
                        <Toggle
                            value={verboseLogging}
                            onChange={(enabled) => void onVerboseLoggingChange(enabled)}
                            label="Verbose app logging"
                        />
                    </div>

                    <div className="flex items-center justify-between gap-4 rounded-xl border border-gray-800 bg-gray-900 px-4 py-3">
                        <div>
                            <p className="text-sm font-medium text-white">Free log storage</p>
                            <p className="mt-0.5 text-xs text-gray-400">Clear the existing diagnostic logs without changing the option above.</p>
                        </div>
                        <Button
                            type="button"
                            variant="dangerSecondary"
                            onClick={() => void clearLogs()}
                            disabled={clearing}
                            className="shrink-0 rounded-xl"
                        >
                            {clearing ? 'Clearing…' : 'Clear logs'}
                        </Button>
                    </div>
                </div>

                <div className="flex items-center justify-between gap-4 border-t border-gray-800 bg-gray-900 p-5">
                    <Checkbox checked={dontShowAgain} onChange={setDontShowAgain} label="Don't show again" />
                    <Button type="button" variant="primary" onClick={close}>Done</Button>
                </div>
            </div>
        </div>
    );
}
