import type { ProgressState } from '../../types/progress';

interface ProgressModalProps {
    isOpen: boolean;
    title: string;
    progress: number; // 0 to 100
    currentTask: string;
    downloadSpeedBps?: ProgressState['downloadSpeedBps'];
    downloadedBytes?: ProgressState['downloadedBytes'];
    totalBytes?: ProgressState['totalBytes'];
    activeDownloads?: ProgressState['activeDownloads'];
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
}: ProgressModalProps) {
    if (!isOpen) return null;
    const hasDownloadStats = typeof downloadedBytes === 'number' || typeof totalBytes === 'number' || typeof downloadSpeedBps === 'number';
    const transferLabel = typeof totalBytes === 'number' && totalBytes > 0
        ? `${formatBytes(downloadedBytes)} / ${formatBytes(totalBytes)}`
        : `${formatBytes(downloadedBytes)} downloaded`;

    return (
        <div className="fixed inset-0 bg-black/70 backdrop-blur-sm flex items-center justify-center z-[60] p-4">
            <div className="bg-gray-800 rounded-xl p-6 max-w-md w-full border border-gray-700 shadow-2xl">
                <h3 className="text-xl font-bold text-white mb-4">{title}</h3>

                <div className="mb-2 flex justify-between text-sm text-gray-400">
                    <span>{currentTask}</span>
                    <span>{Math.round(progress)}%</span>
                </div>

                <div className="w-full bg-gray-700 rounded-full h-2.5 mb-4 overflow-hidden">
                    <div
                        className="bg-blue-600 h-2.5 rounded-full transition-all duration-300 ease-out"
                        style={{ width: `${progress}%` }}
                    ></div>
                </div>

                {hasDownloadStats && (
                    <div className="mb-4 flex items-center justify-between text-xs text-gray-400">
                        <span>{transferLabel}</span>
                        <span>
                            {formatSpeed(downloadSpeedBps)}
                            {typeof activeDownloads === 'number' && activeDownloads > 1 ? ` • ${activeDownloads} parallel` : ''}
                        </span>
                    </div>
                )}

                <div className="flex justify-center">
                    <svg className="animate-spin h-8 w-8 text-blue-500" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
                        <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4"></circle>
                        <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                    </svg>
                </div>
            </div>
        </div>
    );
}
