import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import App from './App.tsx'
import { tauriAPI } from './tauriAdapter'

// Inject Tauri API adapter
window.ipcRenderer = tauriAPI;

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
