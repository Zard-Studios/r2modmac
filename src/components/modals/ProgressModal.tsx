import { useEffect } from 'react';
import type { ProgressState } from '../../types/progress';

interface ProgressModalProps {
    isOpen: boolean;
    title: string;
    progress: number;
    currentTask: string;
    downloadSpeedBps?: ProgressState['downloadSpeedBps'];
    downloadedBytes?: ProgressState['downloadedBytes'];
    totalBytes?: ProgressState['totalBytes'];
    activeDownloads?: ProgressState['activeDownloads'];
    onCancel?: () => void | Promise<void>;
    onMinimize?: () => void;
    isCancelling?: boolean;
}

const formatBytes = (bytes?: number) => {
    if (typeof bytes !== 'number' || Number.isNaN(bytes) || bytes <= 0) return '0 B';
    const units = ['B', 'KB', 'MB', 'GB'];
    let value = bytes;
    let unitIndex = 0;
    while (value >= 1024 && unitIndex < units.length - 1) {
        value /= 1024;
        unitIndex++;
    }
    return `${value.toFixed(value >= 100 ? 0 : value >= 10 ? 1 : 2)} ${units[unitIndex]}`;
};

const formatSpeed = (speedBps?: number) => {
    if (typeof speedBps !== 'number' || Number.isNaN(speedBps) || speedBps <= 0) return '--';
    return `${formatBytes(speedBps)}/s`;
};

export function ProgressModal({
    isOpen,
    title,
    progress,
    currentTask,
    downloadSpeedBps,
    downloadedBytes,
    totalBytes,
    activeDownloads,
    onCancel,
    onMinimize,
    isCancelling = false,
}: ProgressModalProps) {
    useEffect(() => {
        if (!isOpen || !onMinimize) return;
        const handleKeyDown = (event: KeyboardEvent) => {
            if (event.key === 'Escape') {
                event.preventDefault();
                event.stopImmediatePropagation();
                onMinimize();
            }
        };
        window.addEventListener('keydown', handleKeyDown);
        return () => window.removeEventListener('keydown', handleKeyDown);
    }, [isOpen, onMinimize]);

    if (!isOpen) return null;
    const hasDownloadStats = typeof downloadedBytes === 'number' || typeof totalBytes === 'number' || typeof downloadSpeedBps === 'number';
    const transferLabel = typeof totalBytes === 'number' && totalBytes > 0
        ? `${formatBytes(downloadedBytes)} / ${formatBytes(totalBytes)}`
        : `${formatBytes(downloadedBytes)} downloaded`;

    return (
        <div className="fixed inset-0 bg-black/70 backdrop-blur-sm flex items-center justify-center z-[60] p-4">
            <div className="relative bg-gray-800 rounded-xl p-6 max-w-md w-full min-w-0 border border-gray-700 shadow-2xl overflow-hidden">
                <div className="flex items-start gap-4 mb-4 min-w-0 pr-8">
                    <h3 className="text-xl font-bold text-white min-w-0 break-words">{title}</h3>
                    {onMinimize && (
                        <button
                            type="button"
                            onClick={onMinimize}
                            className="absolute right-3 top-3 h-8 w-8 rounded-lg flex items-center justify-center text-gray-400 hover:text-white hover:bg-gray-700 transition-colors"
                            aria-label="Continue in background"
                            title="Continue in background"
                        >
                            <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" aria-hidden="true">
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                            </svg>
                        </button>
                    )}
                </div>

                <div className="mb-2 flex h-5 items-center justify-between gap-3 text-sm text-gray-400 min-w-0">
                    <span className="min-w-0 truncate" title={isCancelling ? 'Stopping...' : currentTask}>
                        {isCancelling ? 'Stopping...' : currentTask}
                    </span>
                    <span className="shrink-0 tabular-nums">{Math.round(progress)}%</span>
                </div>

                <div className="w-full bg-gray-700 rounded-full h-2.5 mb-4 overflow-hidden">
                    <div
                        className="bg-blue-600 h-2.5 rounded-full transition-all duration-300 ease-out"
                        style={{ width: `${Math.min(100, Math.max(0, progress))}%` }}
                    />
                </div>

                <div
                    className={`mb-4 flex h-4 items-center justify-between gap-3 text-xs text-gray-400 min-w-0 ${hasDownloadStats ? '' : 'invisible'}`}
                    aria-hidden={!hasDownloadStats}
                >
                        <span className="min-w-0 truncate">{hasDownloadStats ? transferLabel : 'Waiting for transfer'}</span>
                        <span className="shrink-0 tabular-nums">
                            {formatSpeed(downloadSpeedBps)}
                            {typeof activeDownloads === 'number' && activeDownloads > 1 ? ` • ${activeDownloads} parallel` : ''}
                        </span>
                </div>

                {onCancel && (
                    <div className="flex justify-center">
                        <button
                            type="button"
                            onClick={onCancel}
                            disabled={isCancelling}
                            className="rounded-lg bg-red-600 px-5 py-2 text-sm font-semibold text-on-danger hover:bg-red-500 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
                        >
                            {isCancelling ? 'Stopping...' : 'Stop'}
                        </button>
                    </div>
                )}
            </div>
        </div>
    );
}
