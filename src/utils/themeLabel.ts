import { findPreset, isBuiltinId, STOCK_THEME_ID } from './themePresets.ts';

/**
 * The name Preferences shows for the selected theme.
 *
 * The selection can be a built-in preset ("builtin:claudio-dark") as well as a
 * file in the themes folder, and a preset is in no file list. Looking only
 * through the files made every preset read "Default", even though its colours
 * were on screen.
 */
export function activeThemeLabel(
    files: ReadonlyArray<{ file_name: string; name: string }>,
    activeFileName: string | null
): string {
    if (!activeFileName || activeFileName === STOCK_THEME_ID) return 'Default';
    if (isBuiltinId(activeFileName)) return findPreset(activeFileName)?.name ?? 'Default';

    const file = files.find((entry) => entry.file_name === activeFileName);
    if (file) return file.name;

    // A file the list does not hold (yet): name it rather than claim the
    // stock theme is selected.
    return activeFileName.replace(/\.toml$/i, '');
}
