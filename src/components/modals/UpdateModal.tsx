import React from 'react';
import type { UpdateInfo } from '../../types/electron';
import DOMPurify from 'dompurify';
import { marked } from 'marked';

interface UpdateModalProps {
    updateInfo: UpdateInfo;
    onClose: () => void;
    onUpdate: () => void;
}

export const UpdateModal: React.FC<UpdateModalProps> = ({ updateInfo, onClose, onUpdate }) => {
    const canInstall = !!updateInfo.download_url;
    const getHtml = () => {
        const raw = marked.parse(updateInfo.notes, { breaks: true, gfm: true }) as string;
        return { __html: DOMPurify.sanitize(raw) };
    };

    return (
        <div
            className="fixed inset-0 bg-black/50 backdrop-blur-sm flex items-center justify-center z-50 p-3"
            onClick={onClose}
        >
            <div
                className="bg-gray-800 rounded-xl max-w-xl w-full border border-gray-700 shadow-2xl flex flex-col"
                style={{ maxHeight: 'calc(100vh - 48px)' }}
                onClick={(e) => e.stopPropagation()}
            >
                {/* Compact Header */}
                <div className="flex-shrink-0 p-4 border-b border-gray-700 bg-gradient-to-br from-gray-900 to-gray-800 rounded-t-xl">
                    <div className="flex items-center gap-3">
                        <img
                            src="/app-icon.png"
                            alt="r2modmac"
                            className="w-10 h-10 flex-shrink-0 rounded-lg"
                        />
                        <div className="flex-1 min-w-0">
                            <div className="flex items-center gap-2 flex-wrap">
                                <h2 className="text-lg font-bold text-white">Update Available</h2>
                                <span className="text-xs font-bold text-fg-success bg-green-400/10 px-2 py-0.5 rounded border border-green-400/20">
                                    {updateInfo.version}
                                </span>
                            </div>
                        </div>
                    </div>
                </div>

                {/* Scrollable Content */}
                <div className="flex-1 overflow-y-auto p-4" style={{ minHeight: 0 }}>
                    <div
                        className="prose prose-invert prose-sm max-w-none 
                                   prose-headings:text-white prose-headings:text-base prose-headings:mt-3 prose-headings:mb-2
                                   prose-p:my-1 prose-ul:my-1 prose-li:my-0.5
                                   prose-a:text-fg-success hover:prose-a:text-fg-success"
                        dangerouslySetInnerHTML={getHtml()}
                    />
                </div>

                {/* Compact Footer */}
                <div className="flex-shrink-0 p-4 border-t border-gray-700 bg-gray-900/50 rounded-b-xl flex justify-end gap-2">
                    <button
                        onClick={onClose}
                        className="px-3 py-2 rounded-lg text-sm text-gray-400 hover:text-white hover:bg-gray-700/50 transition-all"
                    >
                        Later
                    </button>
                    <button
                        onClick={onUpdate}
                        disabled={!canInstall}
                        title={canInstall ? 'Install update' : 'No compatible update asset is available for this system'}
                        className="px-4 py-2 bg-green-600 hover:bg-green-500 text-white rounded-lg text-sm font-semibold transition-all flex items-center gap-1.5 disabled:bg-gray-700 disabled:text-gray-400 disabled:cursor-not-allowed"
                    >
                        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
                        </svg>
                        {canInstall ? 'Update' : 'Unavailable for this system'}
                    </button>
                </div>
            </div>
        </div>
    );
};
