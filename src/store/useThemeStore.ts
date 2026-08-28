import { create } from 'zustand';

import { applyTheme, applyThemeBackgroundImage, normalizeTheme, type Theme } from '../utils/theme.ts';
import { findPreset, isBuiltinId, type ThemePreset } from '../utils/themePresets.ts';
import type { ThemeSummary } from '../types/electron';

/** A file from disk, filled out with defaults for anything it left unset. */
export function summaryToTheme(summary: ThemeSummary): Theme {
    return normalizeTheme({
        name: summary.name,
        author: summary.author ?? undefined,
        colors: summary.colors,
        icons: summary.icons ?? undefined,
        opacity: summary.opacity ?? undefined,
        options: summary.options
            ? {
                autoContrast:
                    typeof summary.options.auto_contrast === 'boolean'
                        ? summary.options.auto_contrast
                        : true,
                adaptSvg:
                    typeof summary.options.adapt_svg === 'boolean'
                        ? summary.options.adapt_svg
                        : true,
                interfaceBlur: summary.options.interface_blur ?? 0,
            }
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
    /** Optional explicit preview. The visual editor keeps drafts local and
     * commits them only on Save, so routine colour dragging does not repaint
     * the whole application. */
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
        // Hand the picture straight back when it has not changed. When the user
        // returns to an already visited theme, read the warm cache synchronously
        // so the old picture is never cleared for a frame before being restored.
        const cachedUrl = path && imageCache.has(path) ? imageCache.get(path) ?? null : null;
        const url = path && path === paintedImagePath ? paintedImageUrl : cachedUrl;
        applyTheme(theme, document.documentElement, url);

        if (path && path !== paintedImagePath && !imageCache.has(path)) {
            void loadBackgroundImage(theme).then((loaded) => {
                const latest = get();
                const current = latest.preview ?? resolveActive(latest.themes, latest.activeFileName);
                if (current === BROKEN) return;
                // Only paint if the theme has not moved on while we were reading.
                if (current?.backgroundImage?.path !== path) return;
                paintedImagePath = path;
                paintedImageUrl = loaded;
                // The colours were painted before the async read. Updating only
                // the picture avoids a second global palette invalidation.
                applyThemeBackgroundImage(current, document.documentElement, loaded);
            });
        } else if (path) {
            paintedImagePath = path;
            paintedImageUrl = url;
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
    let swapFrame: number | null = null;
    let swapTimer: ReturnType<typeof setTimeout> | null = null;
    let swapVariant = false;

    const crossfade = (run: () => void) => {
        if (typeof document === 'undefined') { run(); return; }

        const root = document.documentElement;

        /**
         * Land the whole palette in one frame.
         *
         * Components carry their own `transition-colors`, which otherwise fire
         * on the custom-property change and bring the theme in element by
         * element. Suppressing them for the swap is what makes it arrive all at
         * once — and costs nothing, since it removes animation work.
         */
        const release = () => {
            root.classList?.remove('r2-theme-swapping');
            root.classList?.remove('r2-theme-crossfade-a');
            root.classList?.remove('r2-theme-crossfade-b');
            swapTimer = null;
        };

        const reduced =
            typeof matchMedia === 'function' &&
            matchMedia('(prefers-reduced-motion: reduce)').matches;

        if (reduced) {
            release();
            run();
            return;
        }

        // Native View Transitions snapshot the entire window twice. That is
        // inexpensive on a small document but stalls this app when the home
        // grid or profile sidebar contains hundreds of cards. A tiny colour
        // veil gives the palette a soft landing using one compositor animation,
        // without duplicating the page or retaining another full-size texture.
        if (swapFrame !== null && typeof cancelAnimationFrame === 'function') {
            cancelAnimationFrame(swapFrame);
            swapFrame = null;
        }
        if (swapTimer !== null) clearTimeout(swapTimer);
        root.classList?.remove('r2-theme-crossfade-a');
        root.classList?.remove('r2-theme-crossfade-b');
        root.classList?.add('r2-theme-swapping');
        swapVariant = !swapVariant;
        root.classList?.add(swapVariant ? 'r2-theme-crossfade-a' : 'r2-theme-crossfade-b');
        run();

        if (typeof requestAnimationFrame !== 'function') {
            release();
            return;
        }
        swapFrame = requestAnimationFrame(() => {
            swapFrame = null;
            swapTimer = setTimeout(release, 160);
        });
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
            // Painted straight away rather than through the frame-coalescing
            // path: the transition class has to be in place for the same frame
            // that changes the colours, or there is nothing to ease from.
            crossfade(() => {
                // Selection state and CSS tokens belong to the same visual
                // transaction. Updating either outside it is what produced the
                // staggered sidebar/content swap visible in screen recordings.
                set({ activeFileName: id });
                paint();
            });
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
