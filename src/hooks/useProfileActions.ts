import type { Package } from '../types/thunderstore';
import type { InstalledMod } from '../types/profile';
import type { ProgressSetter } from '../types/progress';
import { useProfileStore } from '../store/useProfileStore';
import { listen } from '@tauri-apps/api/event';

const MAX_PARALLEL_OPS = 10;

const getErrorMessage = (err: unknown, fallback: string) => {
    if (err instanceof Error && err.message) return err.message;
    if (typeof err === 'string' && err.trim()) return err;
    if (err && typeof err === 'object') {
        try {
            return JSON.stringify(err);
        } catch {
            return fallback;
        }
    }
    return fallback;
};

interface UseProfileActionsProps {
    selectedCommunity: string | null;
    activeProfileId: string | null;
    legacyInstallMode: boolean;
    installInParallel: boolean;
    setProgressState: ProgressSetter;
    onInstallMod: (pkg: Package, profileId: string) => Promise<void>;
    autoApplyProfileRef?: React.MutableRefObject<string | null>;
}

export function useProfileActions({
    selectedCommunity,
    activeProfileId,
    legacyInstallMode,
    installInParallel,
    setProgressState,
    onInstallMod,
    autoApplyProfileRef,
}: UseProfileActionsProps) {
    const { createProfile, addMod, updateProfile } = useProfileStore();

    const processImportResult = async (result: any, chosenPlatform: 'windows' | 'mac') => {
        if (result.type === 'profile') {
            let profileName: string = result.name;

            if (profileName.startsWith('Imported: ')) profileName = profileName.substring(10);

            const localMods = result.mods.filter((m: any) => m.source === 'local');
            const thunderstoreMods = result.mods.filter((m: any) => m.source !== 'local');
            const modNames = thunderstoreMods.map((m: any) => m.name);
            const lookup = modNames.length > 0
                ? await window.ipcRenderer.lookupPackagesByNames(selectedCommunity!, modNames)
                : { found: [], unknown: [] };

            if (lookup.unknown.length > 0) {
                const proceed = await window.ipcRenderer.confirm(
                    'Some mods cannot be found',
                    `${lookup.unknown.length} mod(s) from the profile were not found and will not be installed:\n\n${lookup.unknown.join('\n')}\n\n${lookup.found.length} mod(s) will be installed. Do you want to continue?`
                );
                if (!proceed) return;
            }

            const newProfileId = createProfile(profileName, selectedCommunity!, chosenPlatform);
            if (autoApplyProfileRef) {
                autoApplyProfileRef.current = newProfileId;
            }

            if (!legacyInstallMode) {
                // ── New Mode: Metadata-only import ──────────────────────────────────
                setProgressState({
                    isOpen: true,
                    title: 'Importing Profile',
                    progress: 0,
                    currentTask: 'Importing metadata...',
                });

                setTimeout(async () => {
                    const unknownLower = lookup.unknown.map((name: string) => name.toLowerCase());
                    const modsToAdd = thunderstoreMods.filter((m: any) => !unknownLower.includes(m.name.toLowerCase()));
                    let installedCount = 0;
                    let completedCount = 0;
                    const totalMods = modsToAdd.length + localMods.length;
                    const failedMods: string[] = [];

                    for (const mod of modsToAdd) {
                        try {
                            const pkg = lookup.found.find((p: Package) => p.full_name.toLowerCase() === mod.name.toLowerCase());
                            if (pkg) {
                                const version = pkg.versions.find((v: any) => v.version_number === mod.version) || pkg.versions[0];
                                const installedMod: InstalledMod = {
                                    uuid4: version.uuid4,
                                    fullName: version.full_name,
                                    versionNumber: version.version_number,
                                    iconUrl: version.icon,
                                    enabled: mod.enabled,
                                    pending_sync: true, // Crucial for new mode: mark as pending sync!
                                };
                                addMod(newProfileId, installedMod);
                                installedCount++;
                            }
                        } catch {
                            failedMods.push(mod.name);
                        } finally {
                            completedCount++;
                            setProgressState(prev => ({
                                ...prev,
                                progress: Math.round((completedCount / Math.max(totalMods, 1)) * 100),
                                currentTask: `Processed ${completedCount}/${totalMods}...`
                            }));
                        }
                    }

                    for (const mod of localMods) {
                        try {
                            const payloadPath = mod.payload;
                            const archivePath = result.archivePath;
                            if (!payloadPath || !archivePath) {
                                throw new Error('Embedded local payload is missing from this profile export.');
                            }
                            setProgressState(prev => ({
                                ...prev,
                                currentTask: `Staging custom mod ${mod.name} (${Math.min(completedCount + 1, totalMods)}/${totalMods})...`
                            }));
                            const imported = await window.ipcRenderer.importEmbeddedCustomMod(
                                newProfileId,
                                archivePath,
                                payloadPath,
                                {
                                    name: mod.displayName || mod.name?.split('-').slice(1).join('-') || mod.name,
                                    author: mod.author || mod.name?.split('-')[0] || 'Local',
                                    version: mod.version,
                                    enabled: mod.enabled,
                                    platforms: mod.platforms,
                                    expectedSha256: mod.sha256,
                                }
                            );
                            addMod(newProfileId, imported.mod);
                            installedCount++;
                        } catch (e) {
                            failedMods.push(mod.name);
                            console.error(`Error staging custom mod ${mod.name}`, e);
                        } finally {
                            completedCount++;
                            setProgressState(prev => ({
                                ...prev,
                                progress: Math.round((completedCount / Math.max(totalMods, 1)) * 100),
                                currentTask: `Processed ${completedCount}/${totalMods}...`
                            }));
                        }
                    }

                    updateProfile(newProfileId, { needs_sync: true });

                    setProgressState(prev => ({ ...prev, progress: 100, currentTask: 'Import Complete!' }));
                    setTimeout(() => {
                        setProgressState(prev => ({ ...prev, isOpen: false }));
                        let msg = `Imported profile "${result.name}" with ${installedCount}/${totalMods} mods.`;
                        if (failedMods.length > 0) msg += `\n\nFailed to import:\n${failedMods.join('\n')}`;
                        alert(msg);
                    }, 500);
                }, 500);

                return;
            }

            // ── Legacy Mode: Direct download & install ───────────────────────────
            const gamePath = await window.ipcRenderer.getGamePath(selectedCommunity || '', chosenPlatform);
            if (!gamePath) {
                await window.ipcRenderer.alert(
                    'Game Path Required',
                    'Please configure the game directory in Settings before importing profiles.\n\nGo to Settings → Game Directory and set the path to your game installation folder.'
                );
                return;
            }

            const concurrency = installInParallel ? MAX_PARALLEL_OPS : 1;
            setProgressState({
                isOpen: true,
                title: 'Importing Profile (Legacy)',
                progress: 0,
                currentTask: 'Starting import...',
                downloadSpeedBps: undefined,
                downloadedBytes: undefined,
                totalBytes: undefined,
                activeDownloads: 0,
            });

            setTimeout(async () => {
                const unknownLower = lookup.unknown.map((name: string) => name.toLowerCase());
                const modsToInstall = thunderstoreMods.filter((m: any) => !unknownLower.includes(m.name.toLowerCase()));
                let installedCount = 0;
                let completedCount = 0;
                const totalMods = modsToInstall.length + localMods.length;
                const failedMods: string[] = [];

                const trackedModKeys = modsToInstall.map((mod: any) => mod.name.toLowerCase());
                const activeDownloads = new Map<string, { downloaded: number; total?: number; speed: number; progress: number }>();

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
                        Math.round(((completedCount + partialUnits) / Math.max(totalMods, 1)) * 100)
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

                const unlistenDownloadProgress = await listen<any>('mod-download-progress', (event) => {
                    const payload = event.payload;
                    if (!payload || !payload.mod_name) return;

                    const modNameLower = payload.mod_name.toLowerCase();
                    const isTracked = trackedModKeys.some((modKey: string) => modNameLower.startsWith(modKey));
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

                const processMod = async (mod: any) => {
                    let trackedFullName: string | null = null;
                    try {
                        setProgressState(prev => ({
                            ...prev,
                            currentTask: `Installing ${mod.name} (${Math.min(completedCount + 1, totalMods)}/${totalMods})...`
                        }));

                        const pkg = lookup.found.find((p: Package) => p.full_name.toLowerCase() === mod.name.toLowerCase());
                        if (pkg) {
                            const version = pkg.versions.find((v: any) => v.version_number === mod.version) || pkg.versions[0];
                            trackedFullName = version.full_name;
                            const installResult = await window.ipcRenderer.installMod(
                                newProfileId, version.download_url, version.full_name, gamePath, legacyInstallMode
                            );
                            if (installResult.success) {
                                const installedMod: InstalledMod = {
                                    uuid4: version.uuid4, fullName: version.full_name,
                                    versionNumber: version.version_number, iconUrl: version.icon, enabled: mod.enabled
                                };
                                addMod(newProfileId, installedMod);
                                installedCount++;
                            } else {
                                throw new Error(installResult.error);
                            }
                        }
                    } catch (e) {
                        failedMods.push(mod.name);
                        console.error(`Error installing ${mod.name}`, e);
                    } finally {
                        if (trackedFullName) {
                            activeDownloads.delete(trackedFullName);
                            recomputeDownloadState();
                        }
                        completedCount++;
                        setProgressState(prev => ({
                            ...prev,
                            progress: Math.round((completedCount / Math.max(totalMods, 1)) * 100),
                            currentTask: `Processed ${completedCount}/${totalMods}: ${mod.name}...`
                        }));
                    }
                };

                if (totalMods > 0) {
                    for (let i = 0; i < modsToInstall.length; i += concurrency) {
                        const batch = modsToInstall.slice(i, i + concurrency);
                        if (concurrency === 1) {
                            if (batch[0]) await processMod(batch[0]);
                        } else {
                            await Promise.all(batch.map((mod: any) => processMod(mod)));
                        }
                    }

                    for (const mod of localMods) {
                        try {
                            const payloadPath = mod.payload;
                            const archivePath = result.archivePath;
                            if (!payloadPath || !archivePath) {
                                throw new Error('Embedded local payload is missing from this profile export.');
                            }
                            setProgressState(prev => ({
                                ...prev,
                                currentTask: `Staging custom mod ${mod.name} (${Math.min(completedCount + 1, totalMods)}/${totalMods})...`
                            }));
                            const imported = await window.ipcRenderer.importEmbeddedCustomMod(
                                newProfileId,
                                archivePath,
                                payloadPath,
                                {
                                    name: mod.displayName || mod.name?.split('-').slice(1).join('-') || mod.name,
                                    author: mod.author || mod.name?.split('-')[0] || 'Local',
                                    version: mod.version,
                                    enabled: mod.enabled,
                                    platforms: mod.platforms,
                                    expectedSha256: mod.sha256,
                                }
                            );
                            addMod(newProfileId, imported.mod);
                            installedCount++;
                        } catch (e) {
                            failedMods.push(mod.name);
                            console.error(`Error staging custom mod ${mod.name}`, e);
                        } finally {
                            completedCount++;
                            setProgressState(prev => ({
                                ...prev,
                                progress: Math.round((completedCount / Math.max(totalMods, 1)) * 100),
                                currentTask: `Processed ${completedCount}/${totalMods}...`
                            }));
                        }
                    }
                } else {
                    setProgressState(prev => ({ ...prev, progress: 100, currentTask: 'No mods found to import.' }));
                }

                unlistenDownloadProgress();

                setProgressState(prev => ({ ...prev, progress: 100, currentTask: 'Import Complete!' }));
                setTimeout(() => {
                    setProgressState(prev => ({ ...prev, isOpen: false }));
                    let msg = `Imported profile "${result.name}" with ${installedCount}/${totalMods} mods.`;
                    if (failedMods.length > 0) msg += `\n\nFailed to install:\n${failedMods.join('\n')}`;
                    alert(msg);
                }, 500);
            }, 500);

        } else if (result.type === 'package') {
            const pkg = result.package;
            const newProfileId = createProfile(pkg.name, selectedCommunity!);
            setTimeout(() => onInstallMod(pkg, newProfileId), 100);
        }
    };

    const handleImportProfile = async (code: string, platform: 'windows' | 'mac') => {
        if (!selectedCommunity) return;
        try {
            const result = await window.ipcRenderer.importProfile(code.trim());
            await processImportResult(result, platform);
        } catch (err: unknown) {
            alert(`Import failed: ${getErrorMessage(err, 'Unknown import error')}. Please check the code.`);
        }
    };

    const handleImportFile = async (path: string, platform: 'windows' | 'mac') => {
        if (!selectedCommunity) return;
        try {
            const result = await window.ipcRenderer.importProfileFromFile(path);
            await processImportResult(result, platform);
        } catch (err: unknown) {
            alert(`File import failed: ${getErrorMessage(err, 'Unknown file import error')}`);
        }
    };

    const handleExportFile = async () => {
        if (!activeProfileId) return;
        try {
            const result = await window.ipcRenderer.exportProfile(activeProfileId);
            if (result.success) alert(`Profile exported to: ${result.path}`);
        } catch (e: any) {
            alert(`Export failed: ${e.message}`);
        }
    };

    const handleExportCode = async () => {
        if (!activeProfileId) return;
        setProgressState({ isOpen: true, title: 'Generating Share Code', progress: 50, currentTask: 'Uploading profile...' });
        try {
            const code = await window.ipcRenderer.shareProfile(activeProfileId);
            setProgressState(prev => ({ ...prev, progress: 100, currentTask: 'Done!' }));
            setTimeout(() => {
                setProgressState(prev => ({ ...prev, isOpen: false }));
                navigator.clipboard.writeText(code);
                alert(`Profile Code Generated: ${code}\n\nCopied to clipboard!`);
            }, 500);
        } catch (e: any) {
            setProgressState(prev => ({ ...prev, isOpen: false }));
            alert(`Failed to generate code: ${e}`);
        }
    };

    return {
        handleImportProfile,
        handleImportFile,
        handleExportFile,
        handleExportCode,
        processImportResult,
    };
}
