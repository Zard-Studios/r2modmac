import React, { useState } from 'react';

interface MacOSGuideModalProps {
    isOpen: boolean;
    onClose: () => void;
    onDontShowAgain?: (dontShow: boolean) => void;
}

export const MacOSGuideModal: React.FC<MacOSGuideModalProps> = ({ isOpen, onClose, onDontShowAgain }) => {
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
                    <h2 className="text-xl font-bold text-white flex items-center gap-2.5">
                        <span className="flex items-center justify-center w-5 h-6 text-white pb-1 border-white/0">
                            <svg xmlns="http://www.w3.org/2000/svg" className="w-[18px] h-[21px]" viewBox="0 0 384 512" fill="currentColor">
                                <path d="M318.7 268.7c-.2-36.7 16.4-64.4 50-84.8-18.8-26.9-47.2-41.7-84.7-44.6-35.5-2.8-74.3 20.7-88.5 20.7-15 0-49.4-19.7-76.4-19.7C63.3 141.2 4 184.8 4 273.5q0 39.3 14.4 81.2c12.8 36.7 59 126.7 107.2 125.2 25.2-.6 43-17.9 75.8-17.9 31.8 0 48.3 17.9 76.4 17.9 48.6-.7 90.4-82.5 102.6-119.3-65.2-30.7-61.7-90-61.7-91.9zm-56.6-164.2c27.3-32.4 24.8-61.9 24-72.5-24.1 1.4-52 16.4-67.9 34.9-17.5 19.8-27.8 44.3-25.6 71.9 26.1 2 49.9-11.4 69.5-34.3z" />
                            </svg>
                        </span>
                        macOS – Steam Launch Option Required
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
                            <p>To load mods on macOS, you must set a Steam launch option that injects BepInEx via <code className="bg-blue-900/50 px-1.5 py-0.5 rounded text-blue-100 font-mono text-xs">run_bepinex.sh</code> under Rosetta. This only needs to be done once per game.</p>
                        </div>
                    </div>

                    <div className="space-y-4">
                        <h3 className="text-lg font-semibold text-white">Instructions:</h3>
                        <ol className="list-decimal list-inside space-y-3 text-gray-300 text-sm">
                            <li>Open <strong>Steam</strong> and find the game in your library</li>
                            <li>Right-click the game → <strong>"Properties"</strong></li>
                            <li>Go to the <strong>"General"</strong> tab</li>
                            <li>In the <strong>"Launch Options"</strong> field, paste exactly:
                                <div className="mt-2 bg-gray-800 border border-gray-700 rounded-lg px-3 py-2 font-mono text-xs text-green-400 select-all">
                                    /usr/bin/arch -x86_64 /bin/sh "run_bepinex.sh" %command%
                                </div>
                            </li>
                            <li>Click <strong>OK</strong> to save</li>
                            <li className="text-white font-bold bg-blue-900/30 px-2 py-0.5 rounded mt-2 border border-blue-500/30">Finally, launch the game via Steam!</li>
                        </ol>
                        <p className="text-xs text-yellow-500/80 mt-3">
                            ⚠️ <strong>Note:</strong> Even on Apple Silicon (M1/M2/M3), Rosetta (<code className="font-mono text-xs">arch -x86_64</code>) is required because BepInEx's doorstop library is x86_64 only. The game itself may run natively, but BepInEx cannot.
                        </p>
                    </div>

                    <div className="rounded-xl overflow-hidden border border-gray-700 shadow-lg bg-black">
                        <img
                            src="https://i.ibb.co/k2yM1VXZ/tut2.gif"
                            alt="macOS Steam Launch Option Tutorial"
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
