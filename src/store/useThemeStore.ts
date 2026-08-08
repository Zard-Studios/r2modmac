import { create } from 'zustand';

import { applyTheme, normalizeTheme, type Theme } from '../utils/theme.ts';
import { findPreset, isBuiltinId, type ThemePreset } from '../utils/themePresets.ts';
import type { ThemeSummary } from '../types/electron';

/** A file from disk, filled out with defaults for anything it left unset. */
export function summaryToTheme(summary: ThemeSummary): Theme {
    return normalizeTheme({
        name: summary.name,
        author: summary.author ?? undefined,
        colors: summary.colors,
        options:
            typeof summary.options?.auto_contrast === 'boolean'
                ? { autoContrast: summary.options.auto_contrast }
                : undefined,
        // Spread rather than list the fields. Copying them by hand is how the
        // layout settings kept reverting: the backend gained `fit` and the
        // offsets, and this function quietly dropped them on the way past.
        // Anything added later now flows through without a second edit here.
        backgroundImage: summary.background_image
            ? { ...summary.background_image }
            : null,
    });
}

/**
 * Background pictures are read from disk as data URLs, which is slow enough to
 * be worth doing once. Keyed by the theme-relative path, so switching back to a
 * theme repaints instantly.
 */
const imageCache = new Map<string, string | null>();

async function loadBackgroundImage(theme: Theme | null): Promise<string | null> {
    const path = theme?.backgroundImage?.path;
    if (!path) return null;
    if (imageCache.has(path)) return imageCache.get(path) ?? null;
    try {
        const url = await window.ipcRenderer.readThemeImage(path);
        imageCache.set(path, url);
        return url;
    } catch (error) {
        console.error('Failed to load the theme background image', error);
        imageCache.set(path, null);
        return null;
    }
}

/** Drop a cached picture so the next paint re-reads it from disk. */
export function forgetBackgroundImage(path?: string | null): void {
    if (path) imageCache.delete(path);
    else imageCache.clear();
}

interface ThemeState {
    /** User themes from the themes folder. */
    themes: ThemeSummary[];
    /** Selected theme: a file name, a `builtin:` preset id, or null for stock. */
    activeFileName: string | null;
    /**
     * Unsaved editor state. While the editor is open its colours are painted
     * over the whole app, so the preview is the app itself rather than a
     * swatch — you judge a theme by living in it for a moment.
     */
    preview: Theme | null;

    loadThemes: () => Promise<ThemeSummary[]>;
    setActive: (id: string | null) => Promise<void>;
    setPreview: (theme: Theme | null) => void;
    hydrate: (activeFileName: string | null) => Promise<void>;
    /** The theme currently on screen, whatever its source. */
    currentTheme: () => Theme | null;
}

/**
 * A theme file the app could not parse.
 *
 * Distinct from "no theme": a broken file must not repaint anything, because
 * every colour in it is missing and filling those from defaults would silently
 * drop the user back to the stock look mid-edit. The editor shows the parse
 * error instead, and the window keeps the colours it already had.
 */
const BROKEN = Symbol('broken-theme');

function resolveActive(
    themes: ThemeSummary[],
    activeFileName: string | null
): Theme | null | typeof BROKEN {
    if (!activeFileName) return null;
    if (isBuiltinId(activeFileName)) {
        const preset: ThemePreset | null = findPreset(activeFileName);
        // A preset id from an older build that no longer exists falls back to
        // stock rather than leaving the app on someone else's colours.
        return preset ? normalizeTheme(preset) : null;
    }
    const match = themes.find((t) => t.file_name === activeFileName);
    if (!match) return null;
    if (match.error) return BROKEN;
    return summaryToTheme(match);
}

export const useThemeStore = create<ThemeState>((set, get) => {
    /**
     * Decide what the window should look like and paint it.
     *
     * Preview wins over the saved selection so edits show immediately; when the
     * editor closes and clears the preview, the saved theme reappears on its own.
     */
    /** Data URL of the picture already on screen, so repaints can reuse it. */
    let paintedImageUrl: string | null = null;
    let paintedImagePath: string | null = null;

    const paint = () => {
        const { themes, activeFileName, preview } = get();
        const resolved = preview ?? resolveActive(themes, activeFileName);

        // A file that will not parse leaves the window exactly as it is. The
        // alternative — treating every colour as missing — repaints the stock
        // palette over the user's theme for what is usually a transient error.
        if (resolved === BROKEN) return;
        const theme = resolved;

        const path = theme?.backgroundImage?.path ?? null;
        // Hand the picture straight back when it has not changed, so dragging a
        // slider repaints colours without touching the megabyte-long data URL.
        const url = path && path === paintedImagePath ? paintedImageUrl : null;
        applyTheme(theme, document.documentElement, url);

        if (path && path !== paintedImagePath) {
            void loadBackgroundImage(theme).then((loaded) => {
                const latest = get();
                const current = latest.preview ?? resolveActive(latest.themes, latest.activeFileName);
                if (current === BROKEN) return;
                // Only paint if the theme has not moved on while we were reading.
                if (current?.backgroundImage?.path !== path) return;
                paintedImagePath = path;
                paintedImageUrl = loaded;
                applyTheme(current, document.documentElement, loaded);
            });
        } else if (!path) {
            paintedImagePath = null;
            paintedImageUrl = null;
        }
    };

    /**
     * Coalesce repaints to one per frame.
     *
     * Dragging inside the colour picker fires pointer events faster than the
     * screen updates, and each one would otherwise expand a full palette and
     * write ~90 custom properties.
     */
    let frame: number | null = null;
    const repaint = () => {
        if (typeof requestAnimationFrame !== 'function') { paint(); return; }
        if (frame !== null) return;
        frame = requestAnimationFrame(() => { frame = null; paint(); });
    };

    /**
     * Ease a whole-theme swap instead of cutting to it.
     *
     * Reserved for changing *which* theme is active. Live edits repaint many
     * times a second, and easing those would lag the colour picker behind the
     * cursor rather than making anything feel smoother.
     */
    const crossfade = (run: () => void) => {
        if (typeof document === 'undefined') { run(); return; }

        const doc = document as Document & {
            startViewTransition?: (callback: () => void) => { finished?: Promise<unknown> };
        };

        const root = doc.documentElement;

        /**
         * Land the whole palette in one frame.
         *
         * Components carry their own `transition-colors`, which otherwise fire
         * on the custom-property change and bring the theme in element by
         * element. Suppressing them for the swap is what makes it arrive all at
         * once — and costs nothing, since it removes animation work.
         */
        const swapAtOnce = () => {
            root.classList?.add('r2-theme-swapping');
            run();
            const release = () => root.classList?.remove('r2-theme-swapping');
            if (typeof requestAnimationFrame !== 'function') { release(); return; }
            // Two frames: one for the style change to be committed, one for it
            // to be painted, before element transitions are allowed back.
            requestAnimationFrame(() => requestAnimationFrame(release));
        };

        const reduced =
            typeof matchMedia === 'function' &&
            matchMedia('(prefers-reduced-motion: reduce)').matches;

        // Without View Transitions the swap is simply instant, which is still
        // the correct behaviour: everything changes together, just with no
        // cross-fade over the top.
        if (reduced || typeof doc.startViewTransition !== 'function') {
            swapAtOnce();
            return;
        }

        try {
            doc.startViewTransition(swapAtOnce);
        } catch {
            swapAtOnce();
        }
    };

    return {
        themes: [],
        activeFileName: null,
        preview: null,

        currentTheme: () => {
            const { themes, activeFileName, preview } = get();
            const resolved = preview ?? resolveActive(themes, activeFileName);
            return resolved === BROKEN ? null : resolved;
        },

        loadThemes: async () => {
            const themes = await window.ipcRenderer.listThemes();
            set({ themes });
            repaint();
            return themes;
        },

        setActive: async (id) => {
            set({ activeFileName: id });
            // Painted straight away rather than through the frame-coalescing
            // path: the transition class has to be in place for the same frame
            // that changes the colours, or there is nothing to ease from.
            crossfade(paint);
            try {
                await window.ipcRenderer.setActiveTheme(id);
            } catch (error) {
                console.error('Failed to save the selected theme', error);
            }
        },

        setPreview: (preview) => {
            set({ preview });
            repaint();
        },

        hydrate: async (activeFileName) => {
            set({ activeFileName });
            try {
                const themes = await window.ipcRenderer.listThemes();
                set({ themes });
            } catch (error) {
                console.error('Failed to load themes', error);
            }
            repaint();
        },
    };
});
