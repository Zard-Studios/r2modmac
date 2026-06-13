const EXCLUDED_NAMES = new Set([
    'steam', 'steamapps', 'common', 'library', 'application support', 
    'drive_c', 'program files', 'program files (x86)', 'users', 'home', 
    'volumes', 'media', 'mnt', 'desktop', 'documents', 'downloads', 
    'pictures', 'music', 'videos', 'appdata', 'local', 'roaming', 'microsoft'
]);

function getEffectiveUsername(path: string | null | undefined, storeUsername: string | null): string | null {
    if (storeUsername) return storeUsername;
    if (!path) return null;
    const match = path.match(/(?:\\|\/)(?:Users|home)(?:\\|\/)([^\\/]+)/i);
    return match ? match[1] : null;
}

export function censorPath(path: string | null | undefined, username: string | null): string {
    if (!path) return '';
    const effUsername = getEffectiveUsername(path, username);
    if (!effUsername) return path;

    const lowerUsername = effUsername.toLowerCase();
    
    // Split the path by / and \ while preserving the delimiters in the resulting array
    const parts = path.split(/([\\/])/);
    
    const censoredParts = parts.map(part => {
        if (part === '/' || part === '\\') return part;
        
        const lowerPart = part.toLowerCase();
        if (!lowerPart || EXCLUDED_NAMES.has(lowerPart)) return part;
        
        // Match conditions:
        // 1. Exact case-insensitive match
        // 2. Substring match if the segment is at least 3 characters long
        const isMatch = lowerPart === lowerUsername || (
            lowerPart.length >= 3 && (
                lowerUsername.includes(lowerPart) || 
                lowerPart.includes(lowerUsername)
            )
        );
        
        if (isMatch) {
            return '*'.repeat(effUsername ? effUsername.length : part.length);
        }
        return part;
    });
    
    return censoredParts.join('');
}

export function uncensorPath(
    censoredPath: string | null | undefined, 
    originalPath: string | null | undefined
): string {
    if (!censoredPath) return '';
    if (!originalPath) return censoredPath;
    
    const censoredParts = censoredPath.split(/([\\/])/);
    const originalParts = originalPath.split(/([\\/])/);
    
    const uncensoredParts = censoredParts.map((part, index) => {
        if (/^\*+$/.test(part) || part === '[Censored]' || part === '••••••••') {
            return originalParts[index] !== undefined ? originalParts[index] : part;
        }
        return part;
    });
    
    return uncensoredParts.join('');
}
