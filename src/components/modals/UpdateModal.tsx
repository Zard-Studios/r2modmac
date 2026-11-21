import React, { Suspense, lazy } from 'react';
import type { UpdateInfo } from '../../types/electron';
import { PanelDialog } from '../ui/PanelDialog';

// The release notes are shown by the same code as the announcements and the mod
// descriptions. It loads when this dialog opens.
const RichText = lazy(() => import('../RichText').then((module) => ({ default: module.RichText })));

interface UpdateModalProps {
    updateInfo: UpdateInfo;
    onClose: () => void;
    onUpdate: () => void;
}

export const UpdateModal: React.FC<UpdateModalProps> = ({ updateInfo, onClose, onUpdate }) => {
    const canInstall = !!updateInfo.download_url;
    const notes = updateInfo.notes?.trim() ?? '';

    return (
        <PanelDialog
            title="Update Available"
            badge={
                <span className="rounded border border-green-400/20 bg-green-400/10 px-2 py-0.5 text-xs font-bold text-fg-success">
                    {updateInfo.version}
                </span>
            }
            onClose={onClose}
            footer={
                <>
                    <button
                        onClick={onClose}
                        className="rounded-lg px-3 py-2 text-sm text-gray-400 transition-all hover:bg-gray-700/50 hover:text-white"
                    >
                        Later
                    </button>
                    <button
                        onClick={onUpdate}
                        disabled={!canInstall}
                        title={canInstall ? 'Install update' : 'No compatible update asset is available for this system'}
                        className="flex items-center gap-1.5 rounded-lg bg-green-600 px-4 py-2 text-sm font-semibold text-on-success transition-all hover:bg-green-500 disabled:cursor-not-allowed disabled:bg-gray-700 disabled:text-gray-400"
                    >
                        <svg className="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
                        </svg>
                        {canInstall ? 'Update' : 'Unavailable for this system'}
                    </button>
                </>
            }
        >
            <div className="px-5 py-4">
                {notes ? (
                    <Suspense fallback={null}>
                        <RichText markdown={notes} mode="announcement" />
                    </Suspense>
                ) : (
                    <p className="text-sm text-gray-400">No release notes for this version.</p>
                )}
            </div>
        </PanelDialog>
    );
};
