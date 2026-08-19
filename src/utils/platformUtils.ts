/** True when r2modmac itself is running on Windows, whatever the profile targets. */
export function runningOnWindows(): boolean {
    return navigator.userAgent.includes('Win');
}

/** Returns the platform-appropriate label for the "reveal in file manager" action. */
export function revealInFileManagerLabel(): string {
    const ua = navigator.userAgent;
    if (ua.includes('Win')) return 'Reveal in Explorer';
    if (ua.includes('Mac')) return 'Reveal in Finder';
    return 'Reveal in File Manager';
}

/** Returns the platform-appropriate label for the "open folder" action. */
export function openFolderLabel(): string {
    const ua = navigator.userAgent;
    if (ua.includes('Win')) return 'Open in Explorer';
    if (ua.includes('Mac')) return 'Open in Finder';
    return 'Open Folder';
}
