import { listen } from '@tauri-apps/api/event';
import { ModDetailModal } from '../modals/ModDetailModal'
import { ProgressModal } from '../modals/ProgressModal'
import { UninstallModal } from '../modals/UninstallModal'
import { SettingsModal } from '../modals/SettingsModal'
import { ExportModal } from '../modals/ExportModal'
import { CrossOverGuideModal } from '../modals/CrossOverGuideModal';
import { MacOSGuideModal } from '../modals/MacOSGuideModal';
import { UpdateModal } from '../modals/UpdateModal';
import PreferencesModal, { type PreferencesSettings } from '../modals/PreferencesModal';
import type { Package, PackageVersion } from '../../types/thunderstore'
import type { UpdateInfo } from '../../types/electron';

export interface AppModalsProps {
    selectedMod: Package | null;
    setSelectedMod: (mod: Package | null) => void;
    activeProfileId: string | null;
    profiles: any[];
    selectedCommunity: string | null;
    handleInstallMod: (pkg: Package, profileId: string, version?: PackageVersion) => void;
    handleUpdateMod: (pkg: Package, profileId: string, version?: PackageVersion) => void;
    handleUninstallWithDependencies: (pkg: Package, profileId: string) => void;
    isBrowsingMode: boolean;
    progressState: any;
    setProgressState: (state: any) => void;
    uninstallModalState: any;
    setUninstallModalState: (state: any) => void;
    executeUninstall: (deps: string[]) => void;
    showSettings: boolean;
    setShowSettings: (show: boolean) => void;
    showExportModal: boolean;
    setShowExportModal: (show: boolean) => void;
    handleExportCode: () => void;
    handleExportFile: () => void;
    showUpdateModal: boolean;
    setShowUpdateModal: (show: boolean) => void;
    updateInfo: UpdateInfo | null;
    showCrossOverGuide: boolean;
    setShowCrossOverGuide: (show: boolean) => void;
    hideCrossOverGuide: boolean;
    setHideCrossOverGuide: (hide: boolean) => void;
    showMacOSGuide: boolean;
    setShowMacOSGuide: (show: boolean) => void;
    hideMacOSGuide: boolean;
    setHideMacOSGuide: (hide: boolean) => void;
    showPreferences: boolean;
    setShowPreferences: (show: boolean) => void;
    preferences: PreferencesSettings;
    onSavePreferences: (settings: PreferencesSettings) => Promise<void>;
    hasHiddenGuideWarnings: boolean;
    onRestoreGuideWarnings: () => Promise<void>;
    onSetGuideHidden: (guide: 'crossover' | 'macos', hidden: boolean) => Promise<void>;
    legacyInstallMode: boolean;
}

export function AppModals({
    selectedMod, setSelectedMod,
    activeProfileId, profiles, selectedCommunity,
    handleInstallMod, handleUpdateMod, handleUninstallWithDependencies,
    isBrowsingMode,
    progressState, setProgressState,
    uninstallModalState, setUninstallModalState, executeUninstall,
    showSettings, setShowSettings,
    showExportModal, setShowExportModal, handleExportCode, handleExportFile,
    showUpdateModal, setShowUpdateModal, updateInfo,
    showCrossOverGuide, setShowCrossOverGuide, hideCrossOverGuide, setHideCrossOverGuide,
    showMacOSGuide, setShowMacOSGuide, hideMacOSGuide, setHideMacOSGuide,
    showPreferences, setShowPreferences, preferences, onSavePreferences,
    hasHiddenGuideWarnings, onRestoreGuideWarnings, onSetGuideHidden,
    legacyInstallMode
}: AppModalsProps) {

    return (
        <>
            {selectedMod && (
                <ModDetailModal
                    pkg={selectedMod}
                    isOpen={!!selectedMod}
                    gameId={selectedCommunity || ''}
                    installedMods={activeProfileId ? profiles.find(p => p.id === activeProfileId)?.mods || [] : []}
                    onClose={() => setSelectedMod(null)}
                    onInstall={(version) => {
                        if (activeProfileId) {
                            handleInstallMod(selectedMod, activeProfileId, version);
                        } else {
                            alert('Please select or create a profile to install mods.');
                        }
                    }}
                    onUpdate={(version) => {
                        if (activeProfileId) {
                            handleUpdateMod(selectedMod, activeProfileId, version);
                        }
                    }}
                    onUninstall={async () => {
                        if (!activeProfileId || !selectedMod) return;
                        await handleUninstallWithDependencies(selectedMod, activeProfileId);
                    }}
                    isInstalled={
                        activeProfileId
                            ? profiles.find(p => p.id === activeProfileId)?.mods.some((m: any) => m.fullName.startsWith(selectedMod.full_name)) ?? false
                            : false
                    }
                    hasUpdate={
                        activeProfileId
                            ? (() => {
                                const profile = profiles.find(p => p.id === activeProfileId);
                                const installed = profile?.mods.find((m: any) => m.fullName.startsWith(selectedMod.full_name));
                                const latestVersion = selectedMod.versions[0].version_number;
                                return installed ? latestVersion !== installed.versionNumber : false;
                            })()
                            : false
                    }
                    isBrowsing={isBrowsingMode}
                    legacyInstallMode={legacyInstallMode}
                />
            )}

            <ProgressModal
                isOpen={progressState.isOpen}
                title={progressState.title}
                progress={progressState.progress}
                currentTask={progressState.currentTask}
            />

            <UninstallModal
                isOpen={uninstallModalState.isOpen}
                modName={uninstallModalState.pkg?.name || ''}
                modIcon={uninstallModalState.pkg?.versions[0]?.icon}
                orphanDeps={uninstallModalState.orphanDeps}
                allDepsCount={uninstallModalState.allInstalledDeps.length}
                onCancel={() => setUninstallModalState((prev: any) => ({ ...prev, isOpen: false }))}
                onModOnly={() => executeUninstall([])}
                onWithOrphans={() => executeUninstall(uninstallModalState.orphanDeps.map((d: any) => d.name))}
                onWithAllDeps={() => executeUninstall(uninstallModalState.allInstalledDeps)}
            />

            <SettingsModal
                isOpen={showSettings}
                onClose={() => setShowSettings(false)}
                selectedGame={selectedCommunity || undefined}
                activeProfilePlatform={profiles.find(p => p.id === activeProfileId)?.platform}
            />

            {showExportModal && activeProfileId && (
                <ExportModal
                    isOpen={showExportModal}
                    onClose={() => setShowExportModal(false)}
                    onExportCode={handleExportCode}
                    onExportFile={handleExportFile}
                />
            )}

            {showUpdateModal && updateInfo && (
                <UpdateModal
                    updateInfo={updateInfo}
                    onClose={() => setShowUpdateModal(false)}
                    onUpdate={async () => {
                        if (updateInfo.download_url) {
                            setProgressState({
                                isOpen: true,
                                title: 'Updating r2modmac',
                                progress: 0,
                                currentTask: 'Downloading update...'
                            });

                            // Listen for progress
                            const unlisten = await listen<number>('update-progress', (event) => {
                                setProgressState((prev: any) => ({
                                    ...prev,
                                    progress: event.payload
                                }));
                            });

                            try {
                                await window.ipcRenderer.installUpdate(updateInfo.download_url);
                                unlisten(); // Clean up listener when done (or before closing)
                                // The script waits for PID exit.
                                window.close();
                            } catch (e) {
                                alert("Update failed: " + e);
                                setProgressState((prev: any) => ({ ...prev, isOpen: false }));
                            }
                        }
                    }}
                />
            )}

            {showCrossOverGuide && !hideCrossOverGuide && (
                <CrossOverGuideModal
                    isOpen={showCrossOverGuide}
                    onClose={() => setShowCrossOverGuide(false)}
                    onDontShowAgain={() => {
                        setHideCrossOverGuide(true);
                        void onSetGuideHidden('crossover', true).catch((e) => console.error('Failed to persist CrossOver guide preference', e));
                        setShowCrossOverGuide(false);
                    }}
                />
            )}

            {showMacOSGuide && !hideMacOSGuide && (
                <MacOSGuideModal
                    isOpen={showMacOSGuide}
                    onClose={() => setShowMacOSGuide(false)}
                    onDontShowAgain={() => {
                        setHideMacOSGuide(true);
                        void onSetGuideHidden('macos', true).catch((e) => console.error('Failed to persist macOS guide preference', e));
                        setShowMacOSGuide(false);
                    }}
                />
            )}

            <PreferencesModal
                isOpen={showPreferences}
                onClose={() => setShowPreferences(false)}
                settings={preferences}
                onSave={onSavePreferences}
                hasHiddenGuideWarnings={hasHiddenGuideWarnings}
                onRestoreGuideWarnings={onRestoreGuideWarnings}
            />
        </>
    );
}
