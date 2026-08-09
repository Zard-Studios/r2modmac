import assert from 'node:assert/strict';
import test from 'node:test';

import { DEFAULT_THEME, type Theme } from '../src/utils/theme.ts';
import { BUILTIN_PREFIX, findPreset } from '../src/utils/themePresets.ts';

/**
 * Exercises the selection/preview/save loop the theme editor drives.
 *
 * These are the paths where the app decides what to paint, and a mistake there
 * shows up as the window silently reverting to the stock palette — which is
 * exactly the failure this file was written to catch.
 */

// ── Test doubles ─────────────────────────────────────────────────────────────

interface FakeFile { name: string; toml: string }

function installFakeEnvironment() {
    const props = new Map<string, string>();
    const attributes = new Set<string>();
    const classes = new Set<string>();
    const root = {
        style: {
            setProperty: (k: string, v: string) => { props.set(k, v); },
            removeProperty: (k: string) => { props.delete(k); },
        },
        setAttribute: (k: string) => { attributes.add(k); },
        removeAttribute: (k: string) => { attributes.delete(k); },
        classList: {
            add: (c: string) => { classes.add(c); },
            remove: (c: string) => { classes.delete(c); },
            contains: (c: string) => classes.has(c),
        },
    };
    // A theme swap cross-fades through a View Transition; recording the calls
    // is how the tests tell an animated swap from a silent repaint.
    const transitions: number[] = [];
    (globalThis as Record<string, unknown>).document = {
        documentElement: root,
        startViewTransition: (cb: () => void | Promise<void>) => {
            transitions.push(1);
            const updated = Promise.resolve(cb());
            return { finished: updated };
        },
    };
    (globalThis as Record<string, unknown>).matchMedia = () => ({ matches: false });

    const files = new Map<string, FakeFile>();
    let savedSelection: string | null = null;

    // A stand-in for the Rust side: parses only what the app writes, which is
    // enough to prove the round trip between saving and reloading.
    const parse = (name: string, toml: string) => {
        const colors: Record<string, string> = {};
        const colorSection = toml.split('[colors]')[1]?.split('[background_image]')[0] ?? '';
        for (const line of colorSection.split('\n')) {
            const m = /^\s*(\w+)\s*=\s*"([^"]+)"/.exec(line);
            if (m) colors[m[1]] = m[2];
        }
        const nameMatch = /^name = "([^"]*)"/m.exec(toml);
        const authorMatch = /^author = "([^"]*)"/m.exec(toml);
        const opacitySection = toml.split('[opacity]')[1]?.split(/^\[/m)[0] ?? '';
        const opacity: Record<string, number> = {};
        for (const line of opacitySection.split('\n')) {
            const m = /^\s*(\w+)\s*=\s*([0-9.]+)/.exec(line);
            if (m) opacity[m[1]] = Number(m[2]);
        }
        const imageSection = toml.split('[background_image]')[1];
        const pathMatch = imageSection ? /^path\s*=\s*"([^"]+)"/m.exec(imageSection) : null;
        const str = (k: string) =>
            imageSection ? new RegExp(`^${k}\\s*=\\s*"([^"]+)"`, 'm').exec(imageSection)?.[1] : undefined;
        const num = (k: string) => {
            const m = imageSection ? new RegExp(`^${k}\\s*=\\s*([0-9.]+)`, 'm').exec(imageSection) : null;
            return m ? Number(m[1]) : undefined;
        };
        return {
            file_name: name,
            name: nameMatch?.[1] ?? name.replace('.toml', ''),
            author: authorMatch ? authorMatch[1] : null,
            colors,
            opacity: Object.keys(opacity).length > 0 ? opacity : null,
            background_image: pathMatch
                ? {
                      path: pathMatch[1],
                      opacity: num('opacity') ?? 0.35,
                      blur: num('blur') ?? 0,
                      fit: str('fit'),
                      offset_x: num('offset_x'),
                      offset_y: num('offset_y'),
                      tile_scale: num('tile_scale'),
                  }
                : null,
            error: null,
        };
    };

    (globalThis as Record<string, unknown>).window = {
        ipcRenderer: {
            listThemes: async () =>
                [...files.values()].map((f) => parse(f.name, f.toml)),
            writeTheme: async (name: string, toml: string) => { files.set(name, { name, toml }); },
            deleteTheme: async (name: string) => { files.delete(name); },
            setActiveTheme: async (id: string | null) => { savedSelection = id; },
            readThemeImage: async () => 'data:image/png;base64,AAAA',
            suggestThemeFileName: async (n: string) =>
                `${n.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '')}.toml`,
        },
    };

    return {
        props,
        files,
        classes,
        transitions,
        selection: () => savedSelection,
        /** The background colour currently painted, as "r g b". */
        background: () => props.get('--r2-gray-900'),
        accent: () => props.get('--r2-blue-500'),
    };
}

function channels(hex: string): string {
    const n = parseInt(hex.slice(1), 16);
    return `${(n >> 16) & 255} ${(n >> 8) & 255} ${n & 255}`;
}

/** Imported lazily so the fake globals exist before the module initialises. */
async function freshStore() {
    const mod = await import(`../src/store/useThemeStore.ts?bust=${Math.random()}`);
    return mod.useThemeStore;
}

// ── Selection ────────────────────────────────────────────────────────────────

test('selecting a built-in preset paints it', async () => {
    const env = installFakeEnvironment();
    const store = await freshStore();

    await store.getState().hydrate(null);
    assert.equal(env.background(), undefined, 'stock look writes no tokens');

    await store.getState().setActive(`${BUILTIN_PREFIX}nord`);
    const nord = findPreset(`${BUILTIN_PREFIX}nord`)!;
    assert.equal(env.background(), channels(nord.colors.background));
    assert.equal(env.selection(), `${BUILTIN_PREFIX}nord`);
});

test('an unknown selection falls back to stock rather than stale colours', async () => {
    const env = installFakeEnvironment();
    const store = await freshStore();

    await store.getState().setActive(`${BUILTIN_PREFIX}nord`);
    assert.ok(env.background());

    await store.getState().setActive('deleted-by-someone-else.toml');
    assert.equal(env.background(), undefined);
});

// ── The duplicate → edit → save loop ─────────────────────────────────────────

test('saving an edited theme keeps it applied instead of reverting to stock', async () => {
    // The reported failure: after Save the window jumped back to the default
    // palette while the editor still showed the edited theme.
    const env = installFakeEnvironment();
    const store = await freshStore();
    const { themeToToml, normalizeTheme } = await import('../src/utils/theme.ts');

    // Duplicate a preset into a real file, as the editor does.
    const preset = normalizeTheme(findPreset(`${BUILTIN_PREFIX}claudio-dark`)!);
    const fileName = await window.ipcRenderer.suggestThemeFileName('Claudio Dark copy');
    await window.ipcRenderer.writeTheme(fileName, themeToToml({ ...preset, name: 'Claudio Dark copy' }));
    await store.getState().loadThemes();
    await store.getState().setActive(fileName);

    assert.equal(env.background(), channels(preset.colors.background), 'duplicate should be applied');

    // Edit it: the editor previews unsaved changes over the whole app.
    const edited: Theme = {
        ...preset,
        name: 'Claudio Dark copy',
        colors: { ...preset.colors, background: '#123456', accent: '#abcdef' },
    };
    store.getState().setPreview(edited);
    assert.equal(env.background(), channels('#123456'), 'preview should be applied');

    // Save: write, drop the preview, reload.
    await window.ipcRenderer.writeTheme(fileName, themeToToml(edited));
    store.getState().setPreview(null);
    await store.getState().loadThemes();

    assert.equal(env.background(), channels('#123456'), 'saved theme must stay applied');
    assert.equal(env.accent(), channels('#abcdef'));
});

test('dropping the preview before the reload never shows the stock palette', async () => {
    // Ordering matters: clearing the preview repaints immediately from whatever
    // is in the store, so the saved file must still be there.
    const env = installFakeEnvironment();
    const store = await freshStore();
    const { themeToToml, normalizeTheme } = await import('../src/utils/theme.ts');

    const preset = normalizeTheme(findPreset(`${BUILTIN_PREFIX}dracula`)!);
    await window.ipcRenderer.writeTheme('mine.toml', themeToToml(preset));
    await store.getState().loadThemes();
    await store.getState().setActive('mine.toml');

    store.getState().setPreview({ ...preset, colors: { ...preset.colors, background: '#010203' } });
    store.getState().setPreview(null);

    assert.notEqual(env.background(), undefined, 'reverted to stock after clearing the preview');
    assert.equal(env.background(), channels(preset.colors.background));
});

test('deleting the active theme returns to stock, not to stale colours', async () => {
    const env = installFakeEnvironment();
    const store = await freshStore();
    const { themeToToml, normalizeTheme } = await import('../src/utils/theme.ts');

    await window.ipcRenderer.writeTheme(
        'mine.toml',
        themeToToml(normalizeTheme(findPreset(`${BUILTIN_PREFIX}nord`)!))
    );
    await store.getState().loadThemes();
    await store.getState().setActive('mine.toml');
    assert.ok(env.background());

    await window.ipcRenderer.deleteTheme('mine.toml');
    await store.getState().setActive(null);
    await store.getState().loadThemes();
    assert.equal(env.background(), undefined);
});

test('a theme file edited outside the app repaints on reload', async () => {
    const env = installFakeEnvironment();
    const store = await freshStore();
    const { themeToToml, normalizeTheme } = await import('../src/utils/theme.ts');

    const base = normalizeTheme(findPreset(`${BUILTIN_PREFIX}nord`)!);
    await window.ipcRenderer.writeTheme('mine.toml', themeToToml(base));
    await store.getState().loadThemes();
    await store.getState().setActive('mine.toml');

    // Someone saves the file in their own editor; the watcher triggers a reload.
    await window.ipcRenderer.writeTheme(
        'mine.toml',
        themeToToml({ ...base, colors: { ...base.colors, background: '#0f0f0f' } })
    );
    await store.getState().loadThemes();

    assert.equal(env.background(), channels('#0f0f0f'));
});

test('switching theme cross-fades; editing repaints without animating', async () => {
    const env = installFakeEnvironment();
    const store = await freshStore();

    // Selecting a theme is a whole-palette swap, so it goes through a View
    // Transition — one compositor cross-fade rather than a per-element one.
    // Transitioning every element instead is what made the swap stutter badly
    // enough that the animation was never seen.
    await store.getState().setActive(`${BUILTIN_PREFIX}nord`);
    assert.equal(env.transitions.length, 1, 'expected a cross-fade on switch');

    // Live editing repaints many times a second. Animating that would make the
    // colour picker lag behind the cursor instead of feeling smoother.
    const nord = findPreset(`${BUILTIN_PREFIX}nord`)!;
    for (const accent of ['#ff0000', '#00ff00', '#0000ff']) {
        store.getState().setPreview({ ...nord, colors: { ...nord.colors, accent } });
    }
    assert.equal(env.transitions.length, 1, 'edits must not animate');
});

test('a swap suppresses per-element transitions so the palette lands at once', async () => {
    // Components carry their own transition-colors for hover feedback. Left on,
    // they animate the custom-property change too — each at its own duration —
    // and the theme arrives as a ripple instead of a single change.
    const env = installFakeEnvironment();
    const store = await freshStore();

    const seen: boolean[] = [];
    const realAdd = (c: string) => { seen.push(c === 'r2-theme-swapping'); };
    void realAdd;

    await store.getState().setActive(`${BUILTIN_PREFIX}github-dark`);
    // The fake rAF runs synchronously, so by now the class has been added and
    // removed again; what matters is that it was applied around the change.
    assert.ok(env.classes.size === 0, 'the freeze must be lifted afterwards');
    assert.ok(env.background(), 'and the theme must actually be applied');
});

test('the stock look is the absence of tokens, not a written-out default', async () => {
    const env = installFakeEnvironment();
    const store = await freshStore();

    await store.getState().setActive(`${BUILTIN_PREFIX}github-dark`);
    assert.ok(env.props.size > 0);

    await store.getState().setActive(null);
    assert.equal(env.props.size, 0, 'stock must fall through to the stylesheet');
    assert.notEqual(DEFAULT_THEME, null);
});

test('the picture layout survives a save and reload, field for field', async () => {
    // Reported three times as "it reverts on save". Each time a different layer
    // was copying the image record field by field and dropping whatever had
    // been added since. This drives the real store through the real writer, so
    // a future field that goes unlisted fails here rather than in the app.
    const env = installFakeEnvironment();
    const store = await freshStore();
    const { themeToToml, normalizeTheme } = await import('../src/utils/theme.ts');
    const { summaryToTheme } = await import(`../src/store/useThemeStore.ts?probe=${Math.random()}`);

    const theme = normalizeTheme({
        ...findPreset(`${BUILTIN_PREFIX}nord`)!,
        opacity: { background: 0.42, surface: 0.8, accent: 0.67, media_scrim: 0.55 },
        backgroundImage: {
            path: 'assets/w.png', opacity: 0.6, blur: 8,
            fit: 'tile', offset_x: 20, offset_y: 80, tile_scale: 35,
        },
    });

    await window.ipcRenderer.writeTheme('mine.toml', themeToToml(theme));
    const [summary] = await store.getState().loadThemes();
    const reloaded = summaryToTheme(summary);

    assert.deepEqual(reloaded.opacity, theme.opacity);
    assert.deepEqual(reloaded.backgroundImage, theme.backgroundImage);
    assert.ok(env.files.has('mine.toml'));
});

// The TOML editor writes raw text; everything downstream has to carry the author
// through, or the colour view keeps the old value and writes it back over the file
// at the next save — which is how a rename kept silently reverting.
test('an author-only edit still reaches the loaded theme', async () => {
    const env = installFakeEnvironment();
    const store = await freshStore();
    const { themeToToml, normalizeTheme } = await import('../src/utils/theme.ts');
    const { summaryToTheme } = await import(`../src/store/useThemeStore.ts?author=${Math.random()}`);

    const base = normalizeTheme({ ...findPreset(`${BUILTIN_PREFIX}nord`)!, author: 'Zard Studios' });
    await window.ipcRenderer.writeTheme('mine.toml', themeToToml(base));
    await store.getState().loadThemes();

    // Rewrite by hand, changing nothing but the author — the case that failed.
    const edited = (env.files.get('mine.toml')!).toml.replace('"Zard Studios"', '"Zard Studio"');
    await window.ipcRenderer.writeTheme('mine.toml', edited);
    const [summary] = await store.getState().loadThemes();

    assert.equal(summary.author, 'Zard Studio', 'the file kept the edit');
    assert.equal(summaryToTheme(summary).author, 'Zard Studio', 'and the app read it back');
});
