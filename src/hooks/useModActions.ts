import type { Package, PackageVersion } from '../types/thunderstore';
import type { InstalledMod } from '../types/profile';
import { useProfileStore } from '../store/useProfileStore';

interface ProgressSetter {
    (state: { isOpen: boolean; title: string; progress: number; currentTask: string }): void;
    (updater: (prev: { isOpen: boolean; title: string; progress: number; currentTask: string }) => { isOpen: boolean; title: string; progress: number; currentTask: string }): void;
}

interface UninstallModalSetter {
    (state: {
        isOpen: boolean;
        pkg: Package | null;
        orphanDeps: { name: string; icon?: string }[];
        allInstalledDeps: string[];
        profileId: string | null;
    }): void;
    (updater: (prev: {
        isOpen: boolean;
        pkg: Package | null;
        orphanDeps: { name: string; icon?: string }[];
        allInstalledDeps: string[];
        profileId: string | null;
    }) => {
        isOpen: boolean;
        pkg: Package | null;
        orphanDeps: { name: string; icon?: string }[];
        allInstalledDeps: string[];
        profileId: string | null;
    }): void;
}

interface UseModActionsProps {
    activeProfileId: string | null;
    selectedCommunity: string | null;
    legacyInstallMode: boolean;
    uninstallModalState: {
        isOpen: boolean;
        pkg: Package | null;
        orphanDeps: { name: string; icon?: string }[];
        allInstalledDeps: string[];
        profileId: string | null;
    };
    setProgressState: ProgressSetter;
    setUninstallModalState: UninstallModalSetter;
}

export function useModActions({
    activeProfileId,
    selectedCommunity,
    legacyInstallMode,
    uninstallModalState,
    setProgressState,
    setUninstallModalState,
}: UseModActionsProps) {
    const { profiles, addMod, removeMod } = useProfileStore();

    // ── Core recursive installer (handles dependencies) ──────────────────────────
    const installModWithDependencies = async (
        pkg: Package,
        version: PackageVersion,
        installedCache: Set<string> = new Set(),
        targetProfileId?: string,
        progressCounter?: { installed: number; total: number },
        gamePath?: string
    ): Promise<void> => {
        if (installedCache.has(version.full_name)) return;
        installedCache.add(version.full_name);

        const profileIdToUse = targetProfileId || activeProfileId;
        if (!profileIdToUse) throw new Error('No profile selected');
        if (!gamePath) throw new Error('Game path not provided');

        // 1. Collect deps to install
        const depsToInstall: string[] = [];
        for (const depString of version.dependencies) {
            const parts = depString.split('-');
            if (parts.length < 3) continue;
            const depFullName = `${parts[0]}-${parts[1]}`;
            const activeProfile = profiles.find(p => p.id === profileIdToUse);
            if (activeProfile?.mods.some(m => m.fullName.startsWith(depFullName))) continue;
            if (installedCache.has(depFullName)) continue;
            depsToInstall.push(depFullName);
        }

        // 2. Batch-fetch missing deps
        if (depsToInstall.length > 0 && selectedCommunity) {
            setProgressState(prev => ({ ...prev, currentTask: `Fetching ${depsToInstall.length} dependencies...` }));
            try {
                const result = await window.ipcRenderer.lookupPackagesByNames(selectedCommunity, depsToInstall);
                if (progressCounter) progressCounter.total += result.found.length;
                for (const depPkg of result.found) {
                    const depVersion = depPkg.versions[0];
                    if (depVersion) {
                        await installModWithDependencies(depPkg, depVersion, installedCache, profileIdToUse, progressCounter, gamePath);
                    }
                }
            } catch (err) {
                console.error('[Dependencies] Failed to lookup:', err);
            }
        }

        // 3. Install the mod itself
        if (progressCounter) {
            progressCounter.installed++;
            const progress = Math.min(95, Math.round((progressCounter.installed / progressCounter.total) * 100));
            setProgressState(prev => ({ ...prev, progress, currentTask: `Installing ${pkg.name}... (${progressCounter.installed}/${progressCounter.total})` }));
        } else {
            setProgressState(prev => ({ ...prev, currentTask: `Installing ${pkg.name}...` }));
        }

        const result = await window.ipcRenderer.installMod(
            profileIdToUse,
            version.download_url,
            version.full_name,
            gamePath,
            true
        );

        if (result.success) {
            addMod(profileIdToUse, {
                uuid4: version.uuid4,
                fullName: version.full_name,
                versionNumber: version.version_number,
                iconUrl: version.icon,
                enabled: true,
            });
        } else {
            throw new Error(result.error);
        }
    };

    // ── Install mod (legacy = download now, new = metadata only) ─────────────────
    const handleInstallMod = async (pkg: Package, targetProfileId?: string): Promise<void> => {
        const profileIdToUse = targetProfileId || activeProfileId;
        if (!profileIdToUse) { alert('Please select a profile first'); return; }

        const version = pkg.versions[0];

        if (legacyInstallMode) {
            const gamePath = await window.ipcRenderer.getGamePath(selectedCommunity || '');
            if (!gamePath) {
                await window.ipcRenderer.alert('Game Path Required', 'Please configure the game directory in Settings before installing mods.');
                return;
            }
            setProgressState({ isOpen: true, title: `Installing ${pkg.name}`, progress: 0, currentTask: 'Starting installation...' });
            try {
                const counter = { installed: 0, total: 1 };
                await installModWithDependencies(pkg, version, new Set(), profileIdToUse, counter, gamePath);
                setProgressState(prev => ({ ...prev, progress: 100, currentTask: 'Done!' }));
                setTimeout(() => setProgressState(prev => ({ ...prev, isOpen: false })), 500);
            } catch (err: any) {
                setProgressState(prev => ({ ...prev, isOpen: false }));
                alert(`Failed to install mod: ${err.message}`);
            }
            return;
        }

        // New mode: metadata only
        setProgressState({ isOpen: true, title: `Adding ${pkg.name}`, progress: 0, currentTask: 'Resolving dependencies...' });
        try {
            const modsToAdd: InstalledMod[] = [];
            const processed = new Set<string>();

            const collectModAndDeps = async (_pkg: Package, ver: PackageVersion) => {
                if (processed.has(ver.full_name)) return;
                processed.add(ver.full_name);
                const profile = profiles.find(p => p.id === profileIdToUse);
                if (profile?.mods.some(m => m.fullName === ver.full_name)) return;
                modsToAdd.push({ uuid4: ver.uuid4, fullName: ver.full_name, versionNumber: ver.version_number, iconUrl: ver.icon, enabled: true });

                for (const depString of ver.dependencies) {
                    const parts = depString.split('-');
                    if (parts.length < 3) continue;
                    const depFullName = `${parts[0]}-${parts[1]}`;
                    if (profile?.mods.some(m => m.fullName.startsWith(depFullName))) continue;
                    if (processed.has(depFullName)) continue;
                    if (selectedCommunity) {
                        const result = await window.ipcRenderer.lookupPackagesByNames(selectedCommunity, [depFullName]);
                        for (const depPkg of result.found) {
                            const depVer = depPkg.versions[0];
                            if (depVer) await collectModAndDeps(depPkg, depVer);
                        }
                    }
                }
            };

            await collectModAndDeps(pkg, version);
            setProgressState(prev => ({ ...prev, progress: 80, currentTask: `Adding ${modsToAdd.length} mods to profile...` }));
            for (const mod of modsToAdd) addMod(profileIdToUse, mod);
            setProgressState(prev => ({ ...prev, progress: 100, currentTask: 'Done! Click "Apply to Game" to download.' }));
            setTimeout(() => setProgressState(prev => ({ ...prev, isOpen: false })), 800);
        } catch (err: any) {
            setProgressState(prev => ({ ...prev, isOpen: false }));
            alert(`Failed to add mod: ${err.message}`);
        }
    };

    // ── Uninstall with optional orphan dependency removal ─────────────────────────
    const handleUninstallWithDependencies = async (pkg: Package, targetProfileId?: string): Promise<void> => {
        const profileIdToUse = targetProfileId || activeProfileId;
        if (!profileIdToUse) return;
        const profile = profiles.find(p => p.id === profileIdToUse);
        if (!profile) return;

        const version = pkg.versions[0];
        if (!version) {
            setUninstallModalState({ isOpen: true, pkg, orphanDeps: [], allInstalledDeps: [], profileId: profileIdToUse });
            return;
        }

        const modDependencies = version.dependencies
            .map(dep => { const parts = dep.split('-'); return parts.length >= 2 ? `${parts[0]}-${parts[1]}` : dep; })
            .filter(dep => !dep.toLowerCase().includes('bepinexpack'));

        const orphanDepsDetails: { name: string; icon?: string }[] = [];

        if (modDependencies.length > 0 && selectedCommunity) {
            const otherMods = profile.mods.filter(m => !m.fullName.startsWith(pkg.full_name));
            const otherModNames = otherMods.map(m => { const parts = m.fullName.split('-'); return parts.length >= 2 ? `${parts[0]}-${parts[1]}` : m.fullName; });
            const otherModsDeps = new Set<string>();

            if (otherModNames.length > 0) {
                try {
                    const result = await window.ipcRenderer.lookupPackagesByNames(selectedCommunity, otherModNames);
                    for (const otherPkg of result.found) {
                        const otherVer = otherPkg.versions[0];
                        if (otherVer) {
                            for (const dep of otherVer.dependencies) {
                                const parts = dep.split('-');
                                otherModsDeps.add(parts.length >= 2 ? `${parts[0]}-${parts[1]}` : dep);
                            }
                        }
                    }
                } catch (err) { console.error('Failed to lookup deps:', err); }
            }

            for (const dep of modDependencies) {
                if (!otherModsDeps.has(dep)) {
                    const installedDep = profile.mods.find(m => m.fullName.startsWith(dep));
                    if (installedDep) orphanDepsDetails.push({ name: dep, icon: installedDep.iconUrl });
                }
            }
        }

        const allInstalledDeps = modDependencies.filter(dep => profile.mods.some(m => m.fullName.startsWith(dep)));

        if (allInstalledDeps.length === 0) {
            const confirmed = await window.ipcRenderer.confirm('Uninstall Mod', `Uninstall ${pkg.name}?`);
            if (confirmed) {
                setProgressState({ isOpen: true, title: `Uninstalling ${pkg.name}`, progress: 0, currentTask: 'Removing mod...' });
                try {
                    const installed = profile.mods.find(m => m.fullName.startsWith(pkg.full_name));
                    if (installed) await removeMod(profileIdToUse, installed.uuid4);
                    setProgressState(prev => ({ ...prev, progress: 100, currentTask: 'Done!' }));
                    setTimeout(() => setProgressState(prev => ({ ...prev, isOpen: false })), 500);
                } catch (err: any) {
                    setProgressState(prev => ({ ...prev, isOpen: false }));
                    alert(`Failed to uninstall: ${err.message}`);
                }
            }
            return;
        }

        setUninstallModalState({ isOpen: true, pkg, orphanDeps: orphanDepsDetails, allInstalledDeps, profileId: profileIdToUse });
    };

    // ── Execute confirmed uninstall from modal ────────────────────────────────────
    const executeUninstall = async (depsToRemove: string[]): Promise<void> => {
        const { pkg, profileId } = uninstallModalState;
        if (!pkg || !profileId) return;
        const profile = profiles.find(p => p.id === profileId);
        if (!profile) return;

        setUninstallModalState(prev => ({ ...prev, isOpen: false }));
        setProgressState({ isOpen: true, title: `Uninstalling ${pkg.name}`, progress: 0, currentTask: 'Removing mod...' });

        try {
            const installed = profile.mods.find(m => m.fullName.startsWith(pkg.full_name));
            if (installed) await removeMod(profileId, installed.uuid4);
            setProgressState(prev => ({ ...prev, progress: 30 }));

            const total = depsToRemove.length;
            for (let i = 0; i < total; i++) {
                const depMod = profile.mods.find(m => m.fullName.startsWith(depsToRemove[i]));
                if (depMod) {
                    setProgressState(prev => ({ ...prev, progress: 30 + Math.round((i / total) * 60), currentTask: `Removing ${depsToRemove[i]}... (${i + 1}/${total})` }));
                    await removeMod(profileId, depMod.uuid4);
                }
            }
            setProgressState(prev => ({ ...prev, progress: 100, currentTask: 'Done!' }));
            setTimeout(() => setProgressState(prev => ({ ...prev, isOpen: false })), 500);
        } catch (err: any) {
            setProgressState(prev => ({ ...prev, isOpen: false }));
            alert(`Failed to uninstall: ${err.message}`);
        }
    };

    // ── Update mod ───────────────────────────────────────────────────────────────
    const handleUpdateMod = async (pkg: Package, targetProfileId?: string): Promise<void> => {
        const profileIdToUse = targetProfileId || activeProfileId;
        if (!profileIdToUse) { alert('Please select a profile first'); return; }

        setProgressState({ isOpen: true, title: `Updating ${pkg.name}`, progress: 0, currentTask: 'Removing old version...' });
        try {
            const profile = profiles.find(p => p.id === profileIdToUse);
            const oldMod = profile?.mods.find(m => m.fullName.startsWith(pkg.full_name));
            if (oldMod) {
                setProgressState(prev => ({ ...prev, progress: 20, currentTask: 'Uninstalling old version...' }));
                await removeMod(profileIdToUse, oldMod.uuid4);
            }
            setProgressState(prev => ({ ...prev, progress: 40, currentTask: 'Installing new version...' }));
            await installModWithDependencies(pkg, pkg.versions[0], new Set(), profileIdToUse);
            setProgressState(prev => ({ ...prev, progress: 100, currentTask: 'Update complete!' }));
            setTimeout(() => setProgressState(prev => ({ ...prev, isOpen: false })), 500);
        } catch (err: any) {
            setProgressState(prev => ({ ...prev, isOpen: false }));
            alert(`Failed to update mod: ${err.message}`);
        }
    };

    return {
        installModWithDependencies,
        handleInstallMod,
        handleUninstallWithDependencies,
        executeUninstall,
        handleUpdateMod,
    };
}
