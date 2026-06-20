import type { Package, PackageVersion } from '../types/thunderstore';
import type { InstalledMod } from '../types/profile';
import { useProfileStore } from '../store/useProfileStore';

const MAX_PARALLEL_OPS = 10;

interface ProgressSetter {
    (state: { isOpen: boolean; title: string; progress: number; currentTask: string }): void;
    (updater: (prev: { isOpen: boolean; title: string; progress: number; currentTask: string }) => { isOpen: boolean; title: string; progress: number; currentTask: string }): void;
}

type DepDetail = {
    name: string;
    icon?: string;
};

interface UninstallModalSetter {
    (state: {
        isOpen: boolean;
        pkg: Package | null;
        orphanDeps: DepDetail[];
        allInstalledDepDetails: DepDetail[];
        allInstalledDeps: string[];
        profileId: string | null;
    }): void;
    (updater: (prev: {
        isOpen: boolean;
        pkg: Package | null;
        orphanDeps: DepDetail[];
        allInstalledDepDetails: DepDetail[];
        allInstalledDeps: string[];
        profileId: string | null;
    }) => {
        isOpen: boolean;
        pkg: Package | null;
        orphanDeps: DepDetail[];
        allInstalledDepDetails: DepDetail[];
        allInstalledDeps: string[];
        profileId: string | null;
    }): void;
}

interface UseModActionsProps {
    activeProfileId: string | null;
    selectedCommunity: string | null;
    legacyInstallMode: boolean;
    installInParallel: boolean;
    uninstallModalState: {
        isOpen: boolean;
        pkg: Package | null;
        orphanDeps: DepDetail[];
        allInstalledDepDetails: DepDetail[];
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
    installInParallel,
    uninstallModalState,
    setProgressState,
    setUninstallModalState,
}: UseModActionsProps) {
    const { profiles, addMod, removeMod, updateProfile } = useProfileStore();

    const getPackageKey = (fullName: string) => {
        const parts = fullName.split('-');
        return parts.length >= 2 ? `${parts[0]}-${parts[1]}` : fullName;
    };

    const runWithConcurrency = async (tasks: Array<() => Promise<void>>, maxConcurrency: number) => {
        for (let i = 0; i < tasks.length; i += maxConcurrency) {
            const batch = tasks.slice(i, i + maxConcurrency);
            if (maxConcurrency === 1) {
                await batch[0]();
            } else {
                await Promise.all(batch.map((task) => task()));
            }
        }
    };

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
                const dependencyTasks = result.found
                    .map((depPkg) => {
                        const depVersion = depPkg.versions[0];
                        if (!depVersion) return null;
                        return () => installModWithDependencies(depPkg, depVersion, installedCache, profileIdToUse, progressCounter, gamePath);
                    })
                    .filter((task): task is () => Promise<void> => !!task);

                const concurrency = installInParallel ? MAX_PARALLEL_OPS : 1;
                if (dependencyTasks.length > 0) {
                    await runWithConcurrency(dependencyTasks, concurrency);
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

            // If Outer Wilds, dynamically load and install post-install dependencies!
            if (selectedCommunity === 'outerwilds' && Array.isArray(result.dependencies) && result.dependencies.length > 0) {
                for (const depUniqueName of result.dependencies) {
                    if (depUniqueName.toLowerCase() === 'alek.owml' || depUniqueName.toLowerCase() === 'owml') continue;

                    const depName = depUniqueName.replace('.', '-'); // normalized to hyphen
                    const activeProfile = useProfileStore.getState().profiles.find(p => p.id === profileIdToUse);
                    const isAlreadyAdded = activeProfile?.mods.some(m => {
                        const parts = m.fullName.split('-');
                        const matchKey = parts.length >= 2 ? `${parts[0]}-${parts[1]}` : m.fullName;
                        return matchKey.replace('.', '-').toLowerCase() === depName.toLowerCase();
                    });

                    if (isAlreadyAdded || installedCache.has(depName)) continue;

                    // Fetch package information
                    const depPkg = await window.ipcRenderer.fetchPackageByName(depUniqueName, 'outerwilds');
                    if (depPkg) {
                        const depVersion = depPkg.versions[0];
                        if (depVersion) {
                            if (progressCounter) progressCounter.total++;
                            // Install dependency recursively
                            await installModWithDependencies(depPkg, depVersion, installedCache, profileIdToUse, progressCounter, gamePath);
                        }
                    }
                }
            }
        } else {
            throw new Error(result.error || 'Failed to install mod');
        }
    };

    // ── Install mod (legacy = download now, new = metadata only) ─────────────────
    const handleInstallMod = async (
        pkg: Package,
        targetProfileId?: string,
        selectedVersion?: PackageVersion
    ): Promise<void> => {
        const profileIdToUse = targetProfileId || activeProfileId;
        if (!profileIdToUse) { alert('Please select a profile first'); return; }

        const version = selectedVersion || pkg.versions[0];
        const targetProfile = profiles.find(p => p.id === profileIdToUse);

        if (legacyInstallMode) {
            const gamePath = await window.ipcRenderer.getGamePath(selectedCommunity || '', targetProfile?.platform);
            if (!gamePath) {
                await window.ipcRenderer.alert('Game Path Required', 'Please configure the game directory in Settings before installing mods in Legacy mode.\n\nLegacy mode installs mods directly into the game folder. Go to Settings → Game Directory to set the path.');
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

        // New mode: metadata only (no progress modal - this is not a real install)
        try {
            const modsToAdd: InstalledMod[] = [];
            const processed = new Set<string>();

            const collectModAndDeps = async (_pkg: Package, ver: PackageVersion) => {
                if (processed.has(ver.full_name)) return;
                processed.add(ver.full_name);
                const profile = profiles.find(p => p.id === profileIdToUse);
                if (profile?.mods.some(m => m.fullName === ver.full_name)) return;
                modsToAdd.push({
                    uuid4: ver.uuid4,
                    fullName: ver.full_name,
                    versionNumber: ver.version_number,
                    iconUrl: ver.icon,
                    enabled: true,
                    pending_sync: true,
                });

                const depsToResolve: string[] = [];
                for (const depString of ver.dependencies) {
                    const parts = depString.split('-');
                    if (parts.length < 3) continue;
                    const depFullName = `${parts[0]}-${parts[1]}`;
                    if (profile?.mods.some(m => m.fullName.startsWith(depFullName))) continue;
                    if (processed.has(depFullName)) continue;
                    depsToResolve.push(depFullName);
                }

                if (!selectedCommunity || depsToResolve.length === 0) return;

                const result = await window.ipcRenderer.lookupPackagesByNames(selectedCommunity, depsToResolve);
                const dependencyTasks = result.found
                    .map((depPkg) => {
                        const depVer = depPkg.versions[0];
                        if (!depVer) return null;
                        return () => collectModAndDeps(depPkg, depVer);
                    })
                    .filter((task): task is () => Promise<void> => !!task);

                const concurrency = installInParallel ? MAX_PARALLEL_OPS : 1;
                if (dependencyTasks.length > 0) {
                    await runWithConcurrency(dependencyTasks, concurrency);
                }
            };

            await collectModAndDeps(pkg, version);
            for (const mod of modsToAdd) addMod(profileIdToUse, mod);
            updateProfile(profileIdToUse, { needs_sync: true });
        } catch (err: any) {
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
            setUninstallModalState({ isOpen: true, pkg, orphanDeps: [], allInstalledDepDetails: [], allInstalledDeps: [], profileId: profileIdToUse });
            return;
        }

        const targetPackageKey = getPackageKey(pkg.full_name);
        const modDependencies = Array.from(new Set(
            version.dependencies
                .map(getPackageKey)
                .filter((dep) => dep.length > 0)
                .filter((dep) => dep.toLowerCase() !== targetPackageKey.toLowerCase())
                .filter((dep) => !dep.toLowerCase().includes('bepinexpack'))
        ));

        const allInstalledDepDetails = modDependencies
            .map((dep) => {
                const installedDep = profile.mods.find((m) => getPackageKey(m.fullName).toLowerCase() === dep.toLowerCase());
                const detail: DepDetail | null = installedDep ? { name: dep, icon: installedDep.iconUrl } : null;
                return detail;
            })
            .filter((dep): dep is DepDetail => dep !== null);

        const orphanDepsDetails: DepDetail[] = [];

        if (modDependencies.length > 0 && selectedCommunity) {
            const otherMods = profile.mods.filter(m => !m.fullName.startsWith(pkg.full_name));
            const otherModNames = otherMods.map((m) => getPackageKey(m.fullName));
            const otherModsDeps = new Set<string>();

            if (otherModNames.length > 0) {
                try {
                    const result = await window.ipcRenderer.lookupPackagesByNames(selectedCommunity, otherModNames);
                    for (const otherPkg of result.found) {
                        const otherVer = otherPkg.versions[0];
                        if (otherVer) {
                            for (const dep of otherVer.dependencies) {
                                const depKey = getPackageKey(dep);
                                if (depKey.toLowerCase() !== targetPackageKey.toLowerCase()) {
                                    otherModsDeps.add(depKey);
                                }
                            }
                        }
                    }
                } catch (err) { console.error('Failed to lookup deps:', err); }
            }

            for (const dep of modDependencies) {
                if (!otherModsDeps.has(dep)) {
                    const installedDep = allInstalledDepDetails.find((item) => item.name.toLowerCase() === dep.toLowerCase());
                    if (installedDep) orphanDepsDetails.push(installedDep);
                }
            }
        }

        const allInstalledDeps = modDependencies.filter((dep) =>
            profile.mods.some((m) => getPackageKey(m.fullName).toLowerCase() === dep.toLowerCase())
        );

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

        setUninstallModalState({
            isOpen: true,
            pkg,
            orphanDeps: orphanDepsDetails,
            allInstalledDepDetails,
            allInstalledDeps,
            profileId: profileIdToUse,
        });
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
    const handleUpdateMod = async (
        pkg: Package,
        targetProfileId?: string,
        selectedVersion?: PackageVersion
    ): Promise<void> => {
        const profileIdToUse = targetProfileId || activeProfileId;
        if (!profileIdToUse) { alert('Please select a profile first'); return; }
        const targetVersion = selectedVersion || pkg.versions[0];
        const targetProfile = profiles.find(p => p.id === profileIdToUse);

        if (legacyInstallMode) {
            setProgressState({
                isOpen: true,
                title: `Updating ${pkg.name}`,
                progress: 0,
                currentTask: 'Removing old version...',
            });
            try {
                const profile = profiles.find(p => p.id === profileIdToUse);
                const oldMods = profile?.mods.filter(m => m.fullName.startsWith(pkg.full_name)) || [];
                if (oldMods.length > 0) {
                    setProgressState(prev => ({
                        ...prev,
                        progress: 20,
                        currentTask: 'Uninstalling old version...',
                    }));
                    for (const oldMod of oldMods) {
                        await removeMod(profileIdToUse, oldMod.uuid4);
                    }
                }
                setProgressState(prev => ({
                    ...prev,
                    progress: 40,
                    currentTask: `Installing v${targetVersion.version_number}...`,
                }));

                const gamePath = await window.ipcRenderer.getGamePath(selectedCommunity || '', targetProfile?.platform);
                if (!gamePath) {
                    throw new Error('Game directory not configured. Open Settings → Game Directory to set the path before updating mods in Legacy mode.');
                }
                await installModWithDependencies(pkg, targetVersion, new Set(), profileIdToUse, undefined, gamePath);

                setProgressState(prev => ({
                    ...prev,
                    progress: 100,
                    currentTask: 'Update complete!',
                }));
                setTimeout(() => setProgressState(prev => ({ ...prev, isOpen: false })), 500);
            } catch (err: any) {
                setProgressState(prev => ({ ...prev, isOpen: false }));
                alert(`Failed to update mod: ${err.message}`);
            }
            return;
        }

        // New mode update: metadata-only change, no progress modal
        try {
            const profile = profiles.find(p => p.id === profileIdToUse);
            const oldMods = profile?.mods.filter(m => m.fullName.startsWith(pkg.full_name)) || [];
            const wasEnabled = oldMods.some(m => m.enabled) || oldMods.length === 0;
            for (const oldMod of oldMods) {
                await removeMod(profileIdToUse, oldMod.uuid4);
            }

            addMod(profileIdToUse, {
                uuid4: targetVersion.uuid4,
                fullName: targetVersion.full_name,
                versionNumber: targetVersion.version_number,
                iconUrl: targetVersion.icon,
                enabled: wasEnabled,
                pending_sync: true,
            });
            updateProfile(profileIdToUse, { needs_sync: true });
        } catch (err: any) {
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
