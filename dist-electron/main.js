import { app, BrowserWindow, ipcMain, dialog, shell } from 'electron';
import path from 'path';
import { fileURLToPath } from 'url';
import fs from 'fs';
import os from 'os';
import https from 'https';
import { exec } from 'child_process';
import yaml from 'js-yaml';
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
ipcMain.handle('select-file', async () => {
    const result = await dialog.showOpenDialog({
        properties: ['openFile'],
        filters: [{ name: 'r2modman Profile', extensions: ['r2z', 'zip'] }]
    });
    if (result.canceled)
        return null;
    return result.filePaths[0];
});
ipcMain.handle('install-mod', async (_, { profileId, downloadUrl, modName }) => {
    try {
        const profileDir = path.join(app.getPath('userData'), 'profiles', profileId);
        const tempDir = path.join(app.getPath('temp'), 'r2modmac-temp');
        if (!fs.existsSync(profileDir)) {
            fs.mkdirSync(profileDir, { recursive: true });
        }
        await ModManager.installMod(downloadUrl, modName, profileDir, tempDir);
        return { success: true, profileDir: profileDir };
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
        const fetchPage = (url) => {
            return new Promise((resolve, reject) => {
                https.get(url, (res) => {
                    let data = '';
                    res.on('data', chunk => data += chunk);
                    res.on('end', () => {
                        try {
                            const json = JSON.parse(data);
                            resolve(json);
                        }
                        catch (e) {
                            reject(e);
                        }
                    });
                }).on('error', reject);
            });
        };
        let allResults = [];
        let nextUrl = 'https://thunderstore.io/api/experimental/community/';
        while (nextUrl) {
            console.log(`Fetching communities from: ${nextUrl}`);
            const data = await fetchPage(nextUrl);
            if (data.results) {
                allResults = [...allResults, ...data.results];
                nextUrl = data.pagination?.next_link || null;
            }
            else {
                // Fallback for non-paginated response or error
                if (Array.isArray(data)) {
                    allResults = data;
                }
                else {
                    allResults = [data];
                }
                nextUrl = null;
            }
        }
        console.log(`Fetched total ${allResults.length} communities`);
        return allResults;
    }
    catch (error) {
        console.error('Failed to fetch communities', error);
        throw error;
    }
});
// Fetch community cover images from /communities/ page
ipcMain.handle('fetch-community-images', async () => {
    try {
        return new Promise((resolve, reject) => {
            https.get('https://thunderstore.io/communities/', (res) => {
                let data = '';
                res.on('data', chunk => data += chunk);
                res.on('end', () => {
                    try {
                        // Extract image URLs from both <link rel="preload"> and <img> tags
                        const imageMap = {};
                        // Pattern 1: <link rel="preload" as="image" href="..."/>
                        const preloadRegex = /<link rel="preload" as="image" href="(https:\/\/gcdn\.thunderstore\.io\/live\/community\/([a-z0-9-]+)\/[^"]+)"/g;
                        let match;
                        while ((match = preloadRegex.exec(data)) !== null) {
                            const coverUrl = match[1];
                            const identifier = match[2];
                            // Only accept images that look like covers (contain 360x480 or 'cover' or 'cov' or 'banner')
                            if (coverUrl.includes('360x480') || coverUrl.includes('cover') || coverUrl.includes('cov') || coverUrl.includes('Banner')) {
                                if (!imageMap[identifier]) {
                                    imageMap[identifier] = coverUrl;
                                    console.log(`[preload] Extracted image for ${identifier}: ${coverUrl}`);
                                }
                            }
                        }
                        // Pattern 2: <img class="image__src" ... src="https://gcdn.thunderstore.io/live/community/IDENTIFIER/..."/>
                        // More flexible - accepts any image from the community folder
                        const imgRegex = /<img[^>]*class="image__src"[^>]*src="(https:\/\/gcdn\.thunderstore\.io\/live\/community\/([a-z0-9-]+)\/[^"]+\.(png|webp|jpg))"/g;
                        while ((match = imgRegex.exec(data)) !== null) {
                            const coverUrl = match[1];
                            const identifier = match[2];
                            if (!imageMap[identifier]) {
                                imageMap[identifier] = coverUrl;
                                console.log(`[img] Extracted image for ${identifier}: ${coverUrl}`);
                            }
                        }
                        console.log(`Extracted ${Object.keys(imageMap).length} community images`);
                        resolve(imageMap);
                    }
                    catch (e) {
                        reject(e);
                    }
                });
            }).on('error', reject);
        });
    }
    catch (error) {
        console.error('Failed to fetch community images', error);
        throw error;
    }
});
ipcMain.handle('fetch-packages', async (_, communityIdentifier) => {
    try {
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
ipcMain.handle('fetch-package-by-name', async (_, nameString) => {
    // nameString might be "Namespace-Name" or "Namespace-Name-Version"
    // 1. Strip version if present
    const versionMatch = nameString.match(/^(.*)-(\d+\.\d+\.\d+)$/);
    let cleanName = nameString;
    if (versionMatch) {
        cleanName = versionMatch[1];
        console.log(`Stripped version for fetch: ${nameString} -> ${cleanName}`);
    }
    // 2. Split Namespace and Name
    // Assumption: Namespace is the first part before the first dash
    const parts = cleanName.split('-');
    const namespace = parts[0];
    const name = parts.slice(1).join('-');
    console.log(`Fetching package by name: ${namespace}/${name}`);
    try {
        return new Promise((resolve, reject) => {
            https.get(`https://thunderstore.io/api/v1/package/${namespace}/${name}/`, (res) => {
                let data = '';
                res.on('data', chunk => data += chunk);
                res.on('end', () => {
                    try {
                        if (res.statusCode !== 200) {
                            console.error(`Failed to fetch ${namespace}/${name}: Status ${res.statusCode}`);
                            console.error(`Response: ${data.substring(0, 200)}`);
                            resolve(null); // Not found
                            return;
                        }
                        console.log(`Successfully fetched ${namespace}/${name}`);
                        resolve(JSON.parse(data));
                    }
                    catch (e) {
                        console.error(`Error parsing response for ${namespace}/${name}:`, e);
                        reject(e);
                    }
                });
            }).on('error', (err) => {
                console.error(`Network error fetching ${namespace}/${name}:`, err);
                reject(err);
            });
        });
    }
    catch (error) {
        console.error('Failed to fetch package by name', error);
        throw error;
    }
});
// Helper to download file to buffer
const downloadToBuffer = (url) => {
    return new Promise((resolve, reject) => {
        https.get(url, (res) => {
            if (res.statusCode === 302 || res.statusCode === 301) {
                downloadToBuffer(res.headers.location).then(resolve).catch(reject);
                return;
            }
            if (res.statusCode !== 200) {
                reject(new Error(`Failed to download: ${res.statusCode}`));
                return;
            }
            const chunks = [];
            res.on('data', (chunk) => chunks.push(chunk));
            res.on('end', () => resolve(Buffer.concat(chunks)));
        }).on('error', reject);
    });
};
ipcMain.handle('import-profile', async (_, code) => {
    console.log(`Importing profile with code: ${code}`);
    try {
        // STRATEGY 1: Try as r2modman Profile Code
        const profileUrl = `https://thunderstore.io/api/experimental/legacyprofile/get/${code}/`;
        console.log(`Strategy 1: Trying Profile Code via ${profileUrl}`);
        try {
            const buffer = await downloadToBuffer(profileUrl);
            const content = buffer.toString('utf-8');
            if (content.startsWith('#r2modman')) {
                console.log('Detected r2modman profile export');
                const base64 = content.substring(9).trim(); // Remove #r2modman
                const zipBuffer = Buffer.from(base64, 'base64');
                const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'r2modmac-import-'));
                const zipPath = path.join(tempDir, 'profile.zip');
                fs.writeFileSync(zipPath, zipBuffer);
                // Extract
                await new Promise((resolve, reject) => {
                    exec(`unzip -o "${zipPath}" -d "${tempDir}"`, (err) => {
                        if (err)
                            reject(err);
                        else
                            resolve();
                    });
                });
                // Read export.r2x
                const exportPath = path.join(tempDir, 'export.r2x');
                if (fs.existsSync(exportPath)) {
                    const r2xContent = fs.readFileSync(exportPath, 'utf-8');
                    const profileData = yaml.load(r2xContent);
                    console.log(`Parsed profile: ${profileData.profileName}`);
                    // Map to Package format expected by frontend
                    // We need to fetch package details for each mod to get download URLs
                    // For now, return the raw list and let frontend or a helper handle resolution?
                    // Better: Return a special "ProfileImport" object
                    return {
                        type: 'profile',
                        name: profileData.profileName,
                        game: profileData.gameIdentifier || null,
                        mods: profileData.mods.map((m) => {
                            const versionStr = `${m.version.major}.${m.version.minor}.${m.version.patch}`;
                            let name = m.name;
                            // Fix for bad exports: strip version from name if present using Regex
                            const versionMatch = name.match(/^(.*)-(\d+\.\d+\.\d+)$/);
                            if (versionMatch) {
                                name = versionMatch[1];
                            }
                            return {
                                name: name,
                                version: versionStr,
                                enabled: m.enabled
                            };
                        })
                    };
                }
            }
        }
        catch (e) {
            console.log('Strategy 1 failed or not a profile code, trying Strategy 2...');
        }
        // STRATEGY 2: Try as Package UUID (Namespace lookup)
        console.log('Strategy 2: Trying Package UUID lookup');
        // Step 1: Resolve UUID to namespace/name
        const resolveUrl = `https://thunderstore.io/api/experimental/namespace-by-id/${code}/`;
        const metadata = await new Promise((resolve, reject) => {
            https.get(resolveUrl, (res) => {
                let data = '';
                res.on('data', chunk => data += chunk);
                res.on('end', () => {
                    if (res.statusCode !== 200) {
                        reject(new Error(`UUID not found`));
                        return;
                    }
                    try {
                        resolve(JSON.parse(data));
                    }
                    catch (e) {
                        reject(e);
                    }
                });
            }).on('error', reject);
        });
        const { namespace, name } = metadata;
        console.log(`Found package: ${namespace}/${name}`);
        // Step 2: Fetch full package details
        const packageUrl = `https://thunderstore.io/api/v1/package/${namespace}/${name}/`;
        const pkg = await new Promise((resolve, reject) => {
            https.get(packageUrl, (res) => {
                let data = '';
                res.on('data', chunk => data += chunk);
                res.on('end', () => {
                    if (res.statusCode !== 200)
                        reject(new Error('Package details not found'));
                    else
                        resolve(JSON.parse(data));
                });
            }).on('error', reject);
        });
        return {
            type: 'package',
            package: pkg
        };
    }
    catch (error) {
        console.error('Import failed', error);
        throw error;
    }
});
ipcMain.handle('import-profile-from-file', async (_, filePath) => {
    console.log(`Importing profile from file: ${filePath}`);
    try {
        const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'r2modmac-import-file-'));
        // Extract zip
        await new Promise((resolve, reject) => {
            exec(`unzip -o "${filePath}" -d "${tempDir}"`, (err) => {
                if (err)
                    reject(err);
                else
                    resolve();
            });
        });
        // Read export.r2x
        const exportPath = path.join(tempDir, 'export.r2x');
        if (fs.existsSync(exportPath)) {
            const r2xContent = fs.readFileSync(exportPath, 'utf-8');
            const profileData = yaml.load(r2xContent);
            return {
                type: 'profile',
                name: profileData.profileName,
                game: profileData.gameIdentifier || null,
                mods: profileData.mods.map((m) => {
                    const versionStr = `${m.version.major}.${m.version.minor}.${m.version.patch}`;
                    let name = m.name;
                    console.log(`Processing import mod: "${name}", version: "${versionStr}"`);
                    // Fix for bad exports: strip version from name if present
                    // Strategy 1: Regex match for "-X.Y.Z" at the end
                    const versionMatch = name.match(/^(.*)-(\d+\.\d+\.\d+)$/);
                    if (versionMatch) {
                        const cleanName = versionMatch[1];
                        const versionInName = versionMatch[2];
                        console.log(`Regex matched: ${cleanName} | ${versionInName}`);
                        // We trust the regex more than the metadata version for splitting
                        name = cleanName;
                    }
                    else if (name.endsWith(versionStr)) {
                        // Strategy 2: Simple string suffix check
                        // e.g. "ModName-1.0.0" ends with "1.0.0" -> "ModName-" -> "ModName"
                        // We need to handle the hyphen
                        console.log(`Suffix matched version ${versionStr}`);
                        const withoutVersion = name.substring(0, name.length - versionStr.length);
                        if (withoutVersion.endsWith('-')) {
                            name = withoutVersion.substring(0, withoutVersion.length - 1);
                        }
                        else {
                            name = withoutVersion;
                        }
                    }
                    else {
                        console.log(`No version suffix detected in "${name}"`);
                    }
                    console.log(`Final clean name: "${name}"`);
                    return {
                        name: name,
                        version: versionStr,
                        enabled: m.enabled
                    };
                })
            };
        }
        else {
            throw new Error('Invalid profile file: export.r2x not found');
        }
    }
    catch (error) {
        console.error('File import failed', error);
        throw error;
    }
});
ipcMain.handle('delete-profile-folder', async (_, profileId) => {
    const profileDir = path.join(app.getPath('userData'), 'profiles', profileId);
    if (fs.existsSync(profileDir)) {
        fs.rmSync(profileDir, { recursive: true, force: true });
    }
    return true;
});
ipcMain.handle('open-mod-folder', async (_, { profileId, modName }) => {
    const pluginsDir = path.join(app.getPath('userData'), 'profiles', profileId, 'BepInEx', 'plugins');
    if (!fs.existsSync(pluginsDir)) {
        const profileDir = path.join(app.getPath('userData'), 'profiles', profileId);
        if (fs.existsSync(profileDir)) {
            shell.openPath(profileDir);
            return;
        }
        return;
    }
    try {
        const files = fs.readdirSync(pluginsDir);
        // Try to find a folder that includes the mod name (case insensitive)
        const modFolder = files.find(f => f.toLowerCase().includes(modName.toLowerCase()));
        if (modFolder) {
            const fullPath = path.join(pluginsDir, modFolder);
            shell.openPath(fullPath);
        }
        else {
            // Fallback: open the plugins folder
            shell.openPath(pluginsDir);
        }
    }
    catch (e) {
        console.error('Error opening mod folder:', e);
        shell.openPath(pluginsDir);
    }
});
ipcMain.handle('export-profile', async (_, profileId) => {
    try {
        // Get profile data
        const p = getProfilesPath();
        const profiles = JSON.parse(fs.readFileSync(p, 'utf-8'));
        const profile = profiles.find((pr) => pr.id === profileId);
        if (!profile)
            throw new Error('Profile not found');
        // Create export.r2x content
        const exportData = {
            profileName: profile.name,
            mods: profile.mods.map((m) => {
                // Strip version from fullName to get clean name (Namespace-Name)
                // fullName is usually "Namespace-Name-Version"
                const versionStr = m.versionNumber;
                let cleanName = m.fullName;
                if (cleanName.endsWith(`-${versionStr}`)) {
                    cleanName = cleanName.substring(0, cleanName.length - versionStr.length - 1);
                }
                return {
                    name: cleanName,
                    version: {
                        major: parseInt(versionStr.split('.')[0]),
                        minor: parseInt(versionStr.split('.')[1]),
                        patch: parseInt(versionStr.split('.')[2])
                    },
                    enabled: m.enabled
                };
            })
        };
        const yamlContent = yaml.dump(exportData);
        // Create temp dir for zip
        const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'r2modmac-export-'));
        const exportPath = path.join(tempDir, 'export.r2x');
        fs.writeFileSync(exportPath, yamlContent);
        // Zip it
        const zipPath = path.join(tempDir, `${profile.name}.r2z`);
        // Use zip command (macOS/Linux)
        await new Promise((resolve, reject) => {
            exec(`cd "${tempDir}" && zip -r "${zipPath}" export.r2x`, (err) => {
                if (err)
                    reject(err);
                else
                    resolve();
            });
        });
        // Show save dialog
        const { filePath } = await dialog.showSaveDialog({
            title: 'Export Profile',
            defaultPath: `${profile.name}.r2z`,
            filters: [{ name: 'r2modman Profile', extensions: ['r2z'] }]
        });
        if (filePath) {
            fs.copyFileSync(zipPath, filePath);
            return { success: true, path: filePath };
        }
        return { success: false };
    }
    catch (error) {
        console.error('Export failed', error);
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
        title: 'r2modmac',
        webPreferences: {
            preload: path.join(__dirname, 'preload.js'),
            contextIsolation: true,
            nodeIntegration: false,
            sandbox: false,
        },
        width: 1200,
        height: 800,
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