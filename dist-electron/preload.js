import { contextBridge, ipcRenderer } from 'electron';
// --------- Expose some API to the Renderer process ---------
contextBridge.exposeInMainWorld('ipcRenderer', {
    on(...args) {
        const [channel, listener] = args;
        return ipcRenderer.on(channel, (event, ...args) => listener(event, ...args));
    },
    off(...args) {
        const [channel, ...omit] = args;
        return ipcRenderer.off(channel, ...omit);
    },
    send(...args) {
        const [channel, ...omit] = args;
        return ipcRenderer.send(channel, ...omit);
    },
    invoke(...args) {
        const [channel, ...omit] = args;
        return ipcRenderer.invoke(channel, ...omit);
    },
    // Profile API
    getProfiles: () => ipcRenderer.invoke('get-profiles'),
    saveProfiles: (profiles) => ipcRenderer.invoke('save-profiles', profiles),
    // File System API
    selectFolder: () => ipcRenderer.invoke('select-folder'),
    selectFile: () => ipcRenderer.invoke('select-file'),
    installMod: (profileId, downloadUrl, modName) => ipcRenderer.invoke('install-mod', { profileId, downloadUrl, modName }),
    checkDirectoryExists: (dirPath) => ipcRenderer.invoke('check-directory-exists', dirPath),
    // Thunderstore API
    fetchCommunities: () => ipcRenderer.invoke('fetch-communities'),
    fetchCommunityImages: () => ipcRenderer.invoke('fetch-community-images'),
    fetchPackages: (communityIdentifier) => ipcRenderer.invoke('fetch-packages', communityIdentifier),
    fetchPackageByName: (name) => ipcRenderer.invoke('fetch-package-by-name', name),
    importProfile: (code) => ipcRenderer.invoke('import-profile', code),
    importProfileFromFile: (path) => ipcRenderer.invoke('import-profile-from-file', path),
    openModFolder: (profileId, modName) => ipcRenderer.invoke('open-mod-folder', { profileId, modName }),
    exportProfile: (profileId) => ipcRenderer.invoke('export-profile', profileId),
    deleteProfileFolder: (profileId) => ipcRenderer.invoke('delete-profile-folder', profileId),
});
//# sourceMappingURL=preload.js.map