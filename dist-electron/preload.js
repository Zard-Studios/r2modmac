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
    installMod: (downloadUrl, modName, gameDir) => ipcRenderer.invoke('install-mod', downloadUrl, modName, gameDir),
    checkDirectoryExists: (dirPath) => ipcRenderer.invoke('check-directory-exists', dirPath),
    // Thunderstore API
    fetchCommunities: () => ipcRenderer.invoke('fetch-communities'),
    fetchPackages: (communityIdentifier) => ipcRenderer.invoke('fetch-packages', communityIdentifier),
});
//# sourceMappingURL=preload.js.map