import { useRef, useEffect } from 'react';
import { useAppStore } from '../../store/useAppStore';
import { censorPath, uncensorPath } from '../../utils/pathCensorUtils';

interface PathCensorProps {
    path: string | null | undefined;
    className?: string;
}

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

export function PathCensor({ path, className = '' }: PathCensorProps) {
    const { streamMode, username } = useAppStore();

    if (!path) return null;

    if (!streamMode) {
        return <span className={className}>{path}</span>;
    }

    const effUsername = getEffectiveUsername(path, username);
    if (!effUsername) {
        return <span className={className}>{path}</span>;
    }

    const lowerUsername = effUsername.toLowerCase();
    const parts = path.split(/([\\/])/);

    return (
        <span className={className}>
            {parts.map((part, index) => {
                if (part === '/' || part === '\\') {
                    return <span key={index}>{part}</span>;
                }
                
                const lowerPart = part.toLowerCase();
                if (!lowerPart || EXCLUDED_NAMES.has(lowerPart)) {
                    return <span key={index}>{part}</span>;
                }
                
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
                    const censorLength = effUsername ? effUsername.length : part.length;
                    const censorStr = '*'.repeat(censorLength);
                    return (
                        <span key={index} className="stream-censor-blur mx-[2px]" title="Hidden by Stream Mode">
                            <span className="select-none pointer-events-none font-bold tracking-wider">{censorStr}</span>
                        </span>
                    );
                }
                
                return <span key={index}>{part}</span>;
            })}
        </span>
    );
}

interface CensoredInputProps extends Omit<React.InputHTMLAttributes<HTMLInputElement>, 'value' | 'onChange'> {
    value: string;
    onChange: (val: string) => void;
}

export function CensoredInput({ value, onChange, className = '', placeholder, ...props }: CensoredInputProps) {
    const { streamMode, username } = useAppStore();
    const inputRef = useRef<HTMLInputElement>(null);
    const overlayRef = useRef<HTMLDivElement>(null);

    const syncScroll = () => {
        if (inputRef.current && overlayRef.current) {
            overlayRef.current.scrollLeft = inputRef.current.scrollLeft;
        }
    };

    useEffect(() => {
        syncScroll();
    }, [value]);

    if (!streamMode) {
        return (
            <input
                type="text"
                value={value}
                onChange={(e) => onChange(e.target.value)}
                className={className}
                placeholder={placeholder}
                {...props}
            />
        );
    }

    const effUsername = username || (value ? (value.match(/(?:\\|\/)(?:Users|home)(?:\\|\/)([^\\/]+)/i)?.[1] || null) : null);
    const censorStr = effUsername ? '*'.repeat(effUsername.length) : '****';
    const censoredVal = censorPath(value, username);

    const parts = censoredVal.split(/([\\/])/);

    return (
        <div className="relative w-full flex-1 min-w-0">
            {/* Overlay */}
            <div
                ref={overlayRef}
                className="absolute inset-0 pointer-events-none flex items-center overflow-hidden whitespace-nowrap text-sm text-gray-300"
                style={{
                    paddingLeft: '12px',
                    paddingRight: '12px',
                    fontFamily: 'inherit',
                    fontSize: 'inherit',
                    lineHeight: '1.25rem',
                    boxSizing: 'border-box',
                    border: '1px solid transparent',
                }}
            >
                {parts.map((part, idx) => {
                    if (part === '/' || part === '\\') {
                        return <span key={idx} className="text-gray-500">{part}</span>;
                    }
                    if (part === censorStr) {
                        return (
                            <span key={idx} className="stream-censor-blur font-bold mx-[1px]">
                                <span className="text-white/90">{part}</span>
                            </span>
                        );
                    }
                    return <span key={idx}>{part}</span>;
                })}
            </div>

            {/* Real Input */}
            <input
                ref={inputRef}
                type="text"
                value={censoredVal}
                onChange={(e) => {
                    const rawVal = e.target.value;
                    const realVal = uncensorPath(rawVal, value);
                    onChange(realVal);
                }}
                onScroll={syncScroll}
                onSelect={syncScroll}
                onKeyDown={() => setTimeout(syncScroll, 0)}
                style={{
                    color: 'transparent',
                    caretColor: '#fff',
                }}
                className={className}
                placeholder={placeholder}
                {...props}
            />
        </div>
    );
}
