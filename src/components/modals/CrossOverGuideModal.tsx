import React, { useState } from 'react';

interface CrossOverGuideModalProps {
    isOpen: boolean;
    onClose: () => void;
    onDontShowAgain?: (dontShow: boolean) => void;
}

export const CrossOverGuideModal: React.FC<CrossOverGuideModalProps> = ({ isOpen, onClose, onDontShowAgain }) => {
    const [dontShowAgain, setDontShowAgain] = useState(false);

    if (!isOpen) return null;

    const handleClose = () => {
        if (dontShowAgain && onDontShowAgain) {
            onDontShowAgain(true);
        }
        onClose();
    };

    return (
        <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/80 backdrop-blur-sm animate-in fade-in duration-200">
            <div className="bg-gray-900 border border-gray-700 rounded-xl shadow-2xl max-w-2xl w-full flex flex-col max-h-[90vh]">

                {/* Header */}
                <div className="flex items-center justify-between p-5 border-b border-gray-800">
                    <h2 className="text-xl font-bold text-white flex items-center gap-2">
                        <span className="text-2xl">🍷</span>
                        CrossOver Configuration Required
                    </h2>
                    <button
                        onClick={handleClose}
                        className="text-gray-400 hover:text-white transition-colors p-1 rounded-lg hover:bg-gray-800"
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" className="h-6 w-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                        </svg>
                    </button>
                </div>

                {/* Content */}
                <div className="p-6 overflow-y-auto space-y-6">

                    <div className="bg-blue-900/20 border border-blue-500/30 rounded-lg p-4 flex gap-3">
                        <div className="text-blue-400 mt-1">
                            <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" viewBox="0 0 20 20" fill="currentColor">
                                <path fillRule="evenodd" d="M18 10a8 8 0 11-16 0 8 8 0 0116 0zm-7-4a1 1 0 11-2 0 1 1 0 012 0zM9 9a1 1 0 000 2v3a1 1 0 001 1h1a1 1 0 100-2v-3a1 1 0 00-1-1H9z" clipRule="evenodd" />
                            </svg>
                        </div>
                        <div className="text-sm text-blue-200">
                            <p className="font-semibold mb-1">One-time Setup Required</p>
                            <p>To make mods work with CrossOver, you must configure a library override for <code className="bg-blue-900/50 px-1.5 py-0.5 rounded text-blue-100 font-mono text-xs">winhttp.dll</code>. This only needs to be done once per bottle.</p>
                        </div>
                    </div>

                    <div className="space-y-4">
                        <h3 className="text-lg font-semibold text-white">Instructions:</h3>
                        <ol className="list-decimal list-inside space-y-3 text-gray-300 text-sm">
                            <li>Open <strong>CrossOver</strong> and select your bottle (e.g., "Steam")</li>
                            <li>In the right sidebar, click on <strong>"Wine Configuration"</strong></li>
                            <li>Go to the <strong>"Libraries"</strong> tab</li>
                            <li>In "New override for library", type or select <strong>winhttp</strong></li>
                            <li>Click <strong>"Add"</strong></li>
                            <li>Click <strong>"Apply"</strong> and then "OK"</li>
                            <li className="text-white font-bold bg-blue-900/30 px-2 py-0.5 rounded mt-2 border border-blue-500/30">Finally, launch the game via Steam!</li>
                        </ol>
                    </div>

                    <div className="rounded-xl overflow-hidden border border-gray-700 shadow-lg bg-black">
                        <img
                            src="https://i.ibb.co/hFnfqV1q/tut.gif"
                            alt="CrossOver Configuration Tutorial"
                            className="w-full h-auto"
                        />
                    </div>

                </div>

                {/* Footer */}
                <div className="p-5 border-t border-gray-800 bg-gray-900/50 flex items-center justify-between">
                    <label className="flex items-center gap-2 text-sm text-gray-400 hover:text-gray-300 cursor-pointer">
                        <input
                            type="checkbox"
                            checked={dontShowAgain}
                            onChange={(e) => setDontShowAgain(e.target.checked)}
                            className="w-4 h-4 rounded border-gray-600 bg-gray-800 text-blue-600 focus:ring-2 focus:ring-blue-500 focus:ring-offset-0"
                        />
                        <span>Don't show again</span>
                    </label>
                    <button
                        onClick={handleClose}
                        className="px-5 py-2.5 bg-blue-600 hover:bg-blue-500 text-white rounded-lg font-medium transition-colors shadow-lg shadow-blue-900/20"
                    >
                        Done
                    </button>
                </div>

            </div>
        </div>
    );
};
