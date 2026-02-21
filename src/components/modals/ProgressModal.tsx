interface ProgressModalProps {
    isOpen: boolean;
    title: string;
    progress: number; // 0 to 100
    currentTask: string;
}

export function ProgressModal({ isOpen, title, progress, currentTask }: ProgressModalProps) {
    if (!isOpen) return null;

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
