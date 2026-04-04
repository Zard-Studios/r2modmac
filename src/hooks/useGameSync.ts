import { listen } from '@tauri-apps/api/event';
import type { Package } from '../types/thunderstore';
import { useProfileStore } from '../store/useProfileStore';
import type { ModDownloadProgressEvent, ProgressSetter } from '../types/progress';

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

    const handleSyncToGame = async (
        isVanillaOverride?: boolean,
        syncOptions?: SyncToGameOptions
    ) => {
        const activeProfile = useProfileStore.getState().profiles.find(p => p.id === activeProfileId);
        const community = activeProfile?.gameIdentifier || selectedCommunity;
        if (!activeProfile || !community) return;
        const silentSuccess = !!syncOptions?.silentSuccess;

        try {
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

            // ── BepInEx auto-install ───────────────────────────────────────────────
            const isBalatro = community === 'balatro';
            const hasLoaderInstalled = isBalatro
                ? activeProfile.mods.some(m => m.fullName.toLowerCase().includes('-lovely-'))
                : activeProfile.mods.some(m => m.fullName.toLowerCase().includes('bepinexpack'));
            if (!hasLoaderInstalled) {
                const requirementQuery = isBalatro ? 'lovely' : 'BepInExPack';
                setProgressState({
                    isOpen: true,
                    title: 'Checking Requirements',
                    progress: 0,
                    currentTask: `Searching for ${isBalatro ? 'Lovely' : 'BepInExPack'}...`
                });
                const packages = await window.ipcRenderer.getPackages(community, 0, 20, requirementQuery, 'downloads');
                const loaderPkg = Array.isArray(packages)
                    ? packages.find((p: Package) => isBalatro
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

            await persistProfilesNow();

            // ── Profile sync ──────────────────────────────────────────────────────
            const syncResult = await window.ipcRenderer.syncProfileToGame(activeProfile.id, community, legacyInstallMode);

            const skippedVersionMismatch: string[] = [];
            const failedInstalls: string[] = [];
            let actuallyInstalled = 0;
            const hasSyncWork = syncResult.removed > 0 || syncResult.to_install.length > 0 || (syncResult.cached ?? 0) > 0;

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
                });

                let completed = 0;
                const total = syncResult.to_install.length;
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
                    try {
                    const modInProfile = activeProfile.mods.find(m => {
                        if (!m.enabled) return false;
                        const parts = m.fullName.split('-');
                        const key = parts.length >= 2 ? `${parts[0]}-${parts[1]}` : m.fullName;
                        return key.toLowerCase() === modKey.toLowerCase();
                    });

                    if (modInProfile) {
                        if (legacyInstallMode) {
                            const cacheResult = await window.ipcRenderer.copyModFromCache(activeProfile.id, modInProfile.fullName, gamePath);
                            if (cacheResult.copied) {
                                actuallyInstalled++;
                                status = 'Copied from cache';
                                return;
                            }
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
                            await window.ipcRenderer.installMod(activeProfile.id, version.download_url, version.full_name, gamePath, legacyInstallMode);
                            actuallyInstalled++;
                            status = 'Installed';
                        }
                    }

                    } catch (err: any) {
                        failedInstalls.push(`${modKey} (${String(err?.message || err || 'unknown error')})`);
                        status = 'Failed';
                    } finally {
                        if (trackedFullName) {
                            activeDownloads.delete(trackedFullName);
                            recomputeDownloadState();
                        }
                        completed++;
                        updateProgress(`${status} ${completed}/${total}: ${modKey}`);
                    }
                };

                try {
                    for (let i = 0; i < syncResult.to_install.length; i += concurrency) {
                        const batch = syncResult.to_install.slice(i, i + concurrency);
                        if (concurrency === 1) {
                            await processMod(batch[0]);
                        } else {
                            await Promise.all(batch.map((modKey) => processMod(modKey)));
                        }
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
                }));
            }

            const latestProfile = useProfileStore.getState().profiles.find((p) => p.id === activeProfile.id) || activeProfile;
            const disabledMods = latestProfile.mods.filter((m) => !m.enabled).map((m) => m.fullName);

            if (hasSyncWork) {
                setProgressState({
                    isOpen: true,
                    title: 'Syncing to Game',
                    progress: 100,
                    currentTask: community === 'balatro'
                        ? 'Finalizing Lovely runtime and Balatro Mods folder...'
                        : latestProfile.platform === 'mac'
                        ? 'Finalizing BepInEx and Steam launch options...'
                        : 'Applying profile files to game...'
                });
            }

            await window.ipcRenderer.installToGame(community, latestProfile.id, disabledMods);

            if (hasSyncWork) {
                setProgressState(prev => ({ ...prev, isOpen: false }));
            }

            updateProfile(latestProfile.id, {
                needs_sync: false,
                mods: latestProfile.mods.map((m) => ({
                    ...m,
                    pending_sync: false,
                    synced_enabled: m.enabled,
                })),
            });

            // ── Success message ────────────────────────────────────────────────────
            const { removed, to_install: toInstall, cached = 0 } = syncResult;
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
            const isCrossOverProfile = typeof gamePath === 'string' && gamePath.toLowerCase().includes('crossover');
            if (syncedProfile?.platform !== 'mac' && isCrossOverProfile) {
                setShowCrossOverGuide(true);
            }

            } catch (e: any) {
            console.error('Sync to game failed:', e);
            setProgressState(prev => ({ ...prev, isOpen: false }));
            alert('Error syncing: ' + e);
        }
    };

    return { handleSyncToGame };
}
