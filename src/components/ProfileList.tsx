
import { useState } from 'react';
import type { Profile } from '../types/profile';

interface ProfileListProps {
    profiles: Profile[];
    selectedGameIdentifier: string;
    onSelectProfile: (profileId: string) => void;
    onCreateProfile: (name: string) => void;
    onImportProfile: (code: string) => void;
    onImportFile: (path: string) => void;
    onBrowseMods: () => void;
    onDeleteProfile: (profileId: string, gameIdentifier?: string) => void;
    onUpdateProfile: (profileId: string, updates: Partial<Profile>) => void;
}

export function ProfileList({
    profiles,
    selectedGameIdentifier,
    onSelectProfile,
    onCreateProfile,
    onImportProfile,
    onImportFile,
    onBrowseMods,
    onDeleteProfile,
    onUpdateProfile
}: ProfileListProps) {
    const [isCreating, setIsCreating] = useState(false);
    const [isImporting, setIsImporting] = useState(false);
    const [editingProfile, setEditingProfile] = useState<Profile | null>(null);
    const [newProfileName, setNewProfileName] = useState('');
    const [editName, setEditName] = useState('');
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

    const handleUpdateProfile = (e: React.FormEvent) => {
        e.preventDefault();
        if (editingProfile && editName.trim()) {
            onUpdateProfile(editingProfile.id, { name: editName.trim() });
            setEditingProfile(null);
            setEditName('');
        }
    };

    const handleImageSelect = async () => {
        if (!editingProfile) return;
        try {
            const filePath = await window.ipcRenderer.selectFile([
                { name: 'Images', extensions: ['png', 'jpg', 'jpeg', 'webp'] }
            ]);
            if (filePath) {
                // Convert file path to a format usable by the renderer (e.g. file:// protocol or base64)
                // For now, let's assume the renderer can handle the path or we need to read it.
                // Actually, browsers can't just read local paths. We might need the backend to serve it or read it as base64.
                // Let's ask the backend to read it as base64.
                // Assuming we have a method for this or we can add one.
                // Wait, `window.ipcRenderer.selectFile` returns the path.
                // We should probably store the path and let the main process handle serving/reading.
                // But for `img src`, we need a proper URL.
                // Let's try using the `convertFileSrc` from Tauri if available, or just the path if Electron handles it.
                // Since this is "r2modmac", it might be Electron or Tauri. The user mentioned Tauri earlier.
                // If Tauri, we need `convertFileSrc`.
                // Let's assume we can just pass the path for now and see if it works (Electron often allows it with proper security settings).
                // If not, we might need a `readImageAsBase64` IPC call.

                // Let's try to read it as base64 via IPC for safety and compatibility.
                const base64 = await window.ipcRenderer.readImage(filePath);
                if (base64) {
                    // Update global store
                    onUpdateProfile(editingProfile.id, { profileImageUrl: base64 });
                    // Update local state to show preview immediately
                    setEditingProfile(prev => prev ? { ...prev, profileImageUrl: base64 } : null);
                }
            }
        } catch (e) {
            console.error("Failed to select image:", e);
        }
    };

    const handleRemoveImage = () => {
        if (editingProfile) {
            onUpdateProfile(editingProfile.id, { profileImageUrl: undefined });
            setEditingProfile(prev => prev ? { ...prev, profileImageUrl: undefined } : null);
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
                            <svg xmlns="http://www.w3.org/2000/svg" className="h-8 w-8 text-gray-500 group-hover:text-blue-400 transition-colors" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
                            </svg>
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
                            <svg xmlns="http://www.w3.org/2000/svg" className="h-8 w-8 text-gray-500 group-hover:text-purple-400 transition-colors" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 14l-7 7m0 0l-7-7m7 7V3" />
                            </svg>
                        </div>
                        <h3 className="text-lg font-bold text-white mb-1">Import Profile</h3>
                        <p className="text-sm text-gray-500">Use a code or file</p>
                    </div>

                    {/* Browse Only Card */}
                    <div
                        className="bg-gray-800/50 border-2 border-dashed border-gray-700 rounded-xl p-6 flex flex-col items-center justify-center text-center hover:border-green-500/50 hover:bg-gray-800 transition-all cursor-pointer min-h-[200px] group"
                        onClick={onBrowseMods}
                    >
                        <div className="w-16 h-16 bg-gray-900 rounded-full flex items-center justify-center mb-4 group-hover:bg-green-500/20 transition-colors">
                            <svg xmlns="http://www.w3.org/2000/svg" className="h-8 w-8 text-gray-500 group-hover:text-green-400 transition-colors" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
                            </svg>
                        </div>
                        <h3 className="text-lg font-bold text-white mb-1">Browse Mods</h3>
                        <p className="text-sm text-gray-500">Explore without a profile</p>
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
                                    onClick={(e) => {
                                        e.stopPropagation();
                                        setEditingProfile(profile);
                                        setEditName(profile.name);
                                    }}
                                    className="w-8 h-8 bg-gray-700 hover:bg-gray-600 text-gray-300 hover:text-white rounded-full flex items-center justify-center transition-colors"
                                    title="Edit Profile"
                                >
                                    <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15.232 5.232l3.536 3.536m-2.036-5.036a2.5 2.5 0 113.536 3.536L6.5 21.036H3v-3.572L16.732 3.732z" />
                                    </svg>
                                </button>
                                <button
                                    onClick={async (e) => {
                                        e.stopPropagation();
                                        const confirmed = await window.ipcRenderer.confirm('Delete Profile', 'Are you sure you want to delete this profile?');
                                        if (confirmed) {
                                            await onDeleteProfile(profile.id, selectedGameIdentifier);
                                        }
                                    }}
                                    className="w-8 h-8 bg-red-500/20 hover:bg-red-500 text-red-500 hover:text-white rounded-full flex items-center justify-center transition-colors"
                                    title="Delete Profile"
                                >
                                    <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                                    </svg>
                                </button>

                            </div>

                            {profile.profileImageUrl ? (
                                <img
                                    src={profile.profileImageUrl}
                                    alt={profile.name}
                                    className="w-12 h-12 rounded-lg mb-4 flex-shrink-0 object-cover bg-gray-800"
                                />
                            ) : (
                                <div className="w-12 h-12 bg-gradient-to-br from-blue-500 to-purple-600 rounded-lg mb-4 flex-shrink-0 flex items-center justify-center text-xl font-bold text-white">
                                    {profile.name.charAt(0).toUpperCase()}
                                </div>
                            )}

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
                                            const filePath = await window.ipcRenderer.selectFile([
                                                { name: 'r2modman Profile', extensions: ['r2z', 'zip'] }
                                            ]);
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
                                    <svg xmlns="http://www.w3.org/2000/svg" className="h-8 w-8 mb-1" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
                                    </svg>
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

            {/* Edit Profile Modal */}
            {editingProfile && (
                <div className="fixed inset-0 bg-black/70 backdrop-blur-sm flex items-center justify-center z-50 p-4">
                    <div className="bg-gray-800 rounded-xl p-6 max-w-md w-full border border-gray-700 shadow-2xl">
                        <h2 className="text-2xl font-bold text-white mb-4">Edit Profile</h2>

                        <div className="flex justify-center mb-6">
                            <div className="relative group cursor-pointer" onClick={handleImageSelect}>
                                {editingProfile.profileImageUrl ? (
                                    <img
                                        src={editingProfile.profileImageUrl}
                                        alt="Profile"
                                        className="w-24 h-24 rounded-full object-cover border-4 border-gray-700 group-hover:border-blue-500 transition-colors"
                                    />
                                ) : (
                                    <div className="w-24 h-24 rounded-full bg-gradient-to-br from-blue-500 to-purple-600 flex items-center justify-center text-4xl font-bold text-white border-4 border-gray-700 group-hover:border-blue-500 transition-colors">
                                        {editName.charAt(0).toUpperCase()}
                                    </div>
                                )}
                                <div className="absolute inset-0 bg-black/50 rounded-full flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity">
                                    <span className="text-white font-medium text-xs">Change</span>
                                </div>
                            </div>
                        </div>

                        {editingProfile.profileImageUrl && (
                            <div className="text-center mb-4">
                                <button
                                    type="button"
                                    onClick={handleRemoveImage}
                                    className="text-xs text-red-400 hover:text-red-300 hover:underline"
                                >
                                    Remove Custom Image
                                </button>
                            </div>
                        )}

                        <form onSubmit={handleUpdateProfile}>
                            <input
                                type="text"
                                value={editName}
                                onChange={(e) => setEditName(e.target.value)}
                                placeholder="Profile Name"
                                className="w-full bg-gray-900 border border-gray-700 rounded-lg px-4 py-3 text-white placeholder-gray-500 focus:outline-none focus:border-blue-500 mb-6"
                                autoFocus
                            />
                            <div className="flex gap-3">
                                <button
                                    type="button"
                                    onClick={() => setEditingProfile(null)}
                                    className="flex-1 px-4 py-2 rounded-lg bg-gray-700 text-white hover:bg-gray-600 font-semibold"
                                >
                                    Cancel
                                </button>
                                <button
                                    type="submit"
                                    disabled={!editName.trim()}
                                    className="flex-1 px-4 py-2 rounded-lg bg-blue-600 text-white hover:bg-blue-500 font-semibold disabled:opacity-50 disabled:cursor-not-allowed"
                                >
                                    Save
                                </button>
                            </div>
                        </form>
                    </div>
                </div>
            )}
        </div>
    );
}
