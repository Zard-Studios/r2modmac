import { useEffect, useState } from 'react';
import { PathCensor } from '../ui/PathCensor';
import { AppIcon } from '../ui/icons';
import { runningOnLinux, runningOnMacOS } from '../../utils/platformUtils';
import { PREFERENCE_ICON_COLORS, themedPreferenceIconStyle } from '../../utils/preferencesIconColors';
import { useThemeStore } from '../../store/useThemeStore';
import { isStockThemeId } from '../../utils/themePresets';

type SteamStatus = Awaited<ReturnType<typeof window.ipcRenderer.getSteamSetupStatus>>;

/** The Steam mark, from the simple-icons set (CC0). */
export function SteamLogo({ className = 'h-5 w-5' }: { className?: string }) {
    return (
        <svg className={className} viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
            <path d="M11.979 0C5.678 0 .511 4.86.022 11.037l6.432 2.658c.545-.371 1.203-.59 1.912-.59.063 0 .125.004.188.006l2.861-4.142V8.91c0-2.495 2.028-4.524 4.524-4.524 2.494 0 4.524 2.031 4.524 4.527s-2.03 4.525-4.524 4.525h-.105l-4.076 2.911c0 .052.004.105.004.159 0 1.875-1.515 3.396-3.39 3.396-1.635 0-3.016-1.173-3.331-2.727L.436 15.27C1.862 20.307 6.486 24 11.979 24c6.627 0 11.999-5.373 11.999-12S18.605 0 11.979 0zM7.54 18.21l-1.473-.61c.262.543.714.999 1.314 1.25 1.297.539 2.793-.076 3.332-1.375.263-.63.264-1.319.005-1.949s-.75-1.121-1.377-1.383c-.624-.26-1.29-.249-1.878-.03l1.523.63c.956.4 1.409 1.5 1.009 2.455-.397.957-1.497 1.41-2.454 1.012H7.54zm11.415-9.303c0-1.662-1.353-3.015-3.015-3.015-1.665 0-3.015 1.353-3.015 3.015 0 1.665 1.35 3.015 3.015 3.015 1.663 0 3.015-1.35 3.015-3.015zm-5.273-.005c0-1.252 1.013-2.266 2.265-2.266 1.249 0 2.266 1.014 2.266 2.266 0 1.251-1.017 2.265-2.266 2.265-1.253 0-2.265-1.014-2.265-2.265z" />
        </svg>
    );
}

/**
 * A small wine glass for the corner of the Steam logo, so Steam for Wine does
 * not look like Steam for macOS. It is a Preferences icon like the others, so
 * custom themes can recolour it.
 */
function WineBadge() {
    const hasThemePalette = useThemeStore(
        (state) => state.preview !== null || !isStockThemeId(state.activeFileName)
    );
    return (
        <span
            className={`absolute -bottom-1 -right-1 flex h-[18px] w-[18px] items-center justify-center rounded-full border border-gray-700 bg-gray-800 ${PREFERENCE_ICON_COLORS.wine}`}
            style={themedPreferenceIconStyle('wine', hasThemePalette)}
        >
            <AppIcon name="wine" className="h-3 w-3" strokeWidth={2} />
        </span>
    );
}

interface PathRowProps {
    title: string;
    /** One short line, or nothing. */
    detail?: string;
    /** Where Steam usually is; shown as a tooltip and, when missing, as the line itself. */
    usualPath: string;
    /** The folder the user picked by hand. Empty when detection is in charge. */
    value: string;
    /** Where it was found on its own. Shown when there is no `value`. */
    detected?: string[];
    /** `ok` is a green badge, `none` a muted one. */
    badge?: { label: string; ok: boolean };
    /** Draw a wine glass on the Steam logo: the Steam inside a Wine bottle. */
    wine?: boolean;
    onPick: () => void;
    onReset: () => void;
}

const smallButton = 'rounded-lg border border-gray-600 px-3 py-1 text-[13px] font-medium text-gray-200 transition-colors hover:border-gray-500 hover:bg-gray-700';

function PathRow({ title, detail, usualPath, value, detected = [], badge, wine, onPick, onReset }: PathRowProps) {
    const missing = badge && !badge.ok && !value;
    // A folder picked by hand wins; otherwise show where it was found.
    const shown = value || detected[0] || '';
    const others = value ? 0 : Math.max(0, detected.length - 1);
    return (
        <div className="p-4 flex items-center gap-4">
            <div className="relative w-9 h-9 rounded-xl flex items-center justify-center bg-gray-700/60 border border-gray-700 text-gray-200 flex-shrink-0">
                <SteamLogo />
                {wine && <WineBadge />}
            </div>
            <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2">
                    <p className="text-[15px] font-medium text-white">{title}</p>
                    {badge && (
                        <span className={`rounded-full px-2 py-0.5 text-[11px] font-medium ${badge.ok ? 'bg-green-500/15 text-green-400' : 'bg-gray-700 text-gray-400'}`}>
                            {badge.label}
                        </span>
                    )}
                </div>
                {shown ? (
                    <div className="flex min-w-0 items-baseline gap-2">
                        <PathCensor path={shown} className="block min-w-0 truncate text-[13px] text-gray-400" />
                        {others > 0 && <span className="shrink-0 text-[12px] text-gray-500">+{others} more</span>}
                    </div>
                ) : missing ? (
                    <p className="truncate text-[13px] text-gray-500">{usualPath}</p>
                ) : detail ? (
                    <p className="truncate text-[13px] text-gray-400">{detail}</p>
                ) : null}
                {shown && detail && <p className="truncate text-[12px] text-gray-500">{detail}</p>}
            </div>
            <div className="flex shrink-0 items-center gap-2">
                {value && (
                    <button type="button" onClick={onReset} title="Go back to automatic detection" className={`${smallButton} text-gray-400`}>
                        Auto
                    </button>
                )}
                <button type="button" onClick={onPick} title={`Usually ${usualPath}`} className={smallButton}>
                    {value ? 'Change' : 'Choose…'}
                </button>
            </div>
        </div>
    );
}

function LibrariesRow({ libraries, expanded, onToggle, onAdd, onRemove }: {
    libraries: SteamStatus['libraries'];
    expanded: boolean;
    onToggle: (next: boolean) => void;
    onAdd: () => void;
    onRemove: (path: string) => void;
}) {
    return (
        <div>
            <button
                type="button"
                aria-expanded={expanded}
                aria-controls="preference-libraries-details"
                onClick={() => onToggle(!expanded)}
                className="flex w-full items-center justify-between gap-4 p-4 text-left transition-colors hover:bg-surface-hover"
            >
                <p className="text-[15px] font-medium text-white">Libraries</p>
                <span className="flex shrink-0 items-center gap-3">
                    <span className="text-xs font-medium text-gray-400">{libraries.length}</span>
                    <svg className={`h-4 w-4 text-gray-400 transition-transform duration-200 ${expanded ? 'rotate-180' : ''}`} fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="m6 9 6 6 6-6" />
                    </svg>
                </span>
            </button>
            <div
                id="preference-libraries-details"
                className={`grid transition-[grid-template-rows,opacity] duration-300 ease-[cubic-bezier(0.4,0,0.2,1)] ${expanded ? 'grid-rows-[1fr] opacity-100' : 'grid-rows-[0fr] opacity-0'}`}
                aria-hidden={!expanded}
                inert={!expanded}
            >
                <div className="min-h-0 overflow-hidden">
                    <div className="space-y-3 px-4 pb-4">
                        {libraries.length > 0 && (
                            <ul className="space-y-1.5">
                                {libraries.map(library => (
                                    <li key={library.path} className="flex items-center gap-2 rounded-lg bg-gray-900/60 border border-gray-700/60 px-3 py-2">
                                        <PathCensor path={library.path} className="min-w-0 flex-1 truncate text-[13px] text-gray-300" />
                                        {library.mounted
                                            ? <span className="text-[12px] text-gray-500 shrink-0">{library.games}</span>
                                            : <span className="rounded-full bg-yellow-500/15 px-2 py-0.5 text-[11px] text-fg-warning shrink-0">Disconnected</span>}
                                        {library.added_by_user && (
                                            <button type="button" onClick={() => onRemove(library.path)} title="Remove" aria-label="Remove this library" className="shrink-0 rounded-md px-1.5 text-gray-400 hover:bg-gray-700 hover:text-white">×</button>
                                        )}
                                    </li>
                                ))}
                            </ul>
                        )}
                        <button type="button" onClick={onAdd} title="Add a folder that contains steamapps" className={smallButton}>
                            Add folder…
                        </button>
                    </div>
                </div>
            </div>
        </div>
    );
}

/**
 * Where Steam lives, shown only for the setups this machine can have.
 * Everything is detected on its own; a folder here is just an override.
 * Games outside Steam (Epic, GOG, ...) are pointed at in Profile Settings.
 */
const EXPANDED_KEY = 'r2modmac:preferences:game-libraries-expanded';

function storedExpanded(): boolean {
    try {
        return window.localStorage.getItem(EXPANDED_KEY) === 'true';
    } catch {
        return false;
    }
}

export function GameLocationsSection({ isOpen, revealed = false }: { isOpen: boolean; revealed?: boolean }) {
    const [expanded, setExpanded] = useState(() => storedExpanded());
    // Arriving from a "Steam locations" link means the user wants to see it.
    useEffect(() => {
        if (isOpen && revealed) setExpanded(true);
    }, [isOpen, revealed]);
    const changeExpanded = (next: boolean) => {
        setExpanded(next);
        try {
            window.localStorage.setItem(EXPANDED_KEY, String(next));
        } catch {
            // View state is non-critical and must never block Preferences.
        }
    };

    const isMac = runningOnMacOS();
    const isLinux = runningOnLinux();
    const [nativePath, setNativePath] = useState('');
    const [wineOrWindowsPath, setWineOrWindowsPath] = useState('');
    const [status, setStatus] = useState<SteamStatus | null>(null);

    useEffect(() => {
        if (!isOpen) return;
        let cancelled = false;
        window.ipcRenderer.getSettings().then(settings => {
            if (cancelled) return;
            const legacy = settings.steam_path ?? '';
            const legacyIsWine = /drive_c|crossover|wine/i.test(legacy);
            setNativePath(settings.mac_steam_path || (legacy && !legacyIsWine ? legacy : ''));
            setWineOrWindowsPath(settings.windows_steam_path || (legacyIsWine || !isMac ? legacy : ''));
        }).catch(error => console.error('Failed to load Steam settings', error));
        {
            window.ipcRenderer.getSteamSetupStatus()
                .then(next => { if (!cancelled) setStatus(next); })
                .catch(() => { if (!cancelled) setStatus(null); });
        }
        return () => { cancelled = true; };
    }, [isOpen, isMac]);

    const refreshStatus = () => {
        window.ipcRenderer.getSteamSetupStatus()
            .then(next => setStatus(next))
            .catch(() => setStatus(null));
    };

    const addLibrary = async () => {
        const picked = await window.ipcRenderer.selectFolder().catch(() => null);
        if (!picked) return;
        // Accept the steamapps (or steamapps/common) folder too, and step up to the library.
        const library = picked.replace(/[\\/]+$/, '').replace(/[\\/]steamapps([\\/]common)?$/i, '');
        try {
            const current = await window.ipcRenderer.getSettings();
            const existing = current.extra_steam_libraries ?? [];
            if (existing.includes(library)) return;
            await window.ipcRenderer.saveSettings({ ...current, extra_steam_libraries: [...existing, library] });
            refreshStatus();
        } catch (error) {
            await window.ipcRenderer.alert('Could not add the library', String(error));
        }
    };

    const removeLibrary = async (path: string) => {
        try {
            const current = await window.ipcRenderer.getSettings();
            await window.ipcRenderer.saveSettings({
                ...current,
                extra_steam_libraries: (current.extra_steam_libraries ?? []).filter(entry => entry !== path),
            });
            refreshStatus();
        } catch (error) {
            await window.ipcRenderer.alert('Could not remove the library', String(error));
        }
    };

    const persist = async (kind: 'native' | 'windows', value: string) => {
        const path = value.trim() || null;
        try {
            const current = await window.ipcRenderer.getSettings();
            await window.ipcRenderer.saveSettings(kind === 'native'
                ? { ...current, mac_steam_path: path }
                : { ...current, windows_steam_path: path, steam_path: path });
        } catch (error) {
            console.error('Failed to save Steam folder', error);
            await window.ipcRenderer.alert('Could not save the Steam folder', String(error));
        }
    };

    const pick = async (kind: 'native' | 'windows') => {
        const picked = await window.ipcRenderer.selectFolder().catch(() => null);
        if (!picked) return;
        (kind === 'native' ? setNativePath : setWineOrWindowsPath)(picked);
        await persist(kind, picked);
    };

    const reset = async (kind: 'native' | 'windows') => {
        (kind === 'native' ? setNativePath : setWineOrWindowsPath)('');
        await persist(kind, '');
    };

    return (
        <div id="preference-game-locations" className="space-y-3">
            <h3 className="text-xs font-semibold text-gray-400 uppercase tracking-widest px-1">Game locations</h3>
            <div className="bg-gray-800 border border-gray-700 rounded-2xl divide-y divide-gray-700/50 overflow-hidden">
                {isMac ? (
                    <>
                        <PathRow
                            title="Steam for macOS"
                            detail={status?.notproton ? `NotProton ${status.notproton.replace(/^notproton-?/i, '')}` : undefined}
                            usualPath="~/Library/Application Support/Steam"
                            value={nativePath}
                            detected={status?.native_steam_path ? [status.native_steam_path] : []}
                            badge={status ? { label: status.native_steam_found ? 'Found' : 'Not found', ok: status.native_steam_found } : undefined}
                            onPick={() => void pick('native')}
                            onReset={() => void reset('native')}
                        />
                        <PathRow
                            title="Steam for Wine"
                            detail={status?.bottle_steam_paths?.length ? undefined : 'CrossOver, Whisky and other bottles'}
                            usualPath="…/drive_c/Program Files (x86)/Steam"
                            value={wineOrWindowsPath}
                            detected={status?.bottle_steam_paths ?? []}
                            badge={status ? { label: status.bottle_steam_count > 0 ? 'Found' : 'Not found', ok: status.bottle_steam_count > 0 } : undefined}
                            wine
                            onPick={() => void pick('windows')}
                            onReset={() => void reset('windows')}
                        />
                    </>
                ) : (
                    <PathRow
                        title={isLinux ? 'Steam' : 'Steam for Windows'}
                        usualPath={isLinux ? '~/.local/share/Steam' : 'C:\\Program Files (x86)\\Steam'}
                        value={wineOrWindowsPath}
                        detected={status?.native_steam_path ? [status.native_steam_path] : []}
                        badge={status ? { label: status.native_steam_found ? 'Found' : 'Not found', ok: status.native_steam_found } : undefined}
                        onPick={() => void pick('windows')}
                        onReset={() => void reset('windows')}
                    />
                )}
                <LibrariesRow libraries={status?.libraries ?? []} expanded={expanded} onToggle={changeExpanded} onAdd={() => void addLibrary()} onRemove={path => void removeLibrary(path)} />
            </div>
        </div>
    );
}
