import type { Package } from '../types/thunderstore';
import { useProfileStore } from '../store/useProfileStore';

const MAX_PARALLEL_OPS = 10;

interface ProgressSetter {
    (state: { isOpen: boolean; title: string; progress: number; currentTask: string }): void;
    (updater: (prev: { isOpen: boolean; title: string; progress: number; currentTask: string }) => { isOpen: boolean; title: string; progress: number; currentTask: string }): void;
}

interface UseGameSyncProps {
    activeProfileId: string | null;
    selectedCommunity: string | null;
    legacyInstallMode: boolean;
    installInParallel: boolean;
    hideMacOSGuide: boolean;
    setProgressState: ProgressSetter;
    setShowCrossOverGuide: (v: boolean) => void;
    setShowMacOSGuide: (v: boolean) => void;
    installModWithDependencies: (
        pkg: Package,
        version: any,
        cache?: Set<string>,
        profileId?: string,
        counter?: { installed: number; total: number },
        gamePath?: string
    ) => Promise<void>;
}

export function useGameSync({
    activeProfileId,
    selectedCommunity,
    legacyInstallMode,
    installInParallel,
    hideMacOSGuide,
    setProgressState,
    setShowCrossOverGuide,
    setShowMacOSGuide,
    installModWithDependencies,
}: UseGameSyncProps) {
    const { profiles, updateProfile } = useProfileStore();

    const handleSyncToGame = async (isVanillaOverride?: boolean) => {
        const activeProfile = profiles.find(p => p.id === activeProfileId);
        const community = selectedCommunity;
        if (!activeProfile || !community) return;

        try {
            // Vanilla override — direct call, no BepInEx setup needed
            if (isVanillaOverride !== undefined) {
                const disabledMods = activeProfile.mods.filter(m => !m.enabled).map(m => m.fullName);
                await window.ipcRenderer.installToGame(community, activeProfile.id, disabledMods, isVanillaOverride);
                updateProfile(activeProfile.id, {
                    needs_sync: false,
                    mods: activeProfile.mods.map((m) => ({
                        ...m,
                        pending_sync: false,
                        synced_enabled: m.enabled,
                    })),
                });
                return;
            }

            const gamePath = await window.ipcRenderer.getGamePath(community, activeProfile.platform);
            if (!gamePath) {
                await window.ipcRenderer.alert('Game Path Required', 'Please set the game directory in Settings first.');
                return;
            }

            // ── BepInEx auto-install ───────────────────────────────────────────────
            const isBepInExInstalled = activeProfile.mods.some(m => m.fullName.toLowerCase().includes('bepinexpack'));
            if (!isBepInExInstalled) {
                setProgressState({ isOpen: true, title: 'Checking Requirements', progress: 0, currentTask: 'Searching for BepInExPack...' });
                const packages = await window.ipcRenderer.getPackages(community, 0, 20, 'BepInExPack', 'downloads');
                const bepInExPkg = Array.isArray(packages) ? packages.find((p: Package) => p.name.toLowerCase().includes('bepinexpack')) : null;

                if (bepInExPkg) {
                    const version = bepInExPkg.versions[0];
                    setProgressState(prev => ({ ...prev, progress: 20, currentTask: `Installing missing requirement: ${bepInExPkg.name}...` }));
                    await installModWithDependencies(bepInExPkg, version, new Set(), activeProfile.id, undefined, gamePath);
                }
                setProgressState(prev => ({ ...prev, isOpen: false }));
            }

            // ── Profile sync ──────────────────────────────────────────────────────
            const syncResult = await window.ipcRenderer.syncProfileToGame(activeProfile.id, community, legacyInstallMode);

            const skippedVersionMismatch: string[] = [];
            const failedInstalls: string[] = [];
            let actuallyInstalled = 0;

            if (syncResult.to_install.length > 0) {
                const concurrency = installInParallel ? MAX_PARALLEL_OPS : 1;
                setProgressState({
                    isOpen: true,
                    title: 'Syncing to Game',
                    progress: 0,
                    currentTask: `Installing ${syncResult.to_install.length} missing mods...`,
                });

                let completed = 0;
                const total = syncResult.to_install.length;
                const updateProgress = (task: string) => {
                    setProgressState(prev => ({
                        ...prev,
                        progress: Math.round((completed / total) * 100),
                        currentTask: task,
                    }));
                };

                const processMod = async (modKey: string) => {
                    let status = 'Installed';
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
                            await window.ipcRenderer.installMod(activeProfile.id, version.download_url, version.full_name, gamePath, legacyInstallMode);
                            actuallyInstalled++;
                            status = 'Installed';
                        }
                    }

                    } catch (err: any) {
                        failedInstalls.push(`${modKey} (${String(err?.message || err || 'unknown error')})`);
                        status = 'Failed';
                    } finally {
                        completed++;
                        updateProgress(`${status} ${completed}/${total}: ${modKey}`);
                    }
                };

                for (let i = 0; i < syncResult.to_install.length; i += concurrency) {
                    const batch = syncResult.to_install.slice(i, i + concurrency);
                    if (concurrency === 1) {
                        await processMod(batch[0]);
                    } else {
                        await Promise.all(batch.map((modKey) => processMod(modKey)));
                    }
                }
                setProgressState(prev => ({ ...prev, isOpen: false }));
            }

            updateProfile(activeProfile.id, {
                needs_sync: false,
                mods: activeProfile.mods.map((m) => ({
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

            await window.ipcRenderer.alert('Success', message);

            const syncedProfile = profiles.find(p => p.id === activeProfileId);
            if (syncedProfile?.platform === 'mac') {
                if (!hideMacOSGuide) setShowMacOSGuide(true);
            } else {
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
