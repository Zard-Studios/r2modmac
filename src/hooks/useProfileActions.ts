import type { Package } from '../types/thunderstore';
import type { InstalledMod } from '../types/profile';
import type { ProgressSetter } from '../types/progress';
import { useProfileStore } from '../store/useProfileStore';
import { findPinnedVersion } from '../utils/modVersioning';

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
    setProgressState: ProgressSetter;
    onInstallMod: (pkg: Package, profileId: string) => Promise<void>;
}

export function useProfileActions({
    selectedCommunity,
    activeProfileId,
    setProgressState,
    onInstallMod,
}: UseProfileActionsProps) {
    const { createProfile, setProfiles, addMod, updateProfile } = useProfileStore();

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

            {
                setProgressState({
                    isOpen: true,
                    title: 'Importing Profile',
                    progress: 0,
                    currentTask: 'Resolving pinned versions...',
                });
                const modsToAdd = thunderstoreMods;
                const resolvedMods: InstalledMod[] = [];
                const failedMods: string[] = [];

                for (let index = 0; index < modsToAdd.length; index++) {
                    const mod = modsToAdd[index];
                    try {
                        const pkg = lookup.found.find((p: Package) => p.full_name.toLowerCase() === mod.name.toLowerCase());
                        const exactPkg = pkg?.versions.some((v: any) => v.version_number === mod.version)
                            ? pkg
                            : await window.ipcRenderer.fetchPackageByName(`${mod.name}-${mod.version}`, selectedCommunity);
                        if (!exactPkg) throw new Error(`pinned version ${mod.version} does not exist`);
                        const version = findPinnedVersion(exactPkg, mod.version, mod.name);
                        resolvedMods.push({
                            uuid4: version.uuid4,
                            fullName: version.full_name,
                            versionNumber: version.version_number,
                            iconUrl: version.icon,
                            enabled: mod.enabled,
                            pending_sync: true,
                        });
                    } catch (error) {
                        failedMods.push(`${mod.name}: ${getErrorMessage(error, 'unknown resolution error')}`);
                    }
                    setProgressState(prev => ({
                        ...prev,
                        progress: Math.round(((index + 1) / Math.max(modsToAdd.length, 1)) * 85),
                        currentTask: `Resolved ${index + 1}/${modsToAdd.length} pinned mods...`,
                    }));
                }

                if (failedMods.length > 0) {
                    setProgressState(prev => ({ ...prev, isOpen: false }));
                    await window.ipcRenderer.alert(
                        'Profile Import Aborted',
                        `No partial profile was created. ${failedMods.length} pinned mod(s) could not be resolved after retries:\n\n${failedMods.slice(0, 12).join('\n')}${failedMods.length > 12 ? `\n...and ${failedMods.length - 12} more` : ''}`
                    );
                    return;
                }

                const newProfileId = createProfile(profileName, selectedCommunity!, chosenPlatform);
                for (const mod of resolvedMods) addMod(newProfileId, mod);
                let installedCount = resolvedMods.length;
                const localFailures: string[] = [];
                const stagedLocalIds: string[] = [];
                for (const mod of localMods) {
                    try {
                        if (!mod.payload || !result.archivePath) {
                            throw new Error('embedded local payload is missing');
                        }
                        const imported = await window.ipcRenderer.importEmbeddedCustomMod(
                            newProfileId,
                            result.archivePath,
                            mod.payload,
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
                        if (imported.mod.localId) stagedLocalIds.push(imported.mod.localId);
                        installedCount++;
                    } catch (error) {
                        localFailures.push(`${mod.name}: ${getErrorMessage(error, 'custom payload error')}`);
                    }
                }
                if (localFailures.length > 0) {
                    setProfiles(useProfileStore.getState().profiles.filter(profile => profile.id !== newProfileId));
                    await Promise.allSettled(stagedLocalIds.map(localId =>
                        window.ipcRenderer.deleteLocalModPayload(newProfileId, localId)
                    ));
                    setProgressState(prev => ({ ...prev, isOpen: false }));
                    await window.ipcRenderer.alert(
                        'Profile Import Aborted',
                        `No partial profile was kept. ${localFailures.length} embedded custom mod(s) could not be staged:\n\n${localFailures.slice(0, 12).join('\n')}${localFailures.length > 12 ? `\n...and ${localFailures.length - 12} more` : ''}`
                    );
                    return;
                }
                updateProfile(newProfileId, { needs_sync: true });
                setProgressState(prev => ({ ...prev, isOpen: false, progress: 100, currentTask: 'Import complete' }));
                const totalMods = modsToAdd.length + localMods.length;
                const message = `Imported profile "${result.name}" with ${installedCount}/${totalMods} mods at their pinned versions.`;
                await window.ipcRenderer.alert('Profile Imported', message);
                return;
            }

        } else if (result.type === 'package') {
            const pkg = result.package;
            const newProfileId = createProfile(pkg.name, selectedCommunity!);
            await onInstallMod(pkg, newProfileId);
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
