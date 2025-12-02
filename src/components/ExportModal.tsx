
interface ExportModalProps {
    isOpen: boolean;
    onClose: () => void;
    onExportFile: () => void;
    onExportCode: () => void;
}

export function ExportModal({ isOpen, onClose, onExportFile, onExportCode }: ExportModalProps) {
    if (!isOpen) return null;

    return (
        <div className="fixed inset-0 bg-black/70 backdrop-blur-sm flex items-center justify-center z-50 p-4">
            <div className="bg-gray-800 rounded-xl p-6 max-w-md w-full border border-gray-700 shadow-2xl">
                <div className="flex justify-between items-center mb-6">
                    <h2 className="text-2xl font-bold text-white">Export Profile</h2>
                    <button onClick={onClose} className="text-gray-400 hover:text-white">
                        <svg xmlns="http://www.w3.org/2000/svg" className="h-6 w-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                        </svg>
                    </button>
                </div>

                <div className="space-y-4">
                    <button
                        onClick={() => {
                            onExportCode();
                            onClose();
                        }}
                        className="w-full bg-gray-800 border-2 border-dashed border-gray-600 rounded-xl p-6 flex flex-col items-center justify-center text-center hover:border-blue-500 hover:bg-gray-700/50 transition-all group"
                    >
                        <div className="w-12 h-12 bg-gray-700 rounded-full flex items-center justify-center mb-3 group-hover:bg-blue-500/20 transition-colors">
                            <span className="text-2xl group-hover:text-blue-400">🔗</span>
                        </div>
                        <h3 className="text-lg font-bold text-white mb-1">Share Code</h3>
                        <p className="text-sm text-gray-400">Generate a code to share with friends</p>
                    </button>

                    <div className="relative">
                        <div className="absolute inset-0 flex items-center">
                            <div className="w-full border-t border-gray-700"></div>
                        </div>
                        <div className="relative flex justify-center text-sm">
                            <span className="px-2 bg-gray-800 text-gray-500">OR</span>
                        </div>
                    </div>

                    <button
                        onClick={() => {
                            onExportFile();
                            onClose();
                        }}
                        className="w-full bg-gray-800 border-2 border-dashed border-gray-600 rounded-xl p-6 flex flex-col items-center justify-center text-center hover:border-purple-500 hover:bg-gray-700/50 transition-all group"
                    >
                        <div className="w-12 h-12 bg-gray-700 rounded-full flex items-center justify-center mb-3 group-hover:bg-purple-500/20 transition-colors">
                            <span className="text-2xl group-hover:text-purple-400">📄</span>
                        </div>
                        <h3 className="text-lg font-bold text-white mb-1">Export as File</h3>
                        <p className="text-sm text-gray-400">Save as .r2z file</p>
                    </button>
                </div>
            </div>
        </div>
    );
}
