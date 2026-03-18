import type { Package } from '../types/thunderstore';
import type { InstalledMod } from '../types/profile';
import { useProfileStore } from '../store/useProfileStore';

const MAX_PARALLEL_OPS = 10;

interface ProgressSetter {
    (state: { isOpen: boolean; title: string; progress: number; currentTask: string }): void;
    (updater: (prev: { isOpen: boolean; title: string; progress: number; currentTask: string }) => { isOpen: boolean; title: string; progress: number; currentTask: string }): void;
}

interface UseProfileActionsProps {
    selectedCommunity: string | null;
    activeProfileId: string | null;
    legacyInstallMode: boolean;
    installInParallel: boolean;
    setProgressState: ProgressSetter;
    onInstallMod: (pkg: Package, profileId: string) => Promise<void>;
}

export function useProfileActions({
    selectedCommunity,
    activeProfileId,
    legacyInstallMode,
    installInParallel,
    setProgressState,
    onInstallMod,
}: UseProfileActionsProps) {
    const { createProfile, addMod } = useProfileStore();

    const processImportResult = async (result: any, chosenPlatform: 'windows' | 'mac') => {
        if (result.type === 'profile') {
            let profileName: string = result.name;

            const gamePath = await window.ipcRenderer.getGamePath(selectedCommunity || '', chosenPlatform);
            if (!gamePath) {
                await window.ipcRenderer.alert(
                    'Game Path Required',
                    'Please configure the game directory in Settings before importing profiles.\n\nGo to Settings → Game Directory → Set the path to your Windows game folder inside Wine, CrossOver, Wineskin, or another compatible wrapper.'
                );
                return;
            }

            if (profileName.startsWith('Imported: ')) profileName = profileName.substring(10);

            const modNames = result.mods.map((m: any) => m.name);
            const lookup = await window.ipcRenderer.lookupPackagesByNames(selectedCommunity!, modNames);

            if (lookup.unknown.length > 0) {
                const proceed = await window.ipcRenderer.confirm(
                    'Some mods cannot be found',
                    `${lookup.unknown.length} mod(s) from the profile were not found and will not be installed:\n\n${lookup.unknown.join('\n')}\n\n${lookup.found.length} mod(s) will be installed. Do you want to continue?`
                );
                if (!proceed) return;
            }

            const newProfileId = createProfile(profileName, selectedCommunity!, chosenPlatform);

            const concurrency = installInParallel ? MAX_PARALLEL_OPS : 1;
            setProgressState({
                isOpen: true,
                title: 'Importing Profile',
                progress: 0,
                currentTask: 'Starting import...',
            });

            setTimeout(async () => {
                const modsToInstall = result.mods.filter((m: any) => !lookup.unknown.includes(m.name));
                let installedCount = 0;
                let completedCount = 0;
                const totalMods = modsToInstall.length;
                const failedMods: string[] = [];

                const processMod = async (mod: any) => {
                    try {
                        setProgressState(prev => ({
                            ...prev,
                            currentTask: `Installing ${mod.name} (${Math.min(completedCount + 1, totalMods)}/${totalMods})...`
                        }));

                        const pkg = lookup.found.find((p: Package) => p.full_name === mod.name);
                        if (pkg) {
                            const version = pkg.versions.find((v: any) => v.version_number === mod.version) || pkg.versions[0];
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
                        completedCount++;
                        setProgressState(prev => ({
                            ...prev,
                            progress: Math.round((completedCount / Math.max(totalMods, 1)) * 100),
                            currentTask: `Processed ${completedCount}/${totalMods}...`
                        }));
                    }
                };

                if (totalMods > 0) {
                    for (let i = 0; i < totalMods; i += concurrency) {
                        const batch = modsToInstall.slice(i, i + concurrency);
                        if (concurrency === 1) {
                            await processMod(batch[0]);
                        } else {
                            await Promise.all(batch.map((mod: any) => processMod(mod)));
                        }
                    }
                } else {
                    setProgressState(prev => ({ ...prev, progress: 100, currentTask: 'No mods found to import.' }));
                }

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
        } catch (err: any) {
            alert(`Import failed: ${err.message}. Please check the code.`);
        }
    };

    const handleImportFile = async (path: string, platform: 'windows' | 'mac') => {
        if (!selectedCommunity) return;
        try {
            const result = await window.ipcRenderer.importProfileFromFile(path);
            await processImportResult(result, platform);
        } catch (err: any) {
            alert(`File import failed: ${err.message}`);
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
