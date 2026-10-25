import type { ArchitectureStatus } from '../types/electron';
import type { ProfileArchitecture } from '../types/profile';

/**
 * What the profile's Architecture setting shows (macOS, Apple Silicon only).
 *
 * The backend decides what is possible (`get_profile_architecture_status`);
 * this only turns its answer into what the player sees, so the rules for what
 * is hidden, disabled and said can be tested without a window.
 */

export const ARCHITECTURE_LABELS: Record<ProfileArchitecture, string> = {
    auto: 'Automatic (recommended)',
    native: 'Apple Silicon',
    rosetta: 'Intel (Rosetta)',
};

/** A profile without the field is automatic. */
export function architectureOf(profile: { architecture?: ProfileArchitecture } | null | undefined): ProfileArchitecture {
    return profile?.architecture === 'native' || profile?.architecture === 'rosetta'
        ? profile.architecture
        : 'auto';
}

/** The setting exists only where a Mac can run the game either way, or has to say why not. */
export function isArchitectureSettingAvailable(status: ArchitectureStatus | null | undefined): status is ArchitectureStatus {
    return !!status && status.applicable && status.hostAppleSilicon;
}

/**
 * The section stays closed for a player who never touched it. It opens by
 * itself only when something is off the default path: a choice was made, or a
 * crash moved the profile to Rosetta and the player should know why.
 */
export function shouldOpenArchitectureSection(
    status: ArchitectureStatus | null | undefined,
    mode: ProfileArchitecture,
): boolean {
    if (!isArchitectureSettingAvailable(status)) return false;
    return mode !== 'auto' || status.pinned !== null;
}

export interface ArchitectureChoice {
    id: ProfileArchitecture;
    label: string;
    /** Why it can't be picked, in words for a player; null when it can. */
    disabledReason: string | null;
}

export function architectureChoices(status: ArchitectureStatus): ArchitectureChoice[] {
    return [
        { id: 'auto', label: ARCHITECTURE_LABELS.auto, disabledReason: null },
        {
            id: 'native',
            label: ARCHITECTURE_LABELS.native,
            disabledReason: status.native.available ? null : status.native.reason ?? 'Not available for this game.',
        },
        {
            id: 'rosetta',
            label: ARCHITECTURE_LABELS.rosetta,
            disabledReason: status.rosetta.available ? null : status.rosetta.reason ?? 'Not available for this game.',
        },
    ];
}

export function formatPinDate(sinceSeconds: number, locale?: string): string {
    return new Date(sinceSeconds * 1000).toLocaleDateString(locale, { day: 'numeric', month: 'short' });
}

/** One line under the control, or nothing: an untouched profile says nothing at all. */
export function architectureStatusLine(
    status: ArchitectureStatus,
    mode: ProfileArchitecture,
    formatDate: (sinceSeconds: number) => string = formatPinDate,
): string | null {
    if (mode === 'native') {
        return 'Always uses Apple Silicon. If the game crashes, switch back to Automatic.';
    }
    if (mode === 'rosetta') {
        return 'Always uses Intel (Rosetta).';
    }
    if (status.pinned) {
        const date = formatDate(status.pinned.since);
        return status.effective === 'rosetta'
            ? `Using Intel (Rosetta) since the game closed unexpectedly on Apple Silicon on ${date}.`
            : `The game closed unexpectedly on Apple Silicon on ${date}, but Rosetta isn't available, so it keeps using Apple Silicon.`;
    }
    return null;
}

/** "Try Apple Silicon again" only makes sense while a crash is what holds the profile on Rosetta. */
export function canTryNativeAgain(status: ArchitectureStatus, mode: ProfileArchitecture): boolean {
    return mode === 'auto' && status.pinned !== null && status.native.available;
}
