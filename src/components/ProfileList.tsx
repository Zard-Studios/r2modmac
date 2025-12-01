
import { useState } from 'react';
import type { Profile } from '../types/profile';

interface ProfileListProps {
    profiles: Profile[];
    selectedGameIdentifier: string;
    onSelectProfile: (profileId: string) => void;
    onCreateProfile: (name: string) => void;
    onImportProfile: (code: string) => void;
    onImportFile: (path: string) => void;
    onDeleteProfile: (profileId: string) => void;
}

export function ProfileList({
    profiles,
    selectedGameIdentifier,
    onSelectProfile,
    onCreateProfile,
    onImportProfile,
    onImportFile,
    onDeleteProfile
}: ProfileListProps) {
    const [isCreating, setIsCreating] = useState(false);
    const [isImporting, setIsImporting] = useState(false);
    const [newProfileName, setNewProfileName] = useState('');
    const [importCode, setImportCode] = useState('');

    const filteredProfiles = profiles.filter(p => p.gameIdentifier === selectedGameIdentifier);

    const handleCreate = (e: React.FormEvent) => {
        e.preventDefault();
        if (newProfileName.trim()) {
            onCreateProfile(newProfileName.trim());
            setNewProfileName('');
            setIsCreating(false);
        }
    };

    const handleImport = (e: React.FormEvent) => {
        e.preventDefault();
        if (importCode.trim()) {
            onImportProfile(importCode.trim());
            setImportCode('');
            setIsImporting(false);
        }
    };

    return (
        <div className="flex-1 p-8 overflow-y-auto">
            <div className="max-w-4xl mx-auto">
                <div className="flex items-center justify-between mb-8">
                    <div>
                        <h1 className="text-3xl font-bold text-white mb-2">Select Profile</h1>
                        <p className="text-gray-400">Choose a profile to manage mods for {selectedGameIdentifier}</p>
                    </div>
                </div>

                <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
                    {/* Create New Profile Card */}
                    <div
                        className="bg-gray-800/50 border-2 border-dashed border-gray-700 rounded-xl p-6 flex flex-col items-center justify-center text-center hover:border-blue-500/50 hover:bg-gray-800 transition-all cursor-pointer min-h-[200px] group"
                        onClick={() => setIsCreating(true)}
                    >
                        <div className="w-16 h-16 bg-gray-900 rounded-full flex items-center justify-center mb-4 group-hover:bg-blue-500/20 transition-colors">
                            <span className="text-3xl text-gray-500 group-hover:text-blue-400">+</span>
                        </div>
                        <h3 className="text-lg font-bold text-white mb-1">Create New</h3>
                        <p className="text-sm text-gray-500">Start fresh with a new profile</p>
                    </div>

                    {/* Import Profile Card */}
                    <div
                        className="bg-gray-800/50 border-2 border-dashed border-gray-700 rounded-xl p-6 flex flex-col items-center justify-center text-center hover:border-purple-500/50 hover:bg-gray-800 transition-all cursor-pointer min-h-[200px] group"
                        onClick={() => setIsImporting(true)}
                    >
                        <div className="w-16 h-16 bg-gray-900 rounded-full flex items-center justify-center mb-4 group-hover:bg-purple-500/20 transition-colors">
                            <span className="text-3xl text-gray-500 group-hover:text-purple-400">↓</span>
                        </div>
                        <h3 className="text-lg font-bold text-white mb-1">Import Profile</h3>
                        <p className="text-sm text-gray-500">Use a code or file</p>
                    </div>

                    {/* Existing Profiles */}
                    {filteredProfiles.map(profile => (
                        <div
                            key={profile.id}
                            onClick={() => onSelectProfile(profile.id)}
                            className="bg-gray-800 border border-gray-700 rounded-xl p-6 hover:border-blue-500 transition-all cursor-pointer flex flex-col min-h-[200px] group relative overflow-hidden"
                        >
                            <div className="absolute top-0 right-0 p-4 flex gap-2 opacity-0 group-hover:opacity-100 transition-opacity">
                                <button
                                    onClick={async (e) => {
                                        e.stopPropagation();
                                        const confirmed = await window.ipcRenderer.confirm('Delete Profile', 'Are you sure you want to delete this profile?');
                                        if (confirmed) {
                                            await onDeleteProfile(profile.id);
                                        }
                                    }}
                                    className="w-8 h-8 bg-red-500/20 hover:bg-red-500 text-red-500 hover:text-white rounded-full flex items-center justify-center transition-colors"
                                    title="Delete Profile"
                                >
                                    <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                                    </svg>
                                </button>
                                <div className="w-8 h-8 bg-blue-500 rounded-full flex items-center justify-center text-white">
                                    <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M14 5l7 7m0 0l-7 7m7-7H3" />
                                    </svg>
                                </div>
                            </div>

                            <div className="w-12 h-12 bg-gradient-to-br from-blue-500 to-purple-600 rounded-lg mb-4 flex-shrink-0" />

                            <h3 className="text-xl font-bold text-white mb-2 truncate">{profile.name}</h3>

                            <div className="mt-auto space-y-2">
                                <div className="flex items-center text-sm text-gray-400 gap-2">
                                    <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4" />
                                    </svg>
                                    <span>{profile.mods.length} mods installed</span>
                                </div>
                                <div className="flex items-center text-sm text-gray-400 gap-2">
                                    <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
                                    </svg>
                                    <span>Last played {new Date(profile.lastUsed).toLocaleDateString()}</span>
                                </div>
                            </div>
                        </div>
                    ))}
                </div>
            </div>

            {/* Create Profile Modal */}
            {isCreating && (
                <div className="fixed inset-0 bg-black/70 backdrop-blur-sm flex items-center justify-center z-50 p-4">
                    <div className="bg-gray-800 rounded-xl p-6 max-w-md w-full border border-gray-700 shadow-2xl">
                        <h2 className="text-2xl font-bold text-white mb-4">Create New Profile</h2>
                        <form onSubmit={handleCreate}>
                            <input
                                type="text"
                                value={newProfileName}
                                onChange={(e) => setNewProfileName(e.target.value)}
                                placeholder="Profile Name (e.g. My Modpack)"
                                className="w-full bg-gray-900 border border-gray-700 rounded-lg px-4 py-3 text-white placeholder-gray-500 focus:outline-none focus:border-blue-500 mb-6"
                                autoFocus
                            />
                            <div className="flex gap-3">
                                <button
                                    type="button"
                                    onClick={() => setIsCreating(false)}
                                    className="flex-1 px-4 py-2 rounded-lg bg-gray-700 text-white hover:bg-gray-600 font-semibold"
                                >
                                    Cancel
                                </button>
                                <button
                                    type="submit"
                                    disabled={!newProfileName.trim()}
                                    className="flex-1 px-4 py-2 rounded-lg bg-blue-600 text-white hover:bg-blue-500 font-semibold disabled:opacity-50 disabled:cursor-not-allowed"
                                >
                                    Create
                                </button>
                            </div>
                        </form>
                    </div>
                </div>
            )}

            {/* Import Profile Modal */}
            {isImporting && (
                <div className="fixed inset-0 bg-black/70 backdrop-blur-sm flex items-center justify-center z-50 p-4">
                    <div className="bg-gray-800 rounded-xl p-6 max-w-md w-full border border-gray-700 shadow-2xl">
                        <h2 className="text-2xl font-bold text-white mb-4">Import Profile</h2>

                        <div className="space-y-4">
                            <div>
                                <label className="block text-sm text-gray-400 mb-2">Option 1: Import from Code</label>
                                <form onSubmit={handleImport} className="flex gap-2">
                                    <input
                                        type="text"
                                        value={importCode}
                                        onChange={(e) => setImportCode(e.target.value)}
                                        placeholder="e.g. 019ad1ed-..."
                                        className="flex-1 bg-gray-900 border border-gray-700 rounded-lg px-4 py-2 text-white placeholder-gray-500 focus:outline-none focus:border-purple-500 font-mono text-sm"
                                        autoFocus
                                    />
                                    <button
                                        type="submit"
                                        disabled={!importCode.trim()}
                                        className="px-4 py-2 rounded-lg bg-purple-600 text-white hover:bg-purple-500 font-semibold disabled:opacity-50 disabled:cursor-not-allowed text-sm"
                                    >
                                        Import
                                    </button>
                                </form>
                            </div>

                            <div className="relative">
                                <div className="absolute inset-0 flex items-center">
                                    <div className="w-full border-t border-gray-700"></div>
                                </div>
                                <div className="relative flex justify-center text-sm">
                                    <span className="px-2 bg-gray-800 text-gray-500">OR</span>
                                </div>
                            </div>

                            <div>
                                <label className="block text-sm text-gray-400 mb-2">Option 2: Import from File</label>
                                <button
                                    onClick={async () => {
                                        try {
                                            const filePath = await window.ipcRenderer.selectFile();
                                            if (filePath) {
                                                // We need to pass this up to App.tsx
                                                // For now, let's reuse onImportProfile but pass a special prefix or handle it in App.tsx
                                                // Better: Add onImportFile prop.
                                                // Since I can't easily change the interface in this step without breaking App.tsx, 
                                                // I'll emit a custom event or use a hack, BUT I should just update the interface.
                                                // Let's assume I will update App.tsx to pass this prop.
                                                // Actually, I can just call the prop if I add it.
                                                // Let's modify the component to accept onImportFile.
                                                if (onImportFile) {
                                                    onImportFile(filePath);
                                                    setIsImporting(false);
                                                }
                                            }
                                        } catch (e) {
                                            console.error(e);
                                            alert("Failed to select file");
                                        }
                                    }}
                                    className="w-full py-3 border-2 border-dashed border-gray-600 rounded-lg text-gray-400 hover:border-purple-500 hover:text-purple-400 transition-colors flex flex-col items-center justify-center gap-1"
                                >
                                    <span className="text-2xl">📄</span>
                                    <span className="text-sm">Select .r2z or .zip file</span>
                                </button>
                            </div>
                        </div>

                        <div className="mt-6 flex justify-end">
                            <button
                                type="button"
                                onClick={() => setIsImporting(false)}
                                className="px-4 py-2 rounded-lg text-gray-400 hover:text-white hover:bg-gray-700 font-semibold text-sm"
                            >
                                Cancel
                            </button>
                        </div>
                    </div>
                </div>
            )}
        </div>
    );
}
