import { useEffect, useRef, useState } from 'react';
import { Menu, MenuItem } from '@tauri-apps/api/menu';
import type { Profile } from '../../types/profile';
import type { CommunityPlatformInfo } from '../../types/thunderstore';
import { Button, HoverMarquee } from '../ui';
import { Toggle } from '../ui/Toggle';
import { PlatformPicker } from './PlatformPicker';
import { revealInFileManagerLabel } from '../../utils/platformUtils';
import { getProfileAvatarGradient, getProfileInitial } from '../../utils/profileAvatar';
import { SponsorSurface } from '../sponsors/SponsorSurface';
import { KeyboardShortcuts } from '../KeyboardShortcuts';

interface ProfileListProps {
    profiles: Profile[];
    selectedGameIdentifier: string;
    selectedGameName?: string;
    selectedGamePlatform?: CommunityPlatformInfo;
    isBusy?: boolean;
    onSelectProfile: (profileId: string) => void;
    onCreateProfile: (name: string, platform?: 'windows' | 'mac') => void;
    onImportProfile: (code: string, platform: 'windows' | 'mac') => void;
    onImportFile: (path: string, platform: 'windows' | 'mac') => void;
    onBrowseMods: () => void;
    /**
     * Opens the search narrowed to profiles, for the magnifier on this page.
     * Deliberately not the same thing: a control sitting among the profile
     * buttons has to mean profiles.
     */
    onFindProfile: () => void;
    onDeleteProfile: (profileId: string, gameIdentifier?: string) => void;
    onUpdateProfile: (profileId: string, updates: Partial<Profile>) => void;
    onToggleVanilla: (profileId: string, newVanillaState: boolean) => void;
    /** Selects and starts this profile through the app's normal launch guard. */
    onLaunchProfile: (profileId: string) => void;
}

export function ProfileList({
    profiles,
    selectedGameIdentifier,
    selectedGameName,
    selectedGamePlatform,
    isBusy = false,
    onSelectProfile,
    onCreateProfile,
    onImportProfile,
    onImportFile,
    onBrowseMods,
    onFindProfile,
    onDeleteProfile,
    onUpdateProfile,
    onToggleVanilla,
    onLaunchProfile,
}: ProfileListProps) {
    const [isCreating, setIsCreating] = useState(false);
    const [isImporting, setIsImporting] = useState(false);
    const [editingProfile, setEditingProfile] = useState<Profile | null>(null);
    const [newProfileName, setNewProfileName] = useState('');
    const [editName, setEditName] = useState('');
    const [importCode, setImportCode] = useState('');
    const [selectedPlatform, setSelectedPlatform] = useState<'windows' | 'mac'>('windows');
    const [openProfileMenuId, setOpenProfileMenuId] = useState<string | null>(null);
    const profileMenuRef = useRef<HTMLDivElement>(null);
    // null = no pending import; string = code; { file: string } = file
    const [pendingImport, setPendingImport] = useState<string | { file: string } | null>(null);

    useEffect(() => {
        const openRequestedAction = (event: Event) => {
            const action = (event as CustomEvent<'new' | 'import'>).detail;
            if (action !== 'new' && action !== 'import') return;

            setEditingProfile(null);
            setPendingImport(null);
            setSelectedPlatform('windows');
            if (action === 'import') {
                setIsCreating(false);
                setImportCode('');
                setIsImporting(true);
            } else {
                setIsImporting(false);
                setNewProfileName('');
                setIsCreating(true);
            }
        };

        window.addEventListener('r2modmac:open-profile-action', openRequestedAction);
        return () => window.removeEventListener('r2modmac:open-profile-action', openRequestedAction);
    }, []);

    useEffect(() => {
        const onKeyDown = (event: KeyboardEvent) => {
            if (event.key !== 'Escape' || event.metaKey || event.ctrlKey || event.altKey) {
                return;
            }

            if (openProfileMenuId) {
                event.preventDefault();
                event.stopPropagation();
                setOpenProfileMenuId(null);
                return;
            }

            if (editingProfile) {
                event.preventDefault();
                event.stopPropagation();
                setEditingProfile(null);
                setEditName('');
                return;
            }

            if (isImporting || pendingImport) {
                event.preventDefault();
                event.stopPropagation();
                setIsImporting(false);
                setPendingImport(null);
                setImportCode('');
                return;
            }

            if (isCreating) {
                event.preventDefault();
                event.stopPropagation();
                setIsCreating(false);
                setNewProfileName('');
                setSelectedPlatform('windows');
            }
        };

        window.addEventListener('keydown', onKeyDown, true);
        return () => window.removeEventListener('keydown', onKeyDown, true);
    }, [editingProfile, isCreating, isImporting, openProfileMenuId, pendingImport]);

    useEffect(() => {
        if (!openProfileMenuId) return;
        const closeOutside = (event: MouseEvent) => {
            if (profileMenuRef.current?.contains(event.target as Node)) return;
            setOpenProfileMenuId(null);
        };
        document.addEventListener('mousedown', closeOutside);
        return () => document.removeEventListener('mousedown', closeOutside);
    }, [openProfileMenuId]);

    // Kept local because it is active only while this screen is mounted.
    const shortcuts = (
        <KeyboardShortcuts
            enabled={!isCreating && !isImporting && !editingProfile && !pendingImport && !isBusy}
            handlers={{
                'new-profile': () => {
                    setSelectedPlatform('windows');
                    setIsCreating(true);
                },
                'import-profile': () => {
                    setImportCode('');
                    setSelectedPlatform('windows');
                    setIsImporting(true);
                },
                'browse-mods': onBrowseMods,
            }}
        />
    );

    const filteredProfiles = profiles.filter(p => p.gameIdentifier === selectedGameIdentifier);
    const isMacCompatible = selectedGamePlatform?.mac ?? false;

    const handleCreate = (e: React.FormEvent) => {
        e.preventDefault();
        if (newProfileName.trim()) {
            const platform = (isMacCompatible || selectedPlatform === 'mac') ? selectedPlatform : 'windows';
            onCreateProfile(newProfileName.trim(), platform);
            setNewProfileName('');
            setIsCreating(false);
            setSelectedPlatform('windows');
        }
    };

    const handleImport = (e: React.FormEvent) => {
        e.preventDefault();
        if (importCode.trim()) {
            if (isMacCompatible) {
                // Show the platform picker before importing
                setPendingImport(importCode.trim());
                setSelectedPlatform('windows');
                setIsImporting(false);
            } else {
                onImportProfile(importCode.trim(), 'windows');
                setImportCode('');
                setIsImporting(false);
            }
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

    const formatLastPlayed = (lastUsed: number) => {
        if (!lastUsed) {
            return 'Never played';
        }

        const now = new Date();
        const playedAt = new Date(lastUsed);
        const dayMs = 24 * 60 * 60 * 1000;
        const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
        const startOfPlayedDay = new Date(playedAt.getFullYear(), playedAt.getMonth(), playedAt.getDate()).getTime();
        const dayDiff = Math.floor((startOfToday - startOfPlayedDay) / dayMs);
        const diffMs = Math.max(0, now.getTime() - playedAt.getTime());
        const diffMinutes = Math.floor(diffMs / (60 * 1000));

        if (dayDiff === 0) {
            if (diffMinutes < 1) {
                return 'Just now';
            }
            if (diffMinutes < 60) {
                return `${diffMinutes} minute${diffMinutes === 1 ? '' : 's'} ago`;
            }

            const diffHours = Math.floor(diffMinutes / 60);
            return `${diffHours} hour${diffHours === 1 ? '' : 's'} ago`;
        }

        if (dayDiff === 1) {
            return 'Yesterday';
        }

        return playedAt.toLocaleDateString();
    };

    const hasProfilesForGame = filteredProfiles.length > 0;

    return (
        <div className="relative flex-1 min-h-0">
            {shortcuts}
            <div className="h-full overflow-y-auto p-8 pb-32">
            <div className="max-w-4xl mx-auto">
                <div className="flex items-start justify-between gap-6 mb-8">
                    <div className="min-w-0">
                        <h1 className="text-3xl font-bold text-white mb-2">Select Profile</h1>
                        <p className="text-gray-400">Choose a profile to manage mods for {selectedGameName || selectedGameIdentifier}</p>
                    </div>
                    {hasProfilesForGame && (
                        <div className="flex items-center gap-1.5 rounded-full border border-gray-700 bg-gray-800/85 p-1 shadow-sm backdrop-blur-sm shrink-0">
                            {/* The magnifier sits with the other profile
                                controls, so it has to mean searching profiles.
                                It used to open the mod catalogue, which is why
                                Browse Mods below no longer wears this icon. */}
                            <button
                                onClick={onFindProfile}
                                className="w-10 h-10 rounded-full flex items-center justify-center text-gray-300 hover:text-white hover:bg-gray-500/15 hover:border-gray-500/30 border border-transparent transition-colors"
                                title="Find Profile"
                                aria-label="Find Profile"
                            >
                                <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
                                </svg>
                            </button>
                            <button
                                onClick={() => {
                                    setSelectedPlatform('windows');
                                    setIsCreating(true);
                                }}
                                className="w-10 h-10 rounded-full flex items-center justify-center text-gray-300 hover:text-white hover:bg-blue-500/15 hover:border-blue-500/30 border border-transparent transition-colors"
                                title="Create New Profile"
                                aria-label="Create New Profile"
                            >
                                <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
                                </svg>
                            </button>
                            <button
                                onClick={() => setIsImporting(true)}
                                className="w-10 h-10 rounded-full flex items-center justify-center text-gray-300 hover:text-white hover:bg-purple-500/15 hover:border-purple-500/30 border border-transparent transition-colors"
                                title="Import Profile"
                                aria-label="Import Profile"
                            >
                                <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 14l-7 7m0 0l-7-7m7 7V3" />
                                </svg>
                            </button>
                            <button
                                onClick={onBrowseMods}
                                className="w-10 h-10 rounded-full flex items-center justify-center text-gray-300 hover:text-white hover:bg-green-500/15 hover:border-green-500/30 border border-transparent transition-colors"
                                title="Browse Mods"
                                aria-label="Browse Mods"
                            >
                                <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 5.5A1.5 1.5 0 015.5 4h3A1.5 1.5 0 0110 5.5v3A1.5 1.5 0 018.5 10h-3A1.5 1.5 0 014 8.5v-3zm10 0A1.5 1.5 0 0115.5 4h3A1.5 1.5 0 0120 5.5v3A1.5 1.5 0 0118.5 10h-3A1.5 1.5 0 0114 8.5v-3zM4 15.5A1.5 1.5 0 015.5 14h3A1.5 1.5 0 0110 15.5v3A1.5 1.5 0 018.5 20h-3A1.5 1.5 0 014 18.5v-3zm10 0A1.5 1.5 0 0115.5 14h3a1.5 1.5 0 011.5 1.5v3a1.5 1.5 0 01-1.5 1.5h-3a1.5 1.5 0 01-1.5-1.5v-3z" />
                                </svg>
                            </button>
                        </div>
                    )}
                </div>

                {!hasProfilesForGame && (
                    <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
                        <div
                            className="bg-gray-800/50 border-2 border-dashed border-gray-700 rounded-xl p-6 flex flex-col items-center justify-center text-center hover:border-blue-500/50 hover:bg-gray-800 transition-all cursor-pointer min-h-[200px] group"
                            onClick={() => {
                                setSelectedPlatform('windows');
                                setIsCreating(true);
                            }}
                        >
                            <div className="w-16 h-16 bg-gray-900 rounded-full flex items-center justify-center mb-4 group-hover:bg-blue-500/20 transition-colors">
                                <svg xmlns="http://www.w3.org/2000/svg" className="h-8 w-8 text-gray-500 group-hover:text-fg-accent transition-colors" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
                                </svg>
                            </div>
                            <h3 className="text-lg font-bold text-white mb-1">Create New</h3>
                            <p className="text-sm text-gray-500">Start fresh with a new profile</p>
                        </div>

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

                        <div
                            className="bg-gray-800/50 border-2 border-dashed border-gray-700 rounded-xl p-6 flex flex-col items-center justify-center text-center hover:border-green-500/50 hover:bg-gray-800 transition-all cursor-pointer min-h-[200px] group"
                            onClick={onBrowseMods}
                        >
                            <div className="w-16 h-16 bg-gray-900 rounded-full flex items-center justify-center mb-4 group-hover:bg-green-500/20 transition-colors">
                                <svg xmlns="http://www.w3.org/2000/svg" className="h-8 w-8 text-gray-500 group-hover:text-fg-success transition-colors" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 5.5A1.5 1.5 0 015.5 4h3A1.5 1.5 0 0110 5.5v3A1.5 1.5 0 018.5 10h-3A1.5 1.5 0 014 8.5v-3zm10 0A1.5 1.5 0 0115.5 4h3A1.5 1.5 0 0120 5.5v3A1.5 1.5 0 0118.5 10h-3A1.5 1.5 0 0114 8.5v-3zM4 15.5A1.5 1.5 0 015.5 14h3A1.5 1.5 0 0110 15.5v3A1.5 1.5 0 018.5 20h-3A1.5 1.5 0 014 18.5v-3zm10 0A1.5 1.5 0 0115.5 14h3a1.5 1.5 0 011.5 1.5v3a1.5 1.5 0 01-1.5 1.5h-3a1.5 1.5 0 01-1.5-1.5v-3z" />
                                </svg>
                            </div>
                            <h3 className="text-lg font-bold text-white mb-1">Browse Mods</h3>
                            <p className="text-sm text-gray-500">Explore without a profile</p>
                        </div>
                    </div>
                )}

                {hasProfilesForGame && (
                    <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
                        {filteredProfiles.map(profile => (
                        <div
                            key={profile.id}
                            onClick={() => onSelectProfile(profile.id)}
                            onContextMenu={async (e) => {
                                e.preventDefault();
                                e.stopPropagation();
                                const menuItem = await MenuItem.new({
                                    text: revealInFileManagerLabel(),
                                    action: async () => {
                                        try {
                                            await window.ipcRenderer.openProfileFolder(profile.id);
                                        } catch (err) {
                                            console.error('Failed to open profile folder:', err);
                                        }
                                    }
                                });
                                const menu = await Menu.new({ items: [menuItem] });
                                await menu.popup();
                            }}
                            className="bg-gray-800 border border-gray-700 rounded-xl p-6 hover:border-blue-500 transition-all cursor-pointer flex flex-col min-h-[200px] group relative overflow-hidden"
                        >
                            <div aria-hidden="true" className="pointer-events-none absolute right-0 top-0 z-10 h-24 w-48 bg-gradient-to-l from-gray-800/90 via-gray-800/50 to-transparent opacity-0 transition-opacity duration-200 group-hover:opacity-100" />
                            <div
                                ref={openProfileMenuId === profile.id ? profileMenuRef : undefined}
                                className="absolute right-0 top-0 z-20 p-5"
                            >
                                <button
                                    type="button"
                                    onClick={(event) => {
                                        event.stopPropagation();
                                        setOpenProfileMenuId((open) => open === profile.id ? null : profile.id);
                                    }}
                                    className="flex h-9 w-9 items-center justify-center rounded-full border border-transparent bg-gray-800/75 text-gray-300 opacity-0 shadow-sm backdrop-blur-sm transition-all hover:border-gray-600 hover:bg-gray-700 hover:text-white group-hover:opacity-100 group-focus-within:opacity-100 focus:opacity-100 focus:outline-none focus:ring-2 focus:ring-fg-accent/70"
                                    aria-label={`Actions for ${profile.name}`}
                                    aria-haspopup="menu"
                                    aria-expanded={openProfileMenuId === profile.id}
                                >
                                    <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
                                        <path d="M5.5 10.5A1.5 1.5 0 107 12a1.5 1.5 0 00-1.5-1.5zm6.5 0a1.5 1.5 0 101.5 1.5 1.5 1.5 0 00-1.5-1.5zm6.5 0A1.5 1.5 0 1020.5 12 1.5 1.5 0 0018.5 10.5z" />
                                    </svg>
                                </button>

                                {openProfileMenuId === profile.id && (
                                    <div
                                        role="menu"
                                        aria-label={`Actions for ${profile.name}`}
                                        className="absolute right-5 top-[3.8rem] z-30 w-52 overflow-hidden rounded-2xl border border-gray-600/80 bg-gray-800/95 p-1.5 shadow-2xl shadow-black/40 backdrop-blur-xl"
                                        onClick={(event) => event.stopPropagation()}
                                    >
                                        <button type="button" role="menuitem" disabled={isBusy} onClick={() => { setOpenProfileMenuId(null); onLaunchProfile(profile.id); }} className="flex w-full items-center gap-3 rounded-xl px-3 py-2.5 text-left text-sm font-medium text-fg-success transition-colors hover:bg-green-500/15 disabled:cursor-not-allowed disabled:opacity-50">
                                            <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M8 5.14v13.72a1 1 0 001.51.86l10.58-6.86a1 1 0 000-1.72L9.51 4.28A1 1 0 008 5.14z" /></svg>
                                            {profile.is_vanilla ? 'Run vanilla profile' : 'Run profile'}
                                        </button>
                                        <button type="button" role="menuitem" onClick={() => { setOpenProfileMenuId(null); setEditingProfile(profile); setEditName(profile.name); }} className="flex w-full items-center gap-3 rounded-xl px-3 py-2.5 text-left text-sm font-medium text-gray-100 transition-colors hover:bg-gray-700">
                                            <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" aria-hidden="true"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15.232 5.232l3.536 3.536m-2.036-5.036a2.5 2.5 0 113.536 3.536L6.5 21.036H3v-3.572L16.732 3.732z" /></svg>
                                            Edit profile
                                        </button>
                                        <button type="button" role="menuitem" disabled={isBusy} onClick={() => { setOpenProfileMenuId(null); if (profile.mods.length === 0 && !profile.is_vanilla) { alert('No mods to disable!'); return; } onToggleVanilla(profile.id, !profile.is_vanilla); }} className="flex w-full items-center gap-3 rounded-xl px-3 py-2.5 text-left text-sm font-medium text-fg-warning transition-colors hover:bg-amber-500/15 disabled:cursor-not-allowed disabled:opacity-50">
                                            <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" aria-hidden="true"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M18.364 18.364A9 9 0 005.636 5.636m12.728 12.728A9 9 0 015.636 5.636m12.728 12.728L5.636 5.636" /></svg>
                                            {profile.is_vanilla ? 'Enable mods' : 'Disable mods'}
                                        </button>
                                        <div className="my-1 border-t border-gray-700" />
                                        <button type="button" role="menuitem" onClick={async () => { setOpenProfileMenuId(null); const confirmed = await window.ipcRenderer.confirm('Delete Profile', 'Are you sure you want to delete this profile?\nALL THE INSTALLED MODS WILL BE DELETED TOO!'); if (confirmed) await onDeleteProfile(profile.id, selectedGameIdentifier); }} className="flex w-full items-center gap-3 rounded-xl px-3 py-2.5 text-left text-sm font-medium text-fg-danger transition-colors hover:bg-red-500/15">
                                            <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" aria-hidden="true"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" /></svg>
                                            Delete profile
                                        </button>
                                    </div>
                                )}
                            </div>

                            <div className={`flex-1 flex flex-col transition-all duration-300 ${profile.is_vanilla ? 'grayscale opacity-60' : ''}`}>
                                {profile.profileImageUrl ? (
                                    <img
                                        src={profile.profileImageUrl}
                                        alt={profile.name}
                                        className="w-16 h-16 rounded-2xl mb-4 flex-shrink-0 object-cover bg-gray-800 shadow-md"
                                    />
                                ) : (
                                    <div style={{ backgroundImage: getProfileAvatarGradient(profile.name, profile.id) }} className="w-16 h-16 rounded-2xl mb-4 flex-shrink-0 flex items-center justify-center text-2xl font-bold text-[#ffffff] shadow-md">
                                        {getProfileInitial(profile.name)}
                                    </div>
                                )}

                                <div className="flex min-w-0 items-center gap-2 mb-2">
                                    <HoverMarquee text={profile.name} className="text-xl font-bold text-white" />
                                    {/* Platform badge */}
                                    {profile.platform === 'mac' ? (
                                        <span title="macOS profile" className="flex flex-col justify-center items-center flex-shrink-0 text-gray-400 w-4 h-4 pb-0.5">
                                            <svg xmlns="http://www.w3.org/2000/svg" className="w-[12px] h-[14px]" viewBox="0 0 384 512" fill="currentColor">
                                                <path d="M318.7 268.7c-.2-36.7 16.4-64.4 50-84.8-18.8-26.9-47.2-41.7-84.7-44.6-35.5-2.8-74.3 20.7-88.5 20.7-15 0-49.4-19.7-76.4-19.7C63.3 141.2 4 184.8 4 273.5q0 39.3 14.4 81.2c12.8 36.7 59 126.7 107.2 125.2 25.2-.6 43-17.9 75.8-17.9 31.8 0 48.3 17.9 76.4 17.9 48.6-.7 90.4-82.5 102.6-119.3-65.2-30.7-61.7-90-61.7-91.9zm-56.6-164.2c27.3-32.4 24.8-61.9 24-72.5-24.1 1.4-52 16.4-67.9 34.9-17.5 19.8-27.8 44.3-25.6 71.9 26.1 2 49.9-11.4 69.5-34.3z" />
                                            </svg>
                                        </span>
                                    ) : (
                                        <span title="Windows/Wine profile" className="flex flex-col justify-center items-center flex-shrink-0 text-gray-400 w-4 h-4">
                                            <svg xmlns="http://www.w3.org/2000/svg" className="w-[14px] h-[14px]" viewBox="0 0 24 24" fill="currentColor">
                                                <path d="M0 3.449L9.75 2.1v9.451H0m10.949-9.602L24 0v11.4H10.949M0 12.6h9.75v9.451L0 20.699M10.949 12.6H24V24l-12.9-1.801" />
                                            </svg>
                                        </span>
                                    )}
                                </div>

                                <div className="mt-auto space-y-2">
                                    <div className="flex items-center text-sm text-gray-400 gap-2">
                                        <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4" />
                                        </svg>
                                        {profile.is_vanilla ? (
                                            <span className="text-gray-500 font-bold uppercase tracking-wider text-xs">DISABLED</span>
                                        ) : (
                                            <span>{profile.mods.length} mods in profile</span>
                                        )}
                                    </div>
                                    <div className="flex items-center text-sm text-gray-400 gap-2">
                                        <span title="Last played" aria-label="Last played" className="flex items-center justify-center cursor-help">
                                            <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" aria-hidden="true">
                                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
                                            </svg>
                                        </span>
                                        <span>{formatLastPlayed(profile.lastUsed)}</span>
                                    </div>
                                </div>
                            </div>
                        </div>
                    ))}
                    </div>
                )}
            </div>

            {/* Create Profile Modal */}
            {
                isCreating && (
                    <div
                        className="fixed inset-0 bg-black/70 backdrop-blur-sm flex items-center justify-center z-50 p-4"
                        onClick={() => {
                            setIsCreating(false);
                            setSelectedPlatform('windows');
                        }}
                    >
                        <div
                            className="bg-gray-800 rounded-xl p-6 max-w-md w-full border border-gray-700 shadow-2xl relative"
                            onClick={(e) => e.stopPropagation()}
                        >
                            <div className="flex justify-between items-start mb-4">
                                <h2 className="text-2xl font-bold text-white">Create New Profile</h2>
                                {!isMacCompatible && (
                                    <div
                                        onClick={() => setSelectedPlatform(selectedPlatform === 'mac' ? 'windows' : 'mac')}
                                        className="flex items-center gap-3 cursor-pointer group select-none"
                                        title="Force this profile to use MacOS structure"
                                    >
                                        <span className={`text-sm font-semibold transition-colors ${selectedPlatform === 'mac' ? 'text-white' : 'text-gray-400'}`}>
                                            Force MacOS
                                        </span>
                                        <Toggle
                                            value={selectedPlatform === 'mac'}
                                            label="Force MacOS structure"
                                            onChange={(next) => setSelectedPlatform(next ? 'mac' : 'windows')}
                                        />
                                    </div>
                                )}
                            </div>
                            <form onSubmit={handleCreate}>
                                <input
                                    type="text"
                                    value={newProfileName}
                                    onChange={(e) => setNewProfileName(e.target.value)}
                                    placeholder="Profile Name (e.g. My Modpack)"
                                    className="w-full bg-gray-900 border border-gray-700 rounded-lg px-4 py-3 text-white placeholder-gray-500 focus:outline-none focus:border-blue-500 mb-6"
                                    autoFocus
                                />

                                {isMacCompatible && (
                                    <div className="mb-6">
                                        <PlatformPicker
                                            value={selectedPlatform}
                                            onChange={setSelectedPlatform}
                                        />
                                    </div>
                                )}

                                <div className="flex gap-3">
                                    <Button
                                        variant="secondary"
                                        fullWidth
                                        onClick={() => {
                                            setIsCreating(false);
                                            setSelectedPlatform('windows');
                                        }}
                                        type="button"
                                    >
                                        Cancel
                                    </Button>
                                    <Button variant="primary" fullWidth type="submit" disabled={!newProfileName.trim()}>
                                        Create
                                    </Button>
                                </div>
                            </form>
                        </div>
                    </div>
                )
            }

            {/* Import Profile Modal */}
            {
                isImporting && (
                    <div
                        className="fixed inset-0 bg-black/70 backdrop-blur-sm flex items-center justify-center z-50 p-4"
                        onClick={() => setIsImporting(false)}
                    >
                        <div
                            className="bg-gray-800 rounded-xl p-6 max-w-md w-full border border-gray-700 shadow-2xl"
                            onClick={(e) => e.stopPropagation()}
                        >
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
                                        <Button variant="purple" size="sm" type="submit" disabled={!importCode.trim()}>
                                            Import
                                        </Button>
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
                                                    if (isMacCompatible) {
                                                        // Show platform picker before importing file
                                                        setPendingImport({ file: filePath });
                                                        setSelectedPlatform('windows');
                                                        setIsImporting(false);
                                                    } else {
                                                        onImportFile(filePath, 'windows');
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
                                <Button variant="ghost" size="sm" onClick={() => setIsImporting(false)}>
                                    Cancel
                                </Button>
                            </div>
                        </div>
                    </div>
                )
            }

            {/* Import Platform Picker - shown after code/file is ready, but only for Mac-compatible games */}
            {
                pendingImport !== null && (
                    <div
                        className="fixed inset-0 bg-black/70 backdrop-blur-sm flex items-center justify-center z-50 p-4"
                        onClick={() => {
                            setPendingImport(null);
                            setImportCode('');
                        }}
                    >
                        <div
                            className="bg-gray-800 rounded-xl p-6 max-w-md w-full border border-gray-700 shadow-2xl"
                            onClick={(e) => e.stopPropagation()}
                        >
                            <h2 className="text-2xl font-bold text-white mb-2">Choose Platform</h2>
                            <p className="text-gray-400 text-sm mb-6">Select which platform you want to use this profile on.</p>

                            <div className="mb-6">
                                <PlatformPicker
                                    value={selectedPlatform}
                                    onChange={setSelectedPlatform}
                                />
                            </div>
                            <div className="flex gap-3">
                                <Button variant="secondary" fullWidth type="button" onClick={() => {
                                    setPendingImport(null);
                                    setImportCode('');
                                }}>
                                    Cancel
                                </Button>
                                <Button variant="primary" fullWidth type="button" onClick={() => {
                                    if (typeof pendingImport === 'string') {
                                        onImportProfile(pendingImport, selectedPlatform);
                                    } else if (pendingImport && 'file' in pendingImport) {
                                        onImportFile(pendingImport.file, selectedPlatform);
                                    }
                                    setPendingImport(null);
                                    setImportCode('');
                                }}>
                                    Import
                                </Button>
                            </div>
                        </div>
                    </div>
                )
            }

            {/* Edit Profile Modal */}
            {
                editingProfile && (
                    <div
                        className="fixed inset-0 bg-black/70 backdrop-blur-sm flex items-center justify-center z-50 p-4"
                        onClick={() => setEditingProfile(null)}
                    >
                        <div
                            className="bg-gray-800 rounded-xl p-6 max-w-md w-full border border-gray-700 shadow-2xl"
                            onClick={(e) => e.stopPropagation()}
                        >
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
                                        <div style={{ backgroundImage: getProfileAvatarGradient(editName, editingProfile.id) }} className="w-24 h-24 rounded-full flex items-center justify-center text-4xl font-bold text-[#ffffff] border-4 border-gray-700 group-hover:border-blue-500 transition-colors">
                                            {getProfileInitial(editName)}
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
                                        className="text-xs text-fg-danger hover:text-fg-danger hover:underline"
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
                                    <Button variant="secondary" fullWidth onClick={() => {
                                        setEditingProfile(null);
                                    }} type="button">
                                        Cancel
                                    </Button>
                                    <Button variant="primary" fullWidth type="submit" disabled={!editName.trim()}>
                                        Save
                                    </Button>
                                </div>
                            </form>
                        </div>
                    </div>
                )
            }
            </div>
            <SponsorSurface placement="profile-selector-support" visible />
        </div>
    );
}
