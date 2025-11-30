import { app, BrowserWindow } from 'electron'
import path from 'path'
import { fileURLToPath } from 'url'

const __filename = fileURLToPath(import.meta.url)
const __dirname = path.dirname(__filename)

process.env.DIST = path.join(__dirname, '../dist')
process.env.VITE_PUBLIC = app.isPackaged ? process.env.DIST : path.join(process.env.DIST, '../public')

if (!process.env.VITE_PUBLIC) throw new Error("VITE_PUBLIC is undefined")
if (!process.env.DIST) throw new Error("DIST is undefined")

let win: BrowserWindow | null

const VITE_DEV_SERVER_URL = process.env['VITE_DEV_SERVER_URL']

function createWindow() {
    win = new BrowserWindow({
        icon: path.join(process.env.VITE_PUBLIC as string, 'electron-vite.svg'),
        webPreferences: {
            preload: path.join(__dirname, 'preload.js'),
        },
        width: 1200,
        height: 800,
        titleBarStyle: 'hiddenInset', // Mac style
        vibrancy: 'under-window', // Mac blur effect
        visualEffectState: 'active',
    })

    // Test active push message to Console
    win.webContents.on('did-finish-load', () => {
        win?.webContents.send('main-process-message', (new Date).toLocaleString())
    })

    if (VITE_DEV_SERVER_URL) {
        win.loadURL(VITE_DEV_SERVER_URL)
    } else {
        if (!app.isPackaged) {
            win.loadURL('http://localhost:5173')
        } else {
            win.loadFile(path.join(process.env.DIST as string, 'index.html'))
        }
    }
}

app.on('window-all-closed', () => {
    if (process.platform !== 'darwin') {
        app.quit()
    }
})

app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
        createWindow()
    }
})

app.whenReady().then(createWindow)
