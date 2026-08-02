import type { Package, PackageVersion } from '../types/thunderstore';
import type { InstalledMod } from '../types/profile';
import { useProfileStore } from '../store/useProfileStore';
import { compareVersions, findPinnedVersion, parsePackageReference } from '../utils/modVersioning';

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

export interface ProfileModUpdate {
    mod: InstalledMod;
    pkg: Package;
    version: PackageVersion;
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
        return parsePackageReference(fullName).packageName;
    };

    const profileSatisfiesPackage = (profileId: string, fullName: string, minimumVersion: string) => {
        const packageKey = getPackageKey(fullName).toLowerCase();
        const profile = useProfileStore.getState().profiles.find(candidate => candidate.id === profileId);
        return !!profile?.mods.some(mod =>
            getPackageKey(mod.fullName).toLowerCase() === packageKey
            && compareVersions(mod.versionNumber, minimumVersion) >= 0
        );
    };

    const runWithConcurrency = async (tasks: Array<() => Promise<void>>, maxConcurrency: number) => {
        for (let i = 0; i < tasks.length; i += maxConcurrency) {
            const batch = tasks.slice(i, i + maxConcurrency);
            if (maxConcurrency === 1) {
                await batch[0]();
            } else {
                const results = await Promise.allSettled(batch.map((task) => task()));
                const rejected = results.find((result): result is PromiseRejectedResult => result.status === 'rejected');
                if (rejected) throw rejected.reason;
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
        const targetReference = parsePackageReference(version.full_name);
        const targetVersion = targetReference.version || version.version_number;
        const packageMarkerPrefix = `package:${targetReference.packageName.toLowerCase()}@`;
        const packageMarker = `${packageMarkerPrefix}${targetVersion}`;
        if (installedCache.has(packageMarker)) return;
        const conflictingMarker = Array.from(installedCache).find(entry =>
            entry.startsWith(packageMarkerPrefix) && entry !== packageMarker
        );
        if (conflictingMarker) {
            throw new Error(
                `Dependency conflict for ${targetReference.packageName}: ` +
                `${conflictingMarker.slice(packageMarkerPrefix.length)} and ${targetVersion} are both required`
            );
        }
        installedCache.add(packageMarker);
        installedCache.add(version.full_name);

        const profileIdToUse = targetProfileId || activeProfileId;
        if (!profileIdToUse) throw new Error('No profile selected');
        if (!gamePath) throw new Error('Game path not provided');

        // 1. Collect deps to install
        const depsToInstall: ReturnType<typeof parsePackageReference>[] = [];
        for (const depString of version.dependencies) {
            const dependency = parsePackageReference(depString);
            if (!dependency.version) throw new Error(`Dependency ${depString} has no pinned version`);
            // Thunderstore dependencies pin a historical version, but an equal or
            // newer installed package already satisfies it. Re-installing the pin
            // here would silently downgrade that package.
            if (profileSatisfiesPackage(profileIdToUse, dependency.packageName, dependency.version)) continue;
            if (installedCache.has(`package:${dependency.packageName.toLowerCase()}@${dependency.version}`)) continue;
            depsToInstall.push(dependency);
        }

        // 2. Batch-fetch missing deps
        if (depsToInstall.length > 0 && !selectedCommunity) {
            throw new Error('Cannot resolve pinned dependencies without a Thunderstore community');
        }
        if (depsToInstall.length > 0 && selectedCommunity) {
            setProgressState(prev => ({ ...prev, currentTask: `Fetching ${depsToInstall.length} dependencies...` }));
            try {
                const result = await window.ipcRenderer.lookupPackagesByNames(selectedCommunity, depsToInstall.map(dep => dep.fullName));
                if (progressCounter) progressCounter.total += depsToInstall.length;
                const dependencyTasks = depsToInstall.map(requirement => async () => {
                    let depPkg = result.found.find(candidate =>
                        candidate.full_name.toLowerCase() === requirement.packageName.toLowerCase()
                    );
                    let depVersion = depPkg?.versions.find(candidate => candidate.version_number === requirement.version);
                    if (!depPkg || !depVersion) {
                        const exactPackage = await window.ipcRenderer.fetchPackageByName(requirement.fullName, selectedCommunity);
                        if (!exactPackage) throw new Error(`Pinned dependency unavailable: ${requirement.fullName}`);
                        depPkg = exactPackage;
                        depVersion = findPinnedVersion(depPkg, requirement.version!, requirement.packageName);
                    }
                    await installModWithDependencies(depPkg, depVersion, installedCache, profileIdToUse, progressCounter, gamePath);
                });

                const concurrency = installInParallel ? MAX_PARALLEL_OPS : 1;
                if (dependencyTasks.length > 0) {
                    await runWithConcurrency(dependencyTasks, concurrency);
                }
            } catch (err) {
                console.error('[Dependencies] Failed to resolve:', err);
                throw err;
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
        selectedVersion?: PackageVersion,
        metadataOnly = false
    ): Promise<void> => {
        const profileIdToUse = targetProfileId || activeProfileId;
        if (!profileIdToUse) { alert('Please select a profile first'); return; }

        const version = selectedVersion || pkg.versions[0];
        const targetProfile = profiles.find(p => p.id === profileIdToUse);

        if (legacyInstallMode && !metadataOnly) {
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
            const modsToAdd = new Map<string, InstalledMod>();
            const processedVersions = new Map<string, string>();

            const collectModAndDeps = async (_pkg: Package, ver: PackageVersion, isRoot = false) => {
                const packageKey = getPackageKey(ver.full_name).toLowerCase();

                // Preserve an installed dependency even when the manifest pins an
                // older version. The root remains replaceable so explicit version
                // selection continues to work.
                if (!isRoot && profileSatisfiesPackage(profileIdToUse, ver.full_name, ver.version_number)) return;

                const processedVersion = processedVersions.get(packageKey);
                if (processedVersion && compareVersions(processedVersion, ver.version_number) >= 0) return;
                processedVersions.set(packageKey, ver.version_number);
                modsToAdd.set(packageKey, {
                    uuid4: ver.uuid4,
                    fullName: ver.full_name,
                    versionNumber: ver.version_number,
                    iconUrl: ver.icon,
                    enabled: true,
                    pending_sync: true,
                });

                const depsToResolve: ReturnType<typeof parsePackageReference>[] = [];
                for (const depString of ver.dependencies) {
                    const dependency = parsePackageReference(depString);
                    if (!dependency.version) throw new Error(`Dependency ${depString} has no pinned version`);
                    if (profileSatisfiesPackage(profileIdToUse, dependency.packageName, dependency.version)) continue;
                    const plannedVersion = processedVersions.get(getPackageKey(dependency.packageName).toLowerCase());
                    if (plannedVersion && compareVersions(plannedVersion, dependency.version) >= 0) continue;
                    depsToResolve.push(dependency);
                }

                if (depsToResolve.length === 0) return;
                if (!selectedCommunity) {
                    throw new Error('Cannot resolve pinned dependencies without a Thunderstore community');
                }

                const result = await window.ipcRenderer.lookupPackagesByNames(selectedCommunity, depsToResolve.map(dep => dep.fullName));
                const dependencyTasks = depsToResolve.map(requirement => async () => {
                    let depPkg = result.found.find(candidate =>
                        candidate.full_name.toLowerCase() === requirement.packageName.toLowerCase()
                    );
                    let depVer = depPkg?.versions.find(candidate => candidate.version_number === requirement.version);
                    if (!depPkg || !depVer) {
                        const exactPackage = await window.ipcRenderer.fetchPackageByName(requirement.fullName, selectedCommunity);
                        if (!exactPackage) throw new Error(`Pinned dependency unavailable: ${requirement.fullName}`);
                        depPkg = exactPackage;
                        depVer = findPinnedVersion(depPkg, requirement.version!, requirement.packageName);
                    }
                    await collectModAndDeps(depPkg, depVer);
                });

                const concurrency = installInParallel ? MAX_PARALLEL_OPS : 1;
                if (dependencyTasks.length > 0) {
                    await runWithConcurrency(dependencyTasks, concurrency);
                }
            };

            await collectModAndDeps(pkg, version, true);
            for (const mod of modsToAdd.values()) addMod(profileIdToUse, mod);
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

    const stageProfileUpdates = async (
        updates: ProfileModUpdate[],
        targetProfileId?: string,
    ): Promise<number> => {
        const profileIdToUse = targetProfileId || activeProfileId;
        if (!profileIdToUse) { alert('Please select a profile first'); return 0; }
        if (updates.length === 0) return 0;

        const profile = useProfileStore.getState().profiles.find(candidate => candidate.id === profileIdToUse);
        if (!profile) throw new Error('Profile not found');
        const community = profile.gameIdentifier || selectedCommunity;
        if (!community) throw new Error('Cannot resolve updates without a Thunderstore community');
        const currentByKey = new Map(profile.mods.map(mod => [getPackageKey(mod.fullName).toLowerCase(), mod]));
        const planned = new Map<string, InstalledMod>();
        const processedVersions = new Map<string, string>();

        const collect = async (version: PackageVersion, isRoot: boolean) => {
            const key = getPackageKey(version.full_name).toLowerCase();
            const current = currentByKey.get(key);
            if (!isRoot && current && compareVersions(current.versionNumber, version.version_number) >= 0) return;
            const processed = processedVersions.get(key);
            if (processed && compareVersions(processed, version.version_number) >= 0) return;
            processedVersions.set(key, version.version_number);
            planned.set(key, {
                ...current,
                uuid4: version.uuid4,
                fullName: version.full_name,
                versionNumber: version.version_number,
                iconUrl: version.icon,
                enabled: current?.enabled ?? true,
                pending_sync: true,
                synced_enabled: current?.synced_enabled ?? current?.enabled,
            });

            const parsedRequirements = version.dependencies.map(parsePackageReference);
            const invalidRequirement = parsedRequirements.find(requirement => !requirement.version);
            if (invalidRequirement) {
                throw new Error(`Dependency ${invalidRequirement.fullName} has no pinned version`);
            }
            const requirements = parsedRequirements
                .filter(requirement => {
                    const installed = currentByKey.get(requirement.packageName.toLowerCase());
                    if (installed && compareVersions(installed.versionNumber, requirement.version!) >= 0) return false;
                    const queued = processedVersions.get(requirement.packageName.toLowerCase());
                    return !queued || compareVersions(queued, requirement.version!) < 0;
                });
            if (requirements.length === 0) return;

            const lookup = await window.ipcRenderer.lookupPackagesByNames(
                community,
                requirements.map(requirement => requirement.fullName)
            );
            const tasks = requirements.map(requirement => async () => {
                let dependencyPackage = lookup.found.find((candidate: Package) =>
                    candidate.full_name.toLowerCase() === requirement.packageName.toLowerCase()
                );
                let dependencyVersion = dependencyPackage?.versions.find((candidate: PackageVersion) =>
                    candidate.version_number === requirement.version
                );
                if (!dependencyPackage || !dependencyVersion) {
                    dependencyPackage = await window.ipcRenderer.fetchPackageByName(requirement.fullName, community) || undefined;
                    if (!dependencyPackage) throw new Error(`Pinned dependency unavailable: ${requirement.fullName}`);
                    dependencyVersion = findPinnedVersion(dependencyPackage, requirement.version!, requirement.packageName);
                }
                await collect(dependencyVersion, false);
            });
            await runWithConcurrency(tasks, installInParallel ? MAX_PARALLEL_OPS : 1);
        };

        await runWithConcurrency(
            updates.map(update => () => collect(update.version, true)),
            installInParallel ? MAX_PARALLEL_OPS : 1
        );

        const plannedKeys = new Set(planned.keys());
        const nextMods = profile.mods
            .filter(mod => !plannedKeys.has(getPackageKey(mod.fullName).toLowerCase()))
            .concat(Array.from(planned.values()));
        updateProfile(profileIdToUse, { mods: nextMods, needs_sync: true });
        return updates.length;
    };

    // ── Update mod ───────────────────────────────────────────────────────────────
    const handleUpdateMod = async (
        pkg: Package,
        targetProfileId?: string,
        selectedVersion?: PackageVersion
    ): Promise<void> => {
        const profileIdToUse = targetProfileId || activeProfileId;
        if (!profileIdToUse) { alert('Please select a profile first'); return; }
        const profile = useProfileStore.getState().profiles.find(candidate => candidate.id === profileIdToUse);
        const mod = profile?.mods.find(candidate => getPackageKey(candidate.fullName).toLowerCase() === pkg.full_name.toLowerCase());
        if (!mod) throw new Error(`${pkg.name} is not installed in this profile`);
        const targetVersion = selectedVersion || pkg.versions.reduce((latest, candidate) =>
            compareVersions(candidate.version_number, latest.version_number) > 0 ? candidate : latest
        );
        await stageProfileUpdates([{ mod, pkg, version: targetVersion }], profileIdToUse);
    };

    return {
        installModWithDependencies,
        handleInstallMod,
        handleUninstallWithDependencies,
        executeUninstall,
        handleUpdateMod,
        stageProfileUpdates,
    };
}
