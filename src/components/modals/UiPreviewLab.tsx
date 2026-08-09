import { useState } from 'react';

import { AppIcon, type IconName } from '../ui/icons';
import { CrossOverGuideModal } from './CrossOverGuideModal';
import { LaunchIssueModal } from './LaunchIssueModal';
import { MacOSGuideModal } from './MacOSGuideModal';
import { ProgressModal } from './ProgressModal';
import { UpdateModal } from './UpdateModal';
import { VerboseLogsWarningModal } from './VerboseLogsWarningModal';

type PreviewId =
    | 'update'
    | 'update-unavailable'
    | 'download'
    | 'verbose-logs'
    | 'steam-cloud'
    | 'launch-failure'
    | 'wine-guide'
    | 'macos-guide';

const PREVIEWS: readonly {
    id: PreviewId;
    title: string;
    description: string;
    group: string;
    icon: IconName;
    tone: string;
}[] = [
    { id: 'update', title: 'Update available', description: 'A downloadable release with notes.', group: 'Updates', icon: 'update', tone: 'text-fg-success' },
    { id: 'update-unavailable', title: 'Update unavailable', description: 'Release found, but without an asset for this system.', group: 'Updates', icon: 'update', tone: 'text-gray-400' },
    { id: 'download', title: 'Download progress', description: 'Transfer speed, progress and cancellation state.', group: 'Operations', icon: 'install', tone: 'text-fg-accent' },
    { id: 'verbose-logs', title: 'Verbose log warning', description: 'The 5 MB troubleshooting-log reminder.', group: 'Warnings', icon: 'warning', tone: 'text-fg-warning' },
    { id: 'steam-cloud', title: 'Steam Cloud conflict', description: 'A Steam-side launch blocker.', group: 'Launch issues', icon: 'warning', tone: 'text-fg-warning' },
    { id: 'launch-failure', title: 'Generic launch failure', description: 'Fallback error presentation.', group: 'Launch issues', icon: 'warning', tone: 'text-fg-danger' },
    { id: 'wine-guide', title: 'Wine configuration', description: 'The one-time CrossOver/Wine setup guide.', group: 'Guides', icon: 'settings', tone: 'text-purple-400' },
    { id: 'macos-guide', title: 'macOS launch option', description: 'The Steam launch-option setup guide.', group: 'Guides', icon: 'settings', tone: 'text-gray-300' },
];

export function UiPreviewLab({ isOpen, onClose }: { isOpen: boolean; onClose: () => void }) {
    const [activePreview, setActivePreview] = useState<PreviewId | null>(null);

    if (!isOpen) return null;

    const closePreview = () => setActivePreview(null);

    if (activePreview === 'update' || activePreview === 'update-unavailable') {
        const available = activePreview === 'update';
        return (
            <UpdateModal
                updateInfo={{
                    available: true,
                    version: 'v0.9.0-preview',
                    notes: '## Preview release\n\n- Faster profile sync\n- Improved Spotlight navigation\n- Several reliability fixes\n\nThis is test data. Nothing will be downloaded.',
                    download_url: available ? 'preview://update' : undefined,
                }}
                onClose={closePreview}
                onUpdate={closePreview}
            />
        );
    }

    if (activePreview === 'download') {
        return (
            <ProgressModal
                isOpen
                title="Downloading BepInExPack"
                progress={67}
                currentTask="Extracting package files…"
                downloadedBytes={35_127_296}
                totalBytes={52_428_800}
                downloadSpeedBps={4_718_592}
                activeDownloads={2}
                onCancel={closePreview}
                onMinimize={closePreview}
            />
        );
    }

    if (activePreview === 'verbose-logs') {
        return (
            <VerboseLogsWarningModal
                isOpen
                bytes={6.4 * 1024 * 1024}
                verboseLogging
                onVerboseLoggingChange={closePreview}
                onClearLogs={closePreview}
                onClose={closePreview}
                onDontShowAgain={closePreview}
            />
        );
    }

    if (activePreview === 'steam-cloud' || activePreview === 'launch-failure') {
        return (
            <LaunchIssueModal
                issue={activePreview === 'steam-cloud'
                    ? {
                        title: 'Steam Cloud Conflict',
                        message: 'Steam is waiting for you to resolve a Cloud conflict before the game can start. Open Steam, choose which save to keep, then try again.',
                        pointsAtSteam: true,
                    }
                    : {
                        title: 'Game Launch Failed',
                        message: 'The game could not be started. Check its configured path and try launching it once without mods.',
                        pointsAtSteam: false,
                    }}
                onClose={closePreview}
            />
        );
    }

    if (activePreview === 'wine-guide') {
        return <CrossOverGuideModal isOpen onClose={closePreview} onDontShowAgain={closePreview} />;
    }

    if (activePreview === 'macos-guide') {
        return <MacOSGuideModal isOpen onClose={closePreview} onDontShowAgain={closePreview} />;
    }

    const groups = [...new Set(PREVIEWS.map((preview) => preview.group))];

    return (
        <div className="fixed inset-0 z-[90] flex items-center justify-center bg-black/85 p-5 backdrop-blur-md">
            <div className="flex max-h-[90vh] w-full max-w-4xl flex-col overflow-hidden rounded-2xl border border-gray-700 bg-gray-900 shadow-2xl">
                <div className="flex shrink-0 items-center justify-between gap-4 border-b border-gray-800 px-7 py-6">
                    <div>
                        <div className="flex items-center gap-3">
                            <span className="flex h-10 w-10 items-center justify-center rounded-xl border border-purple-500/30 bg-purple-500/10 text-purple-400">
                                <AppIcon name="layout" className="h-5 w-5" />
                            </span>
                            <div>
                                <h2 className="text-2xl font-bold text-white">UI Preview Lab</h2>
                                <p className="mt-0.5 text-sm text-gray-400">Rare screens with safe preview data. Actions never touch the app or your files.</p>
                            </div>
                        </div>
                    </div>
                    <button
                        type="button"
                        onClick={onClose}
                        className="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl text-gray-400 transition-colors hover:bg-gray-800 hover:text-white"
                        aria-label="Close UI Preview Lab"
                    >
                        <AppIcon name="close" className="h-5 w-5" />
                    </button>
                </div>

                <div className="min-h-0 flex-1 space-y-7 overflow-y-auto p-7">
                    {groups.map((group) => (
                        <section key={group} className="space-y-3">
                            <h3 className="px-1 text-xs font-semibold uppercase tracking-widest text-gray-400">{group}</h3>
                            <div className="grid gap-3 md:grid-cols-2">
                                {PREVIEWS.filter((preview) => preview.group === group).map((preview) => (
                                    <button
                                        key={preview.id}
                                        type="button"
                                        onClick={() => setActivePreview(preview.id)}
                                        className="group flex items-center gap-4 rounded-2xl border border-gray-700 bg-gray-800 p-4 text-left transition-all hover:border-gray-600 hover:bg-surface-hover active:scale-[0.985]"
                                    >
                                        <span className={`flex h-11 w-11 shrink-0 items-center justify-center rounded-xl border border-gray-700 bg-gray-900 ${preview.tone}`}>
                                            <AppIcon name={preview.icon} className="h-5 w-5" />
                                        </span>
                                        <span className="min-w-0 flex-1">
                                            <span className="block text-[15px] font-medium text-white">{preview.title}</span>
                                            <span className="mt-0.5 block text-[13px] leading-snug text-gray-400">{preview.description}</span>
                                        </span>
                                        <span className="text-gray-500 transition-transform group-hover:translate-x-0.5">›</span>
                                    </button>
                                ))}
                            </div>
                        </section>
                    ))}
                </div>
            </div>
        </div>
    );
}
