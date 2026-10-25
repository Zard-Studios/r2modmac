import { useEffect, useState, type ReactNode } from 'react';
import type { Profile, ProfileArchitecture } from '../../types/profile';
import type { ArchitectureStatus } from '../../types/electron';
import { AppIcon } from '../ui/icons';
import { useProfileStore } from '../../store/useProfileStore';
import {
    architectureChoices,
    architectureOf,
    architectureStatusLine,
    canTryNativeAgain,
    isArchitectureSettingAvailable,
    shouldOpenArchitectureSection,
} from '../../utils/profileArchitecture';

/**
 * The profile's closed "Advanced" section: the settings most players never
 * need (children, such as BepInEx isolation) and, on Apple Silicon Macs, the
 * Architecture setting.
 *
 * Automatic is what everyone gets and needs nothing from the player, so the
 * section stays shut and says nothing until the profile is off that path: a
 * choice was made, or a crash moved it to Rosetta. Then it opens by itself
 * with the reason and a way back.
 */
export function ProfileAdvancedSection({ profile, isOpen, showArchitecture, children }: {
    profile: Profile;
    isOpen: boolean;
    showArchitecture: boolean;
    children?: ReactNode;
}) {
    const updateProfile = useProfileStore((state) => state.updateProfile);
    const mode = architectureOf(profile);
    const [status, setStatus] = useState<ArchitectureStatus | null>(null);
    // What the player opened or closed counts for this profile and this opening
    // of the window only; a new one starts from what the profile calls for.
    const sessionKey = `${profile.id}:${isOpen}`;
    const [userChoice, setUserChoice] = useState<{ key: string; open: boolean } | null>(null);
    const [busy, setBusy] = useState(false);

    // The mode is passed along because profiles are saved a moment after a
    // change, so the file the backend would read can still hold the old one.
    const refresh = () => window.ipcRenderer
        .getProfileArchitectureStatus(profile.gameIdentifier, profile.id, mode)
        .then(setStatus)
        .catch(() => setStatus(null));

    useEffect(() => {
        if (!isOpen || !showArchitecture) return;
        let current = true;
        window.ipcRenderer
            .getProfileArchitectureStatus(profile.gameIdentifier, profile.id, mode)
            .then((next) => { if (current) setStatus(next); })
            .catch(() => { if (current) setStatus(null); });
        return () => { current = false; };
    }, [isOpen, showArchitecture, profile.id, profile.gameIdentifier, mode]);

    const architecture = showArchitecture && isArchitectureSettingAvailable(status) ? status : null;
    if (!architecture && !children) return null;

    const expanded = userChoice?.key === sessionKey
        ? userChoice.open
        : architecture !== null && shouldOpenArchitectureSection(architecture, mode);
    const statusLine = architecture && architectureStatusLine(architecture, mode);
    const choose = (next: ProfileArchitecture) => {
        if (next !== mode) updateProfile(profile.id, { architecture: next });
    };
    const tryAgain = async () => {
        setBusy(true);
        try {
            await window.ipcRenderer.clearProfileArchitecturePin(profile.gameIdentifier, profile.id);
            await refresh();
        } catch (error) {
            await window.ipcRenderer.alert('Could not reset this profile', String(error));
        } finally {
            setBusy(false);
        }
    };

    return (
        <div className="mb-6 border-t border-gray-700 pt-5">
            <button
                type="button"
                onClick={() => setUserChoice({ key: sessionKey, open: !expanded })}
                aria-expanded={expanded}
                className="flex w-full items-center gap-1.5 text-left text-sm font-medium text-gray-300 transition-colors hover:text-white focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-500"
            >
                <AppIcon name="chevron" className={`h-4 w-4 transition-transform ${expanded ? 'rotate-90' : ''}`} />
                Advanced
            </button>
            {expanded && (
                <div className="mt-4 space-y-5">
                    {children}
                    {architecture && (
                        <div className="space-y-3">
                        <div className="flex items-center gap-1.5">
                            <span className="text-sm font-medium text-gray-200">Architecture</span>
                            <span className="group relative shrink-0">
                                <button
                                    type="button"
                                    aria-label="About Architecture"
                                    aria-describedby="architecture-help"
                                    className="flex h-6 w-6 items-center justify-center rounded-md text-gray-400 transition-colors hover:bg-gray-700 hover:text-white focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-500"
                                >
                                    <AppIcon name="help" className="h-4 w-4" />
                                </button>
                                <span
                                    id="architecture-help"
                                    role="tooltip"
                                    className="pointer-events-none invisible absolute bottom-full left-0 z-20 mb-2 w-60 rounded-lg border border-gray-600 bg-gray-800 p-3 text-left text-xs leading-relaxed text-gray-200 opacity-0 shadow-lg transition-opacity group-hover:visible group-hover:opacity-100 group-focus-within:visible group-focus-within:opacity-100"
                                >
                                    Automatic picks the best option for this game and these mods. Change this only if the game crashes. Applies when launching with mods.
                                </span>
                            </span>
                        </div>
                        <div className="grid grid-cols-3 gap-2" role="radiogroup" aria-label="Architecture">
                            {architectureChoices(architecture).map((choice) => {
                                const selected = choice.id === mode;
                                const disabled = choice.disabledReason !== null && !selected;
                                return (
                                    <button
                                        key={choice.id}
                                        type="button"
                                        role="radio"
                                        aria-checked={selected}
                                        disabled={disabled}
                                        title={choice.disabledReason ?? undefined}
                                        onClick={() => choose(choice.id)}
                                        className={`rounded-xl border-2 px-2 py-2.5 text-xs font-medium transition-all focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-500 ${
                                            selected
                                                ? 'border-blue-500 bg-blue-500/20 text-white'
                                                : 'border-gray-700 bg-gray-900/50 text-gray-400 hover:border-gray-500 hover:bg-gray-800/50 hover:text-gray-200 disabled:cursor-not-allowed disabled:opacity-40 disabled:hover:border-gray-700 disabled:hover:bg-gray-900/50 disabled:hover:text-gray-400'
                                        }`}
                                    >
                                        {choice.label}
                                    </button>
                                );
                            })}
                        </div>
                        {statusLine && (
                            <p className="text-xs leading-relaxed text-gray-400">{statusLine}</p>
                        )}
                        {canTryNativeAgain(architecture, mode) && (
                            <button
                                type="button"
                                onClick={tryAgain}
                                disabled={busy}
                                className="text-xs font-medium text-blue-400 transition-colors hover:text-blue-300 focus:outline-none focus-visible:underline disabled:cursor-wait disabled:opacity-50"
                            >
                                Try Apple Silicon again
                            </button>
                        )}
                        {architecture.shimHint && mode !== 'rosetta' && (
                            <p className="text-xs leading-relaxed text-gray-500">{architecture.shimHint}</p>
                        )}
                    </div>
                    )}
                </div>
            )}
        </div>
    );
}
