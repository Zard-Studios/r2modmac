import { listen } from '@tauri-apps/api/event';
import type { Package } from '../types/thunderstore';
import type { InstalledMod } from '../types/profile';
import { useProfileStore } from '../store/useProfileStore';
import type { ModDownloadProgressEvent, ProgressSetter } from '../types/progress';
import { parsePackageReference } from '../utils/modVersioning';

const MAX_PARALLEL_OPS = 10;

interface UseGameSyncProps {
    activeProfileId: string | null;
    selectedCommunity: string | null;
    legacyInstallMode: boolean;
    installInParallel: boolean;
    setProgressState: ProgressSetter;
    setShowCrossOverGuide: (v: boolean) => void;
    installModWithDependencies: (
        pkg: Package,
        version: any,
        cache?: Set<string>,
        profileId?: string,
        counter?: { installed: number; total: number },
        gamePath?: string
    ) => Promise<void>;
}

interface SyncToGameOptions {
    silentSuccess?: boolean;
}

export function useGameSync({
    activeProfileId,
    selectedCommunity,
    legacyInstallMode,
    installInParallel,
    setProgressState,
    setShowCrossOverGuide,
    installModWithDependencies,
}: UseGameSyncProps) {
    const { updateProfile } = useProfileStore();

    const persistProfilesNow = async () => {
        const latestProfiles = useProfileStore.getState().profiles;
        await window.ipcRenderer.saveProfiles(latestProfiles);
        return latestProfiles;
    };

    const refreshLocalModsBeforeSync = async (profile: any) => {
        const localMods = profile.mods.filter((mod: any) => mod.source === 'local' && mod.localId);
        if (localMods.length === 0) return profile;

        let latestProfile = profile;
        let changed = false;
        for (const mod of localMods) {
            try {
                const result = await window.ipcRenderer.refreshLocalModMetadata(
                    profile.id,
                    mod.localId,
                    mod.sourcePath,
                    mod.enabled
                );
                if (!result?.mod) continue;

                const refreshedMod = {
                    ...mod,
                    ...result.mod,
                    uuid4: mod.uuid4,
                    enabled: mod.enabled,
                    synced_enabled: mod.synced_enabled,
                    pending_sync: !!mod.pending_sync || !!result.changed,
                };
                latestProfile = {
                    ...latestProfile,
                    mods: latestProfile.mods.map((candidate: any) =>
                        candidate.uuid4 === mod.uuid4 ? refreshedMod : candidate
                    ),
                    needs_sync: !!latestProfile.needs_sync || !!result.changed,
                };
                changed = changed || !!result.changed;
            } catch (err) {
                console.error(`Failed to refresh custom mod ${mod.fullName}`, err);
            }
        }

        if (changed) {
            updateProfile(profile.id, {
                mods: latestProfile.mods,
                needs_sync: latestProfile.needs_sync,
            });
            await persistProfilesNow();
        }

        return useProfileStore.getState().profiles.find((p) => p.id === profile.id) || latestProfile;
    };

    const handleSyncToGame = async (
        isVanillaOverride?: boolean,
        syncOptions?: SyncToGameOptions
    ) => {
        const activeProfile = useProfileStore.getState().profiles.find(p => p.id === activeProfileId);
        const community = activeProfile?.gameIdentifier || selectedCommunity;
        if (!activeProfile || !community) return;
        const silentSuccess = !!syncOptions?.silentSuccess;
        const ensureNotStopped = async () => {
            if (await window.ipcRenderer.modOperationsCancelled()) {
                throw new Error('MOD_OPERATION_CANCELLED');
            }
        };
        let applyTransactionStarted = false;

        try {
            await window.ipcRenderer.beginModOperations();
            // Vanilla override — direct call, no BepInEx setup needed
            if (isVanillaOverride !== undefined) {
                await persistProfilesNow();
                const disabledMods = activeProfile.mods.filter(m => !m.enabled).map(m => m.fullName);
                await window.ipcRenderer.installToGame(community, activeProfile.id, disabledMods, isVanillaOverride);
                return;
            }

            const gamePath = await window.ipcRenderer.getGamePath(community, activeProfile.platform);
            if (!gamePath) {
                await window.ipcRenderer.alert('Game Path Required', 'Please set the game directory in Settings first.');
                return;
            }
            updateProfile(activeProfile.id, { needs_sync: true, apply_interrupted: true });
            await persistProfilesNow();

            // ── Loader auto-install ───────────────────────────────────────────────
            const isBalatro = community === 'balatro';
            const isOuterWilds = community === 'outerwilds';
            const hasLoaderInstalled = isBalatro
                ? activeProfile.mods.some(m => m.fullName.toLowerCase().includes('-lovely-'))
                : isOuterWilds
                ? activeProfile.mods.some(m => m.fullName.toLowerCase().includes('owml'))
                : activeProfile.mods.some(m => m.fullName.toLowerCase().includes('bepinexpack'));
            if (!hasLoaderInstalled) {
                const requirementQuery = isOuterWilds ? 'OWML' : isBalatro ? 'lovely' : 'BepInExPack';
                setProgressState({
                    isOpen: true,
                    title: 'Checking Requirements',
                    progress: 0,
                    currentTask: `Searching for ${isOuterWilds ? 'OWML' : isBalatro ? 'Lovely' : 'BepInExPack'}...`,
                    isCancelable: true,
                    operation: 'mod-sync',
                });
                const res = await window.ipcRenderer.getPackages(community, 0, 20, requirementQuery, 'downloads');
                const packagesList = res && typeof res === 'object' && 'items' in res ? res.items : (Array.isArray(res) ? res : []);
                
                const loaderPkg = Array.isArray(packagesList)
                    ? packagesList.find((p: Package) => isOuterWilds
                        ? p.name.toLowerCase() === 'owml'
                        : isBalatro
                        ? p.full_name?.toLowerCase().includes('thunderstore-lovely') || p.name.toLowerCase() === 'lovely'
                        : p.name.toLowerCase().includes('bepinexpack'))
                    : null;

                if (loaderPkg) {
                    const version = loaderPkg.versions[0];
                    setProgressState(prev => ({ ...prev, progress: 20, currentTask: `Installing missing requirement: ${loaderPkg.name}...` }));
                    await installModWithDependencies(loaderPkg, version, new Set(), activeProfile.id, undefined, gamePath);
                }
                setProgressState(prev => ({ ...prev, isOpen: false }));
            }
            await ensureNotStopped();

            const refreshedProfile = await refreshLocalModsBeforeSync(activeProfile);
            await persistProfilesNow();
            await ensureNotStopped();

            // ── Profile sync ──────────────────────────────────────────────────────
            const syncResult = await window.ipcRenderer.syncProfileToGame(refreshedProfile.id, community, legacyInstallMode, false);
            const missingKeys = new Set(syncResult.to_install.map((key: string) => key.toLowerCase()));
            const reconciledProfile = useProfileStore.getState().profiles.find(profile => profile.id === activeProfile.id) || refreshedProfile;
            updateProfile(activeProfile.id, {
                // Keep the apply resumable until final runtime setup succeeds.
                needs_sync: true,
                apply_interrupted: true,
                mods: reconciledProfile.mods.map((mod: InstalledMod) => {
                    const key = parsePackageReference(mod.fullName).packageName.toLowerCase();
                    const stillMissing = mod.enabled && missingKeys.has(key);
                    return {
                        ...mod,
                        pending_sync: stillMissing,
                        synced_enabled: stillMissing ? mod.synced_enabled : mod.enabled,
                    };
                }),
            });
            await persistProfilesNow();

            const skippedVersionMismatch: string[] = [];
            const failedInstalls: string[] = [];
            let actuallyInstalled = 0;
            const hasSyncWork = (syncResult.pending_removals ?? 0) > 0 || syncResult.to_install.length > 0;
            if (hasSyncWork) {
                await window.ipcRenderer.beginProfileApplyTransaction(activeProfile.id, community);
                applyTransactionStarted = true;
            }

            if (syncResult.to_install.length > 0) {
                const concurrency = installInParallel ? MAX_PARALLEL_OPS : 1;
                setProgressState({
                    isOpen: true,
                    title: 'Syncing to Game',
                    progress: 0,
                    currentTask: `Installing ${syncResult.to_install.length} missing mods...`,
                    downloadSpeedBps: undefined,
                    downloadedBytes: undefined,
                    totalBytes: undefined,
                    activeDownloads: 0,
                    isCancelable: true,
                    operation: 'mod-sync',
                });

                let completed = 0;
                const total = syncResult.to_install.length;
                const successfullyInstalledKeys = new Set<string>();
                const trackedModKeys = syncResult.to_install.map((modKey: string) => modKey.toLowerCase());
                const activeDownloads = new Map<string, { downloaded: number; total?: number; speed: number; progress: number }>();
                const updateProgress = (task: string) => {
                    setProgressState(prev => ({
                        ...prev,
                        progress: Math.round((completed / total) * 100),
                        currentTask: task,
                    }));
                };

                const recomputeDownloadState = () => {
                    const inFlight = Array.from(activeDownloads.values());
                    if (inFlight.length === 0) {
                        setProgressState(prev => ({
                            ...prev,
                            downloadSpeedBps: undefined,
                            downloadedBytes: undefined,
                            totalBytes: undefined,
                            activeDownloads: 0,
                        }));
                        return;
                    }

                    const partialUnits = inFlight.reduce((sum, item) => sum + (Math.min(100, Math.max(0, item.progress)) / 100), 0);
                    const overallProgress = Math.min(
                        99,
                        Math.round(((completed + partialUnits) / Math.max(total, 1)) * 100)
                    );

                    const downloadedBytes = inFlight.reduce((sum, item) => sum + item.downloaded, 0);
                    const knownTotals = inFlight.filter(item => typeof item.total === 'number' && (item.total ?? 0) > 0);
                    const totalBytes = knownTotals.length === inFlight.length
                        ? knownTotals.reduce((sum, item) => sum + (item.total ?? 0), 0)
                        : undefined;
                    const totalSpeed = inFlight.reduce((sum, item) => sum + item.speed, 0);

                    setProgressState(prev => ({
                        ...prev,
                        progress: Math.max(prev.progress, overallProgress),
                        downloadedBytes,
                        totalBytes,
                        downloadSpeedBps: totalSpeed,
                        activeDownloads: inFlight.length,
                    }));
                };

                const unlistenDownloadProgress = await listen<ModDownloadProgressEvent>('mod-download-progress', (event) => {
                    const payload = event.payload;
                    if (!payload || !payload.mod_name) return;

                    const modNameLower = payload.mod_name.toLowerCase();
                    const isTracked = trackedModKeys.some((modKey) => modNameLower.startsWith(modKey));
                    if (!isTracked) return;

                    if (payload.done) {
                        activeDownloads.delete(payload.mod_name);
                    } else {
                        activeDownloads.set(payload.mod_name, {
                            downloaded: Math.max(0, payload.downloaded_bytes || 0),
                            total: payload.total_bytes && payload.total_bytes > 0 ? payload.total_bytes : undefined,
                            speed: Math.max(0, payload.speed_bps || 0),
                            progress: Math.min(100, Math.max(0, payload.progress_percent || 0)),
                        });
                    }

                    recomputeDownloadState();
                });

                const processMod = async (modKey: string) => {
                    let status = 'Installed';
                    let trackedFullName: string | null = null;
                    let installedSuccessfully = false;
                    try {
                        const profileForLookup = useProfileStore.getState().profiles.find((p) => p.id === activeProfile.id) || refreshedProfile;
                        const modInProfile = profileForLookup.mods.find((m: any) => {
                            if (!m.enabled) return false;
                            const parts = m.fullName.split('-');
                            const key = parts.length >= 2 ? `${parts[0]}-${parts[1]}` : m.fullName;
                            return key.toLowerCase() === modKey.toLowerCase();
                        });

                    if (modInProfile) {
                        if (legacyInstallMode || community === 'outerwilds') {
                            const cacheResult = await window.ipcRenderer.copyModFromCache(activeProfile.id, modInProfile.fullName, gamePath);
                            if (cacheResult.copied) {
                                actuallyInstalled++;
                                status = 'Copied from cache';
                                installedSuccessfully = true;
                                return;
                            }
                        }

                        if (modInProfile.source === 'local') {
                            if (!modInProfile.localId) {
                                skippedVersionMismatch.push(`${modKey} (missing local payload id)`);
                                status = 'Skipped (local payload missing)';
                                return;
                            }
                            trackedFullName = modInProfile.fullName;
                            const installResult = await window.ipcRenderer.installLocalMod(
                                activeProfile.id,
                                modInProfile.localId,
                                modInProfile.fullName,
                                gamePath,
                                legacyInstallMode
                            );
                            if (!installResult.success) {
                                throw new Error(installResult.error || 'Local mod install failed');
                            }
                            actuallyInstalled++;
                            status = 'Installed local mod';
                            installedSuccessfully = true;
                            return;
                        }

                        const pkg = await window.ipcRenderer.fetchPackageByName(modInProfile.fullName, community);
                        if (pkg) {
                            const version = pkg.versions.find((v: any) => v.version_number === modInProfile.versionNumber);
                            if (!version) {
                                skippedVersionMismatch.push(`${modKey} (requested v${modInProfile.versionNumber})`);
                                status = 'Skipped (version not found)';
                                return;
                            }
                            trackedFullName = version.full_name;

                            const installModAndDeps = async (currentPkg: any, currentVer: any) => {
                                const result = await window.ipcRenderer.installMod(
                                    activeProfile.id,
                                    currentVer.download_url,
                                    currentVer.full_name,
                                    gamePath,
                                    legacyInstallMode
                                );
                                if (!result.success) {
                                    throw new Error(result.error || `Failed to install ${currentPkg.name}`);
                                }
                                actuallyInstalled++;

                                if (community === 'outerwilds' && Array.isArray(result.dependencies) && result.dependencies.length > 0) {
                                    for (const depUniqueName of result.dependencies) {
                                        if (depUniqueName.toLowerCase() === 'alek.owml' || depUniqueName.toLowerCase() === 'owml') continue;

                                        const depName = depUniqueName.replace('.', '-'); // normalized to hyphen
                                        const activeProfileState = useProfileStore.getState().profiles.find(p => p.id === activeProfile.id);
                                        const isAlreadyAdded = activeProfileState?.mods.some(m => {
                                            const parts = m.fullName.split('-');
                                            const matchKey = parts.length >= 2 ? `${parts[0]}-${parts[1]}` : m.fullName;
                                            return matchKey.replace('.', '-').toLowerCase() === depName.toLowerCase();
                                        });

                                        if (isAlreadyAdded) continue;

                                        // Fetch dependency package info
                                        const depPkg = await window.ipcRenderer.fetchPackageByName(depUniqueName, 'outerwilds');
                                        if (depPkg) {
                                            const depVersion = depPkg.versions[0];
                                            if (depVersion) {
                                                // Add to profile
                                                useProfileStore.getState().addMod(activeProfile.id, {
                                                    uuid4: depVersion.uuid4,
                                                    fullName: depVersion.full_name,
                                                    versionNumber: depVersion.version_number,
                                                    iconUrl: depVersion.icon,
                                                    enabled: true,
                                                    pending_sync: false,
                                                    synced_enabled: true
                                                });

                                                // Recursively install dependency
                                                await installModAndDeps(depPkg, depVersion);
                                            }
                                        }
                                    }
                                }
                            };

                            await installModAndDeps(pkg, version);
                            status = 'Installed';
                            installedSuccessfully = true;
                        } else {
                            throw new Error(`Package metadata not found for pinned mod ${modInProfile.fullName}`);
                        }
                    } else {
                        throw new Error(`Pinned profile entry not found for ${modKey}`);
                    }

                    } catch (err: any) {
                        if (String(err?.message || err).includes('MOD_OPERATION_CANCELLED')) {
                            throw err;
                        }
                        failedInstalls.push(`${modKey} (${String(err?.message || err || 'unknown error')})`);
                        status = 'Failed';
                    } finally {
                        if (trackedFullName) {
                            activeDownloads.delete(trackedFullName);
                            recomputeDownloadState();
                        }
                        completed++;
                        if (installedSuccessfully) successfullyInstalledKeys.add(modKey.toLowerCase());
                        updateProgress(`${status} ${completed}/${total}: ${modKey}`);
                    }
                };

                const flushSuccessfullyInstalledMods = async () => {
                    if (successfullyInstalledKeys.size === 0) return;
                    const currentProfile = useProfileStore.getState().profiles.find((profile) => profile.id === activeProfile.id);
                    if (!currentProfile) return;
                    const completedKeys = new Set(successfullyInstalledKeys);
                    successfullyInstalledKeys.clear();
                    updateProfile(activeProfile.id, {
                        needs_sync: true,
                        apply_interrupted: true,
                        mods: currentProfile.mods.map((mod) => {
                            const parts = mod.fullName.split('-');
                            const key = (parts.length >= 2 ? `${parts[0]}-${parts[1]}` : mod.fullName).toLowerCase();
                            return completedKeys.has(key)
                                ? { ...mod, pending_sync: false, synced_enabled: mod.enabled }
                                : mod;
                        }),
                    });
                    await persistProfilesNow();
                };

                try {
                    for (let i = 0; i < syncResult.to_install.length; i += concurrency) {
                        await ensureNotStopped();
                        const batch = syncResult.to_install.slice(i, i + concurrency);
                        if (concurrency === 1) {
                            await processMod(batch[0]);
                        } else {
                            const results = await Promise.allSettled(batch.map((modKey) => processMod(modKey)));
                            await flushSuccessfullyInstalledMods();
                            const rejected = results.find((result): result is PromiseRejectedResult => result.status === 'rejected');
                            if (rejected) throw rejected.reason;
                        }
                        if (concurrency === 1) await flushSuccessfullyInstalledMods();
                    }
                } finally {
                    unlistenDownloadProgress();
                }
                setProgressState(prev => ({
                    ...prev,
                    isOpen: false,
                    downloadSpeedBps: undefined,
                    downloadedBytes: undefined,
                    totalBytes: undefined,
                    activeDownloads: 0,
                    isCancelable: false,
                    operation: undefined,
                }));
            }

            if (skippedVersionMismatch.length > 0 || failedInstalls.length > 0) {
                const problems = [...skippedVersionMismatch, ...failedInstalls];
                const preview = problems.slice(0, 8).join('\n');
                const more = problems.length > 8 ? `\n...and ${problems.length - 8} more` : '';
                throw new Error(
                    `Sync incomplete: ${problems.length} pinned mod(s) were not installed. ` +
                    `The profile remains pending and the game was not finalized.\n\n${preview}${more}`
                );
            }
            await ensureNotStopped();

            // Cleanup is deliberately deferred until every requested payload is
            // present. A failed download therefore leaves the previous working
            // version untouched and the profile resumable.
            const finalizeResult = await window.ipcRenderer.syncProfileToGame(
                refreshedProfile.id,
                community,
                legacyInstallMode,
                true
            );

            const latestProfile = useProfileStore.getState().profiles.find((p) => p.id === activeProfile.id) || activeProfile;
            const disabledMods = latestProfile.mods.filter((m) => !m.enabled).map((m) => m.fullName);

            if (hasSyncWork) {
                setProgressState({
                    isOpen: true,
                    title: 'Syncing to Game',
                    progress: 100,
                    isCancelable: false,
                    operation: undefined,
                    currentTask: community === 'balatro'
                        ? 'Finalizing Lovely runtime and Balatro Mods folder...'
                        : latestProfile.platform === 'mac'
                        ? 'Finalizing BepInEx and Steam launch options...'
                        : 'Applying profile files to game...'
                });
            }

            await window.ipcRenderer.installToGame(community, latestProfile.id, disabledMods);

            if (applyTransactionStarted) {
                try {
                    await window.ipcRenderer.commitProfileApplyTransaction(activeProfile.id);
                } catch (cleanupError) {
                    console.warn('Failed to remove completed Apply snapshot', cleanupError);
                }
                applyTransactionStarted = false;
            }

            if (hasSyncWork) {
                setProgressState(prev => ({ ...prev, isOpen: false }));
            }

            updateProfile(latestProfile.id, {
                needs_sync: false,
                apply_interrupted: false,
                mods: latestProfile.mods.map((m) => ({
                    ...m,
                    pending_sync: false,
                    synced_enabled: m.enabled,
                })),
            });

            // ── Success message ────────────────────────────────────────────────────
            const removed = finalizeResult.removed ?? 0;
            const toInstall = syncResult.to_install;
            const cached = finalizeResult.cached ?? 0;
            let message: string;
            if (removed === 0 && toInstall.length === 0 && cached === 0) {
                message = 'Profile already synced! No changes needed.';
            } else {
                const parts: string[] = [];
                if (removed > 0) parts.push(`${removed} removed`);
                if (toInstall.length > 0) parts.push(`${actuallyInstalled} installed`);
                if (cached > 0) parts.push(`${cached} cached`);
                message = `Sync complete! ${parts.join(', ')}.`;
            }

            // Extra safety notice: never auto-upgrade to latest when a pinned version is missing.
            // We skip instead, so users can explicitly choose a new version.
            if (skippedVersionMismatch.length > 0) {
                const preview = skippedVersionMismatch.slice(0, 5).join('\n');
                const more = skippedVersionMismatch.length > 5 ? `\n...and ${skippedVersionMismatch.length - 5} more` : '';
                message += `\n\nSkipped ${skippedVersionMismatch.length} mod(s) because the exact pinned version is unavailable:\n${preview}${more}`;
            }
            if (failedInstalls.length > 0) {
                const preview = failedInstalls.slice(0, 5).join('\n');
                const more = failedInstalls.length > 5 ? `\n...and ${failedInstalls.length - 5} more` : '';
                message += `\n\nFailed ${failedInstalls.length} mod(s) during sync:\n${preview}${more}`;
            }

            if (community === 'balatro' && message !== 'Profile already synced! No changes needed.') {
                message += '\n\nBalatro macOS: mod files are synced to ~/Library/Application Support/Balatro/Mods. Launch the modded game with run_lovely_macos.sh.';
            }

            if (!silentSuccess) {
                await window.ipcRenderer.alert('Success', message);
            }

            const syncedProfile = useProfileStore.getState().profiles.find(p => p.id === activeProfileId);
            if (syncedProfile?.platform !== 'mac' && gamePath) {
                setShowCrossOverGuide(true);
            }

            } catch (e: any) {
            console.error('Sync to game failed:', e);
            setProgressState(prev => ({ ...prev, isOpen: false }));
            if (applyTransactionStarted) {
                try {
                    await window.ipcRenderer.rollbackProfileApplyTransaction(activeProfile.id, community);
                } catch (rollbackError) {
                    console.error('Failed to roll back interrupted Apply', rollbackError);
                }
            }
            if (String(e?.message || e).includes('MOD_OPERATION_CANCELLED')) {
                await window.ipcRenderer.beginModOperations();
                return;
            }
            await window.ipcRenderer.alert('Sync Failed', String(e?.message || e || 'Unknown sync error'));
        }
    };

    return { handleSyncToGame };
}
