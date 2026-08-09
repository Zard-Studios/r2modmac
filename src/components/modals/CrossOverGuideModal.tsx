import React, { useState } from 'react';

import { Button, Checkbox } from '../ui';

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
        <div
            className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/80 backdrop-blur-sm animate-in fade-in duration-200"
            onClick={handleClose}
        >
            <div
                className="bg-gray-900 border border-gray-700 rounded-xl overflow-hidden shadow-2xl max-w-2xl w-full flex flex-col max-h-[90vh]"
                onClick={(e) => e.stopPropagation()}
            >

                {/* Header */}
                <div className="flex items-center justify-between p-5 border-b border-gray-800">
                    <h2 className="text-xl font-bold text-white flex items-center gap-2">
                        <span className="text-2xl">🍷</span>
                        Wine / CrossOver Configuration Required
                    </h2>
                    <button
                        onClick={handleClose}
                        className="p-2 rounded-xl hover:bg-gray-800 text-gray-400 hover:text-white transition-all active:scale-95 focus:outline-none focus:ring-2 focus:ring-gray-700"
                        aria-label="Close"
                    >
                        <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M6 18L18 6M6 6l12 12" />
                        </svg>
                    </button>
                </div>

                {/* Content */}
                <div className="p-6 overflow-y-auto space-y-6">

                    <div className="flex gap-3 rounded-lg border border-blue-500/30 bg-blue-500/10 p-4">
                        <div className="text-fg-accent mt-1">
                            <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" viewBox="0 0 20 20" fill="currentColor">
                                <path fillRule="evenodd" d="M18 10a8 8 0 11-16 0 8 8 0 0116 0zm-7-4a1 1 0 11-2 0 1 1 0 012 0zM9 9a1 1 0 000 2v3a1 1 0 001 1h1a1 1 0 100-2v-3a1 1 0 00-1-1H9z" clipRule="evenodd" />
                            </svg>
                        </div>
                        <div className="text-sm text-fg-accent">
                            <p className="font-semibold mb-1">One-time Setup Required</p>
                            <p>To make mods work under Wine (CrossOver, Wineskin, Whisky, Porting Kit, etc.), you must configure a library override for <code className="rounded border border-blue-500/20 bg-blue-500/15 px-1.5 py-0.5 font-mono text-xs text-fg-accent">winhttp.dll</code>. This only needs to be done once per Wine prefix/bottle.</p>
                        </div>
                    </div>

                    <div className="space-y-4">
                        <h3 className="text-lg font-semibold text-white">Instructions:</h3>
                        <ol className="list-decimal list-inside space-y-3 text-gray-300 text-sm">
                            <li>Open your Wine wrapper (<strong>CrossOver, Wineskin, Whisky, Porting Kit</strong>, etc.)</li>
                            <li>Locate and open the <strong>"Wine Configuration"</strong> (winecfg) for your bottle/prefix</li>
                            <li>Go to the <strong>"Libraries"</strong> tab</li>
                            <li>In "New override for library", type or select <strong>winhttp</strong></li>
                            <li>Click <strong>"Add"</strong></li>
                            <li>Click <strong>"Apply"</strong> and then <strong>"OK"</strong></li>
                            <li className="mt-2 rounded border border-blue-500/30 bg-blue-500/10 px-2 py-0.5 font-bold text-fg-accent">Finally, launch the game!</li>
                        </ol>
                    </div>

                    <div className="rounded-xl overflow-hidden border border-gray-700 shadow-lg bg-black">
                        <img
                            src="https://i.ibb.co/hFnfqV1q/tut.gif"
                            alt="Wine Configuration Tutorial"
                            className="w-full h-auto"
                        />
                    </div>

                </div>

                {/* Footer */}
                <div className="flex items-center justify-between border-t border-gray-800 bg-gray-900 p-5">
                    <Checkbox checked={dontShowAgain} onChange={setDontShowAgain} label="Don't show again" />
                    <Button onClick={handleClose} size="lg">Done</Button>
                </div>

            </div>
        </div>
    );
};
