import React from 'react';
import type { UpdateInfo } from '../types/electron';
import DOMPurify from 'dompurify';
import { marked } from 'marked';

interface UpdateModalProps {
    updateInfo: UpdateInfo;
    onClose: () => void;
    onUpdate: () => void;
}

export const UpdateModal: React.FC<UpdateModalProps> = ({ updateInfo, onClose, onUpdate }) => {
    // Parse markdown notes
    const getHtml = () => {
        const raw = marked.parse(updateInfo.notes) as string;
        return { __html: DOMPurify.sanitize(raw) };
    };

    return (
        <div className="fixed inset-0 bg-black/80 flex items-center justify-center z-50 p-4">
            <div className="bg-gray-800 rounded-lg max-w-2xl w-full border border-green-500/50 shadow-2xl flex flex-col max-h-[80vh]">
                <div className="p-6 border-b border-gray-700 bg-gray-900/50">
                    <h2 className="text-2xl font-bold text-white flex items-center gap-2">
                        <span className="text-green-400">🚀</span>
                        New Update Available!
                        <span className="ml-2 text-sm bg-green-500/20 text-green-400 px-2 py-1 rounded">
                            {updateInfo.version}
                        </span>
                    </h2>
                </div>

                <div className="p-6 overflow-y-auto flex-1 custom-scrollbar">
                    <div className="prose prose-invert max-w-none">
                        <div dangerouslySetInnerHTML={getHtml()} />
                    </div>
                </div>

                <div className="p-6 border-t border-gray-700 bg-gray-900/50 flex justify-end gap-3">
                    <button
                        onClick={onClose}
                        className="px-4 py-2 text-gray-400 hover:text-white transition-colors"
                    >
                        Remind Me Later
                    </button>
                    <button
                        onClick={onUpdate}
                        className="px-6 py-2 bg-green-600 hover:bg-green-500 text-white rounded font-medium transition-colors shadow-lg shadow-green-900/20"
                    >
                        Update Now
                    </button>
                </div>
            </div>
        </div>
    );
};
