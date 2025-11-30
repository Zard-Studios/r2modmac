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
    // You can expose other APTs you need here.
    // ...
});
//# sourceMappingURL=preload.js.map