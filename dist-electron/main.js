import { app, BrowserWindow, ipcMain, dialog } from 'electron';
import path from 'path';
import { fileURLToPath } from 'url';
import fs from 'fs';
import { ModManager } from './modManager.js';
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const PROFILES_FILE = 'profiles.json';
function getProfilesPath() {
    return path.join(app.getPath('userData'), PROFILES_FILE);
}
ipcMain.handle('get-profiles', async () => {
    const p = getProfilesPath();
    if (!fs.existsSync(p))
        return [];
    try {
        const data = fs.readFileSync(p, 'utf-8');
        return JSON.parse(data);
    }
    catch (e) {
        console.error('Failed to read profiles', e);
        return [];
    }
});
ipcMain.handle('save-profiles', async (_, profiles) => {
    const p = getProfilesPath();
    try {
        fs.writeFileSync(p, JSON.stringify(profiles, null, 2));
        return true;
    }
    catch (e) {
        console.error('Failed to save profiles', e);
        return false;
    }
});
// File System handlers
ipcMain.handle('select-folder', async () => {
    const result = await dialog.showOpenDialog({
        properties: ['openDirectory']
    });
    if (result.canceled)
        return null;
    return result.filePaths[0];
});
ipcMain.handle('install-mod', async (_, downloadUrl, modName, gameDir) => {
    try {
        const tempDir = path.join(app.getPath('temp'), 'r2modmac');
        await ModManager.installMod(downloadUrl, modName, gameDir, tempDir);
        return { success: true };
    }
    catch (error) {
        console.error('Failed to install mod', error);
        return { success: false, error: error.message };
    }
});
ipcMain.handle('check-directory-exists', async (_, dirPath) => {
    return fs.existsSync(dirPath);
});
// Thunderstore API handlers
ipcMain.handle('fetch-communities', async () => {
    try {
        const https = await import('https');
        return new Promise((resolve, reject) => {
            https.get('https://thunderstore.io/api/experimental/community/', (res) => {
                let data = '';
                res.on('data', chunk => data += chunk);
                res.on('end', () => {
                    try {
                        const json = JSON.parse(data);
                        resolve(json.results || json);
                    }
                    catch (e) {
                        reject(e);
                    }
                });
            }).on('error', reject);
        });
    }
    catch (error) {
        console.error('Failed to fetch communities', error);
        throw error;
    }
});
ipcMain.handle('fetch-packages', async (_, communityIdentifier) => {
    try {
        const https = await import('https');
        return new Promise((resolve, reject) => {
            https.get(`https://thunderstore.io/c/${communityIdentifier}/api/v1/package/`, (res) => {
                let data = '';
                res.on('data', chunk => data += chunk);
                res.on('end', () => {
                    try {
                        resolve(JSON.parse(data));
                    }
                    catch (e) {
                        reject(e);
                    }
                });
            }).on('error', reject);
        });
    }
    catch (error) {
        console.error('Failed to fetch packages', error);
        throw error;
    }
});
process.env.DIST = path.join(__dirname, '../dist');
process.env.VITE_PUBLIC = app.isPackaged ? process.env.DIST : path.join(process.env.DIST, '../public');
if (!process.env.VITE_PUBLIC)
    throw new Error("VITE_PUBLIC is undefined");
if (!process.env.DIST)
    throw new Error("DIST is undefined");
let win;
const VITE_DEV_SERVER_URL = process.env['VITE_DEV_SERVER_URL'];
function createWindow() {
    win = new BrowserWindow({
        icon: path.join(process.env.VITE_PUBLIC, 'electron-vite.svg'),
        webPreferences: {
            preload: path.join(__dirname, 'preload.js'),
            contextIsolation: true,
            nodeIntegration: false,
            sandbox: false,
        },
        width: 1200,
        height: 800,
        titleBarStyle: 'hiddenInset', // Mac style
        vibrancy: 'under-window', // Mac blur effect
        visualEffectState: 'active',
    });
    // Test active push message to Console
    win.webContents.on('did-finish-load', () => {
        win?.webContents.send('main-process-message', (new Date).toLocaleString());
    });
    if (VITE_DEV_SERVER_URL) {
        win.loadURL(VITE_DEV_SERVER_URL);
    }
    else {
        if (!app.isPackaged) {
            win.loadURL('http://localhost:5173');
        }
        else {
            win.loadFile(path.join(process.env.DIST, 'index.html'));
        }
    }
}
app.on('window-all-closed', () => {
    if (process.platform !== 'darwin') {
        app.quit();
    }
});
app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
        createWindow();
    }
});
app.whenReady().then(createWindow);
//# sourceMappingURL=main.js.map