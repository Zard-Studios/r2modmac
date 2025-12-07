import { useState, useEffect } from 'react';

interface SettingsModalProps {
    isOpen: boolean;
    onClose: () => void;
    selectedGame?: string;
}

export function SettingsModal({ isOpen, onClose, selectedGame }: SettingsModalProps) {
    const [steamPath, setSteamPath] = useState<string>('');
    const [loading, setLoading] = useState(false);
    const [gamePath, setGamePath] = useState<string | null>(null);
    const [checkingGamePath, setCheckingGamePath] = useState(false);

    useEffect(() => {
        if (isOpen) {
            loadSettings();
            if (selectedGame) {
                checkGamePath();
            }
        }
    }, [isOpen, selectedGame]);

    const loadSettings = async () => {
        try {
            const settings = await window.ipcRenderer.getSettings();
            setSteamPath(settings.steam_path || '');
        } catch (e) {
            console.error("Failed to load settings", e);
        }
    };

    const checkGamePath = async () => {
        if (!selectedGame) return;

        setCheckingGamePath(true);
        try {
            const path = await window.ipcRenderer.getGamePath(selectedGame);
            setGamePath(path);
        } catch (e) {
            console.error("Failed to get game path", e);
            setGamePath(null);
        }
        setCheckingGamePath(false);
    };

    const handleSave = async () => {
        setLoading(true);
        try {
            const currentSettings = await window.ipcRenderer.getSettings();
            await window.ipcRenderer.saveSettings({
                ...currentSettings,
                steam_path: steamPath || null
            });
            // Re-check game path after saving
            if (selectedGame) {
                await checkGamePath();
            }
            onClose();
        } catch (e) {
            console.error("Failed to save settings", e);
            alert("Failed to save settings");
        }
        setLoading(false);
    };

    const handleBrowse = async () => {
        try {
            const path = await window.ipcRenderer.selectFolder();
            if (path) {
                setSteamPath(path);
            }
        } catch (e) {
            console.error("Failed to select folder", e);
        }
    };

    const handleOpenGameFolder = async () => {
        if (!selectedGame) return;

        try {
            await window.ipcRenderer.openGameFolder(selectedGame);
        } catch (e: any) {
            alert(e.message || "Failed to open game directory");
        }
    };

    const handleManualGamePath = async () => {
        if (!selectedGame) return;
        try {
            const path = await window.ipcRenderer.selectFolder();
            if (path) {
                await window.ipcRenderer.setGamePath(selectedGame, path);
                await checkGamePath(); // Refresh
            }
        } catch (e) {
            console.error("Failed to set manual game path", e);
        }
    };

    if (!isOpen) return null;

    return (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
            <div className="bg-gray-900 border border-gray-700 rounded-xl p-6 w-full max-w-md shadow-2xl">
                <h2 className="text-xl font-bold text-white mb-4">Settings</h2>

                <div className="mb-6">
                    <label className="block text-sm font-medium text-gray-400 mb-2">
                        Steam Directory
                    </label>
                    <div className="flex gap-2">
                        <input
                            type="text"
                            value={steamPath}
                            onChange={(e) => setSteamPath(e.target.value)}
                            className="flex-1 bg-gray-800 border border-gray-700 rounded-lg px-3 py-2 text-white text-sm focus:outline-none focus:border-blue-500"
                            placeholder="(BottleName)/drive_c/Program Files (x86)/Steam"
                        />
                        <button
                            onClick={handleBrowse}
                            className="bg-gray-700 hover:bg-gray-600 text-white px-3 py-2 rounded-lg text-sm transition-colors"
                        >
                            Browse
                        </button>
                    </div>
                    <p className="text-xs text-gray-500 mt-2">
                        Select the Steam folder inside your bottle (e.g., drive_c/Program Files (x86)/Steam).
                    </p>
                </div>

                {selectedGame && (
                    <div className="mb-6">
                        <label className="block text-sm font-medium text-gray-400 mb-2">
                            Game Directory
                        </label>

                        {checkingGamePath ? (
                            <div className="flex items-center gap-2 text-sm text-gray-400 py-2">
                                <svg className="animate-spin h-4 w-4" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
                                    <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4"></circle>
                                    <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                                </svg>
                                Checking...
                            </div>
                        ) : gamePath ? (
                            <div className="space-y-2">
                                <div className="flex gap-2">
                                    <button
                                        onClick={handleOpenGameFolder}
                                        className="flex-1 bg-gray-800 hover:bg-gray-700 border border-gray-700 text-white px-3 py-2 rounded-lg text-sm transition-colors flex items-center justify-center gap-2"
                                    >
                                        <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
                                        </svg>
                                        Open in Finder
                                    </button>
                                    <button
                                        onClick={handleManualGamePath}
                                        className="bg-gray-800 hover:bg-gray-700 border border-gray-700 text-gray-400 hover:text-white px-3 py-2 rounded-lg text-sm transition-colors"
                                        title="Change Game Location"
                                    >
                                        <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15.232 5.232l3.536 3.536m-2.036-5.036a2.5 2.5 0 113.536 3.536L6.5 21.036H3v-3.572L16.732 3.732z" />
                                        </svg>
                                    </button>
                                </div>
                                <div className="flex items-start gap-2 text-xs">
                                    <svg className="h-3.5 w-3.5 text-green-500 mt-0.5 flex-shrink-0" fill="currentColor" viewBox="0 0 20 20">
                                        <path fillRule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zm3.707-9.293a1 1 0 00-1.414-1.414L9 10.586 7.707 9.293a1 1 0 00-1.414 1.414l2 2a1 1 0 001.414 0l4-4z" clipRule="evenodd" />
                                    </svg>
                                    <span className="text-gray-400 break-all">{gamePath}</span>
                                </div>
                            </div>
                        ) : steamPath ? (
                            <div className="space-y-3">
                                <div className="flex items-start gap-2 text-xs text-yellow-400 bg-yellow-900/20 border border-yellow-500/30 rounded-lg p-3">
                                    <svg className="h-4 w-4 mt-0.5 flex-shrink-0" fill="currentColor" viewBox="0 0 20 20">
                                        <path fillRule="evenodd" d="M8.257 3.099c.765-1.36 2.722-1.36 3.486 0l5.58 9.92c.75 1.334-.213 2.98-1.742 2.98H4.42c-1.53 0-2.493-1.646-1.743-2.98l5.58-9.92zM11 13a1 1 0 11-2 0 1 1 0 012 0zm-1-8a1 1 0 00-1 1v3a1 1 0 002 0V6a1 1 0 00-1-1z" clipRule="evenodd" />
                                    </svg>
                                    <span>Game not detected inside Steam library folders.</span>
                                </div>
                                <button
                                    onClick={handleManualGamePath}
                                    className="w-full bg-gray-800 hover:bg-gray-700 border border-gray-700 text-white px-3 py-2 rounded-lg text-sm transition-colors flex items-center justify-center gap-2"
                                >
                                    <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
                                    </svg>
                                    Manually Locate Game Folder
                                </button>
                            </div>
                        ) : (
                            <div className="text-xs text-gray-500 bg-gray-800 border border-gray-700 rounded-lg p-3">
                                Configure Steam directory to detect game location
                            </div>
                        )}
                    </div>
                )}

                <div className="flex justify-end gap-3">
                    <button
                        onClick={onClose}
                        className="px-4 py-2 text-gray-400 hover:text-white text-sm transition-colors"
                    >
                        Cancel
                    </button>
                    <button
                        onClick={handleSave}
                        disabled={loading}
                        className="bg-blue-600 hover:bg-blue-500 text-white px-4 py-2 rounded-lg text-sm font-medium transition-colors disabled:opacity-50"
                    >
                        {loading ? 'Saving...' : 'Save'}
                    </button>
                </div>
            </div>
        </div>
    );
}
