import { contextBridge, ipcRenderer } from 'electron'

// --------- Expose some API to the Renderer process ---------
contextBridge.exposeInMainWorld('ipcRenderer', {
    on(...args: Parameters<typeof ipcRenderer.on>) {
        const [channel, listener] = args
        return ipcRenderer.on(channel, (event, ...args) => listener(event, ...args))
    },
    off(...args: Parameters<typeof ipcRenderer.off>) {
        const [channel, ...omit] = args
        return ipcRenderer.off(channel, ...omit)
    },
    send(...args: Parameters<typeof ipcRenderer.send>) {
        const [channel, ...omit] = args
        return ipcRenderer.send(channel, ...omit)
    },
    invoke(...args: Parameters<typeof ipcRenderer.invoke>) {
        const [channel, ...omit] = args
        return ipcRenderer.invoke(channel, ...omit)
    },

    // Profile API
    getProfiles: () => ipcRenderer.invoke('get-profiles'),
    saveProfiles: (profiles: any) => ipcRenderer.invoke('save-profiles', profiles),

    // File System API
    selectFolder: () => ipcRenderer.invoke('select-folder'),
    selectFile: () => ipcRenderer.invoke('select-file'),
    installMod: (profileId: string, downloadUrl: string, modName: string) =>
        ipcRenderer.invoke('install-mod', { profileId, downloadUrl, modName }),
    checkDirectoryExists: (dirPath: string) => ipcRenderer.invoke('check-directory-exists', dirPath),

    // Thunderstore API
    fetchCommunities: () => ipcRenderer.invoke('fetch-communities'),
    fetchCommunityImages: () => ipcRenderer.invoke('fetch-community-images'),
    fetchPackages: (communityIdentifier: string) => ipcRenderer.invoke('fetch-packages', communityIdentifier),
    fetchPackageByName: (name: string) => ipcRenderer.invoke('fetch-package-by-name', name),
    importProfile: (code: string) => ipcRenderer.invoke('import-profile', code),
    importProfileFromFile: (path: string) => ipcRenderer.invoke('import-profile-from-file', path),
    openModFolder: (profileId: string, modName: string) => ipcRenderer.invoke('open-mod-folder', { profileId, modName }),
    exportProfile: (profileId: string) => ipcRenderer.invoke('export-profile', profileId),
    deleteProfileFolder: (profileId: string) => ipcRenderer.invoke('delete-profile-folder', profileId),
})
