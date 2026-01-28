import { useState, useEffect } from 'react';
import { Button } from './ui';

interface PreferencesModalProps {
    isOpen: boolean;
    onClose: () => void;
    settings: {
        legacy_install_mode: boolean;
    };
    onSave: (settings: { legacy_install_mode: boolean }) => void;
}

export default function PreferencesModal({ isOpen, onClose, settings, onSave }: PreferencesModalProps) {
    const [legacyMode, setLegacyMode] = useState(settings.legacy_install_mode);

    useEffect(() => {
        setLegacyMode(settings.legacy_install_mode);
    }, [settings]);

    if (!isOpen) return null;

    const handleSave = () => {
        onSave({ legacy_install_mode: legacyMode });
        onClose();
    };

    return (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
            <div className="bg-gray-900 border border-gray-700 rounded-2xl shadow-2xl w-full max-w-lg">
                {/* Header */}
                <div className="flex items-center justify-between p-5 border-b border-gray-800">
                    <h2 className="text-xl font-bold text-white">Preferences</h2>
                    <button
                        onClick={onClose}
                        className="p-1.5 rounded-lg hover:bg-gray-800 text-gray-400 hover:text-white transition-colors"
                    >
                        <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                        </svg>
                    </button>
                </div>

                {/* Content */}
                <div className="p-5 space-y-6">
                    {/* Legacy Mode Toggle */}
                    <div className="flex items-start justify-between gap-4">
                        <div className="flex-1">
                            <h3 className="text-white font-medium mb-1">Legacy Install Mode</h3>
                            <p className="text-gray-400 text-sm">
                                When enabled, mods are downloaded when you click Install (old behavior).
                                <span className="text-yellow-400"> Warning: This uses more storage space.</span>
                            </p>
                        </div>
                        <button
                            onClick={() => setLegacyMode(!legacyMode)}
                            className={`relative w-12 h-6 rounded-full transition-colors flex-shrink-0 ${legacyMode ? 'bg-blue-600' : 'bg-gray-700'
                                }`}
                        >
                            <span
                                className={`absolute top-0.5 left-0.5 w-5 h-5 bg-white rounded-full shadow transition-transform ${legacyMode ? 'translate-x-6' : ''
                                    }`}
                            />
                        </button>
                    </div>

                    {/* Info Box */}
                    <div className={`p-4 rounded-lg border ${legacyMode ? 'bg-yellow-900/20 border-yellow-800' : 'bg-blue-900/20 border-blue-800'}`}>
                        <div className="flex gap-3">
                            <svg className={`w-5 h-5 flex-shrink-0 ${legacyMode ? 'text-yellow-400' : 'text-blue-400'}`} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                            </svg>
                            <div className="text-sm">
                                {legacyMode ? (
                                    <span className="text-yellow-200">
                                        <strong>Legacy Mode:</strong> Mods download immediately to a local cache.
                                        Faster install, but uses more disk space.
                                    </span>
                                ) : (
                                    <span className="text-blue-200">
                                        <strong>New Mode:</strong> Install only adds mods to your profile list.
                                        Click "Apply to Game" to download and sync.
                                    </span>
                                )}
                            </div>
                        </div>
                    </div>

                    {/* Clear Cache Section */}
                    <div className="p-4 rounded-lg bg-red-900/20 border border-red-800">
                        <div className="flex items-center justify-between gap-4">
                            <div className="flex-1">
                                <h4 className="text-red-200 font-medium text-sm mb-1">Clear Profile Cache</h4>
                                <p className="text-red-300/70 text-xs">
                                    Remove all downloaded mods from local cache to free up disk space.
                                </p>
                            </div>
                            <button
                                onClick={async () => {
                                    const confirmed = await window.ipcRenderer.confirm(
                                        'Clear Profile Cache?',
                                        'This will delete all cached mods. You will need to re-download them when applying to game. Continue?'
                                    );
                                    if (confirmed) {
                                        const result = await window.ipcRenderer.clearProfileCache();
                                        const sizeMB = (result.bytes_freed / 1024 / 1024).toFixed(1);
                                        await window.ipcRenderer.alert(
                                            'Cache Cleared',
                                            `Cleared ${result.cleared} profile(s), freed ${sizeMB} MB.`
                                        );
                                    }
                                }}
                                className="px-3 py-1.5 rounded-lg bg-red-600/20 hover:bg-red-600/40 text-red-400 hover:text-red-300 text-sm font-medium border border-red-700 transition-colors flex-shrink-0"
                            >
                                Clear Cache
                            </button>
                        </div>
                    </div>
                </div>

                {/* Footer */}
                <div className="flex justify-end gap-3 p-5 border-t border-gray-800">
                    <Button variant="secondary" onClick={onClose}>
                        Cancel
                    </Button>
                    <Button variant="primary" onClick={handleSave}>
                        Save
                    </Button>
                </div>
            </div>
        </div>
    );
}
