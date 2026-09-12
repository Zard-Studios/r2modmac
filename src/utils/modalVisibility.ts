/**
 * Preferences is a persistent workspace dialog, while an available update is
 * an interrupting dialog. Do not render both backdrops at once: equal stacking
 * contexts previously left the update hidden behind Preferences.
 */
export function shouldShowPreferencesModal(
    preferencesRequested: boolean,
    updateVisible: boolean,
): boolean {
    return preferencesRequested && !updateVisible;
}
