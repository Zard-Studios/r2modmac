import assert from 'node:assert/strict';
import test from 'node:test';

import {
    DEFAULT_THEME,
    applyTheme,
    contrastRatio,
    findContrastWarnings,
    isValidHex,
    normalizeTheme,
    parseHex,
    resolveOnAccent,
    resolveTheme,
    THEMED_FAMILIES,
    DECORATIVE_FAMILIES,
    themeToToml,
    type StyleTarget,
    type Theme,
} from '../src/utils/theme.ts';

// Tailwind's stock ramps, duplicated here on purpose: if the engine ever stops
// reproducing them, this file should fail rather than quietly agree with a
// changed copy of the same constants inside the module under test.
const STOCK_GRAY: Record<number, string> = {
    50: '#f9fafb', 100: '#f3f4f6', 200: '#e5e7eb', 300: '#d1d5db',
    400: '#9ca3af', 500: '#6b7280', 600: '#4b5563', 700: '#374151',
    800: '#1f2937', 900: '#111827', 950: '#030712',
};
const STOCK_BLUE: Record<number, string> = {
    50: '#eff6ff', 100: '#dbeafe', 200: '#bfdbfe', 300: '#93c5fd',
    400: '#60a5fa', 500: '#3b82f6', 600: '#2563eb', 700: '#1d4ed8',
    800: '#1e40af', 900: '#1e3a8a', 950: '#172554',
};

const SHADES = [50, 100, 200, 300, 400, 500, 600, 700, 800, 900, 950];

/** Largest per-channel difference between two hex colours. */
function channelDistance(a: string, b: string): number {
    const x = parseHex(a)!;
    const y = parseHex(b)!;
    return Math.max(Math.abs(x.r - y.r), Math.abs(x.g - y.g), Math.abs(x.b - y.b));
}

function fakeRoot(): StyleTarget & { props: Map<string, string> } {
    const props = new Map<string, string>();
    return {
        props,
        style: {
            setProperty: (name, value) => { props.set(name, value); },
            removeProperty: (name) => { props.delete(name); },
        },
    };
}

const LIGHT: Theme = {
    name: 'Light',
    colors: {
        background: '#ffffff', surface: '#f3f4f6', border: '#d1d5db',
        text: '#111827', text_muted: '#6b7280', accent: '#2563eb',
        danger: '#dc2626', warning: '#d97706', success: '#16a34a',
    },
};

// ── Faithfulness to the stock palette ────────────────────────────────────────

test('the default accent reproduces Tailwind\'s blue ramp exactly', () => {
    // The accent expansion re-derives every shade from shade 500 alone. Feeding
    // it the stock blue must return the stock ramp untouched — proof the
    // transform re-parameterises the ramp rather than approximating it.
    const { blue } = resolveTheme(DEFAULT_THEME);
    for (const shade of SHADES) {
        assert.equal(blue[shade as 500], STOCK_BLUE[shade], `blue-${shade}`);
    }
});

test('the default theme reproduces the gray ramp within an imperceptible margin', () => {
    const { gray } = resolveTheme(DEFAULT_THEME);
    for (const shade of SHADES) {
        // Shade 50 is anchored to the theme's text colour (pure white) rather
        // than gray-50, so it legitimately differs; the rest are interpolated
        // and must stay visually identical to stock.
        const tolerance = shade === 50 ? 8 : 6;
        const delta = channelDistance(gray[shade as 900], STOCK_GRAY[shade]);
        assert.ok(
            delta <= tolerance,
            `gray-${shade}: ${gray[shade as 900]} vs ${STOCK_GRAY[shade]} (Δ${delta} > ${tolerance})`
        );
    }
});

// ── Anchors ──────────────────────────────────────────────────────────────────

test('the six theme colours land exactly on the shades the app paints with', () => {
    const theme: Theme = {
        name: 'Anchors',
        colors: {
            background: '#101020', surface: '#202030', border: '#303040',
            text: '#f0f0ff', text_muted: '#8080a0', accent: '#c81e5a',
            danger: '#e01b24', warning: '#e5a50a', success: '#2ec27e',
        },
    };
    const p = resolveTheme(theme);
    assert.equal(p.gray[900], '#101020', 'background drives gray-900');
    assert.equal(p.gray[800], '#202030', 'surface drives gray-800');
    assert.equal(p.gray[700], '#303040', 'border drives gray-700');
    assert.equal(p.gray[400], '#8080a0', 'muted text drives gray-400');
    assert.equal(p.blue[500], '#c81e5a', 'accent drives blue-500');
    assert.equal(p.white, '#f0f0ff', 'text drives the white token');
});

test('slate follows the same anchors as gray so slate-built screens stay in theme', () => {
    const p = resolveTheme(LIGHT);
    assert.equal(p.slate[900], LIGHT.colors.background);
    assert.equal(p.slate[800], LIGHT.colors.surface);
    assert.equal(p.slate[700], LIGHT.colors.border);
});

// ── A light theme must genuinely invert the ramp ─────────────────────────────

test('a light theme inverts the ramp, so no component needs restyling', () => {
    const { gray } = resolveTheme(LIGHT);
    const lightness = (hex: string) => {
        const { r, g, b } = parseHex(hex)!;
        return 0.2126 * r + 0.7152 * g + 0.0722 * b;
    };
    // gray-900 backs the app surface and gray-50/white carries text: under a
    // light theme those roles swap brightness, which is exactly what lets
    // `bg-gray-900 text-white` keep working without being touched.
    assert.ok(
        lightness(gray[900]) > lightness(gray[50]),
        `expected gray-900 (${gray[900]}) lighter than gray-50 (${gray[50]})`
    );
    assert.ok(lightness(gray[900]) > lightness(gray[700]));
});

test('every derived shade stays a valid, in-gamut hex colour', () => {
    const brutal: Theme = {
        name: 'Brutal',
        colors: {
            background: '#000000', surface: '#0a0a0a', border: '#1f1f1f',
            text: '#00ff00', text_muted: '#00aa00', accent: '#ff0000',
            danger: '#ff00ff', warning: '#ffff00', success: '#00ffff',
        },
    };
    for (const theme of [DEFAULT_THEME, LIGHT, brutal]) {
        const p = resolveTheme(theme);
        for (const family of THEMED_FAMILIES) {
            for (const shade of SHADES) {
                const hex = p[family][shade as 500];
                assert.ok(isValidHex(hex), `${theme.name} ${family}-${shade} = ${hex}`);
            }
        }
    }
});

// ── Status colours ───────────────────────────────────────────────────────────

test('the status colours drive their families', () => {
    const p = resolveTheme(LIGHT);
    assert.equal(p.red[500], LIGHT.colors.danger);
    assert.equal(p.amber[500], LIGHT.colors.warning);
    assert.equal(p.green[500], LIGHT.colors.success);
});

test('yellow and emerald follow their family without collapsing onto it', () => {
    // Both shadow another family, so they must move with the theme yet stay
    // visibly distinct — otherwise a yellow badge and an amber one become the
    // same colour and the distinction the UI relies on disappears.
    const p = resolveTheme(LIGHT);
    assert.notEqual(p.yellow[500], p.amber[500], 'yellow collapsed onto amber');
    assert.notEqual(p.emerald[500], p.green[500], 'emerald collapsed onto green');

    // The gap should be a family resemblance, not an unrelated hue.
    assert.ok(channelDistance(p.yellow[500], p.amber[500]) < 90);
    assert.ok(channelDistance(p.emerald[500], p.green[500]) < 90);
});

test('the stock status colours round-trip exactly', () => {
    // Same identity property as the accent: feeding the engine the stock
    // colours must return the stock ramps.
    const p = resolveTheme(DEFAULT_THEME);
    assert.equal(p.red[500], '#ef4444');
    assert.equal(p.amber[500], '#f59e0b');
    assert.equal(p.yellow[500], '#eab308');
    assert.equal(p.green[500], '#22c55e');
    assert.equal(p.emerald[500], '#10b981');
});

// ── Background pictures ──────────────────────────────────────────────────────

test('a background entry without a path is dropped rather than half-kept', () => {
    const theme = normalizeTheme({ name: 'X', colors: {}, backgroundImage: { opacity: 0.5 } });
    assert.equal(theme.backgroundImage, null);
});

test('background settings are clamped to sane values', () => {
    const theme = normalizeTheme({
        name: 'X',
        colors: {},
        backgroundImage: { path: 'assets/w.jpg', opacity: 5, blur: -3 },
    });
    assert.equal(theme.backgroundImage?.opacity, 1);
    assert.equal(theme.backgroundImage?.blur, 0);
});

test('a background survives a round trip through TOML text', () => {
    const toml = themeToToml({
        ...LIGHT,
        backgroundImage: { path: 'assets/wall.jpg', opacity: 0.4, blur: 6 },
    });
    assert.match(toml, /^\[background_image\]$/m);
    assert.match(toml, /^path\s+= "assets\/wall\.jpg"/m);
    assert.match(toml, /^opacity = 0\.4/m);
    assert.match(toml, /^blur\s+= 6/m);
});

test('a theme without a picture writes no background section', () => {
    assert.ok(!themeToToml(LIGHT).includes('background_image'));
});

// ── One text colour, many fills ──────────────────────────────────────────────

test('a label is picked per fill, so no single text colour has to serve them all', () => {
    // The case that motivated this: a theme carries one text colour, but a
    // confirm button and the cancel button beside it can be at opposite ends of
    // the lightness scale. One colour cannot read on both.
    const theme: Theme = {
        name: 'Contrasty',
        colors: {
            background: '#ffffff', surface: '#f3f4f6', border: '#d1d5db',
            text: '#111827', text_muted: '#6b7280',
            // A very pale accent: a dark label is the only readable choice.
            accent: '#bfe3ff',
            danger: '#7f1d1d', warning: '#d97706', success: '#16a34a',
        },
    };
    const { on } = resolveTheme(theme);

    for (const [fill, label] of Object.entries(on)) {
        assert.ok(
            [theme.colors.text, theme.colors.background].includes(label),
            `${fill} label ${label} is not one of the theme's own colours`
        );
    }

    // The pale accent must take the dark label; the deep red must take the light
    // one. If both came out the same, the feature would not be doing anything.
    assert.equal(on.accent, theme.colors.text, 'pale accent needs the dark label');
    assert.equal(on.danger, theme.colors.background, 'deep danger needs the light label');
    assert.notEqual(on.accent, on.danger);
});

test('every automatic label clears the bar on the fill it sits on', () => {
    const theme: Theme = {
        name: 'Mixed',
        colors: {
            background: '#0d1117', surface: '#151b23', border: '#3d444d',
            text: '#f0f6fc', text_muted: '#9198a1', accent: '#ffe066',
            danger: '#7f1d1d', warning: '#fff3bf', success: '#14532d',
        },
    };
    const p = resolveTheme(theme);
    const fills: Array<[string, string, string]> = [
        ['accent', p.on.accent, p.blue[500]],
        ['surface', p.on.surface, p.gray[700]],
        ['danger', p.on.danger, p.red[600]],
        ['warning', p.on.warning, p.amber[600]],
        ['success', p.on.success, p.green[600]],
    ];
    for (const [name, label, fill] of fills) {
        const ratio = contrastRatio(label, fill);
        assert.ok(ratio >= 3, `${name}: label sits at ${ratio.toFixed(2)}:1 on its fill`);
    }
});

test('turning automatic colours off hands every label back to the text colour', () => {
    // The escape hatch: someone placing every colour by hand gets exactly the
    // single text colour, everywhere, with nothing inferred on their behalf.
    const colors = LIGHT.colors;
    const manual = resolveTheme({ name: 'Manual', colors, options: { autoContrast: false } });
    for (const [fill, label] of Object.entries(manual.on)) {
        assert.equal(label, colors.text, `${fill} should fall back to the text colour`);
    }

    // And the switch has to actually change something, or it is decoration.
    const auto = resolveTheme({ name: 'Auto', colors, options: { autoContrast: true } });
    assert.notDeepEqual(auto.on, manual.on);
});

test('a page and panels at opposite ends are named as the root cause', () => {
    // The case that produced an unreadable editor: a black background beside a
    // near-white surface. Every text pairing then fails, and no amount of
    // automatic label picking can fix it — so the editor has to say plainly
    // that the two surfaces are the problem, not list the symptoms.
    const contradictory = {
        background: '#000000', surface: '#f3f4f6', border: '#d1d5db',
        text: '#111827', text_muted: '#6b7280', accent: '#2563eb',
        danger: '#dc2626', warning: '#d97706', success: '#16a34a',
    };
    const warnings = findContrastWarnings(contradictory);
    assert.equal(
        warnings[0].pair,
        'Background and surface are too far apart',
        'the root cause must be reported first'
    );

    // A sane theme keeps its page and panels close, so it stays silent.
    for (const colors of [DEFAULT_THEME.colors, LIGHT.colors]) {
        assert.ok(
            !findContrastWarnings(colors).some((w) => w.pair.includes('too far apart')),
            'a normal theme must not be flagged'
        );
    }
});

// ── Manual overrides ─────────────────────────────────────────────────────────

test('manual overrides are ignored while automatic colours are on', () => {
    // The switch has to be the single thing in charge. An override sitting in
    // the file must not quietly leak into a theme that asked for automatic.
    const colors = { ...LIGHT.colors, on_danger: '#00ff00', icon: '#ff00ff' };
    const auto = resolveTheme({ name: 'Auto', colors, options: { autoContrast: true } });
    assert.notEqual(auto.on.danger, '#00ff00');
});

test('switching automatic off hands the labels over to the overrides', () => {
    const colors = {
        ...LIGHT.colors,
        on_accent: '#112233', on_surface: '#223344', on_danger: '#334455',
        on_warning: '#445566', on_success: '#556677',
    };
    const manual = resolveTheme({ name: 'Manual', colors, options: { autoContrast: false } });
    assert.equal(manual.on.accent, '#112233');
    assert.equal(manual.on.surface, '#223344');
    assert.equal(manual.on.danger, '#334455');
    assert.equal(manual.on.warning, '#445566');
    assert.equal(manual.on.success, '#556677');
});

test('an override left unset falls back to the plain text colour', () => {
    // Turning the switch off must not change anything by itself; it only opens
    // the controls. Until one is set, the old single-colour behaviour stands.
    const manual = resolveTheme({ name: 'Manual', colors: LIGHT.colors, options: { autoContrast: false } });
    for (const label of Object.values(manual.on)) {
        assert.equal(label, LIGHT.colors.text);
    }
});

test('a manual icon colour replaces the adaptive hues', () => {
    const colors = { ...LIGHT.colors, icon: '#8b5cf6' };
    const manual = resolveTheme({ name: 'Manual', colors, options: { autoContrast: false } });
    // Every family lands on the chosen colour, so icons stop being multi-hued.
    const shades = Object.values(manual.decorative).map((ramp) => ramp[500]);
    assert.ok(shades.every((s) => s === shades[0]), 'families should agree');

    // With automatic on, they stay distinct.
    const auto = resolveTheme({ name: 'Auto', colors, options: { autoContrast: true } });
    const autoShades = new Set(Object.values(auto.decorative).map((r) => r[400]));
    assert.ok(autoShades.size > 1, 'automatic icons must keep their own hues');
});

test('overrides survive the file format and only appear when set', () => {
    assert.ok(!themeToToml(LIGHT).includes('on_danger'));
    const withOverride = themeToToml({
        ...LIGHT,
        colors: { ...LIGHT.colors, on_danger: '#334455' },
        options: { autoContrast: false },
    });
    assert.match(withOverride, /^on_danger\s+= "#334455"/m);
    // And a bad value is dropped rather than poisoning the palette.
    const cleaned = normalizeTheme({ name: 'X', colors: { ...LIGHT.colors, icon: 'not-a-colour' } });
    assert.equal(cleaned.colors.icon, undefined);
});

test('themes written before the option existed behave as if it were on', () => {
    const legacy = resolveTheme({ name: 'Legacy', colors: LIGHT.colors });
    const explicit = resolveTheme({
        name: 'Explicit', colors: LIGHT.colors, options: { autoContrast: true },
    });
    assert.deepEqual(legacy.on, explicit.on);
});

test('the option survives a round trip through the file format', () => {
    assert.match(themeToToml({ ...LIGHT, options: { autoContrast: false } }), /^auto_contrast = false$/m);
    assert.match(themeToToml({ ...LIGHT, options: { autoContrast: true } }), /^auto_contrast = true$/m);
    // A file that never mentions it still loads with it on.
    assert.equal(normalizeTheme({ name: 'X', colors: {} }).options?.autoContrast, true);
    assert.equal(
        normalizeTheme({ name: 'X', colors: {}, options: { autoContrast: false } }).options?.autoContrast,
        false
    );
});

// ── Applying and clearing ────────────────────────────────────────────────────

test('applying a theme writes every token the palette depends on', () => {
    const root = fakeRoot();
    applyTheme(DEFAULT_THEME, root);
    // Every palette family at 11 shades — the ones a theme drives plus the
    // fixed-hue icon families — then the text token, the five per-fill label
    // tokens, the two hover tokens, the four readable-on-surface status ones
    // and the two cover-chrome tokens.
    const families = THEMED_FAMILIES.length + DECORATIVE_FAMILIES.length;
    assert.equal(root.props.size, families * 11 + 1 + 5 + 2 + 4 + 2);
    assert.equal(root.props.get('--r2-gray-900'), '17 24 39');
    assert.equal(root.props.get('--r2-blue-500'), '59 130 246');
    assert.equal(root.props.get('--r2-white'), '255 255 255');
});

test('clearing the theme removes every token so the stylesheet defaults take over', () => {
    // Falling back to :root rather than writing default values inline is what
    // makes "no theme" byte-for-byte the pre-theming appearance.
    const root = fakeRoot();
    applyTheme(LIGHT, root);
    assert.ok(root.props.size > 0);
    applyTheme(null, root);
    assert.equal(root.props.size, 0);
});

test('channel values are integers, as rgb(var(...)) requires', () => {
    const root = fakeRoot();
    applyTheme(LIGHT, root);
    for (const [name, value] of root.props) {
        assert.match(value, /^\d{1,3} \d{1,3} \d{1,3}$/, `${name} = "${value}"`);
    }
});

// ── Hand-edited files ────────────────────────────────────────────────────────

test('a theme file missing colours keeps the ones it does define', () => {
    const theme = normalizeTheme({
        name: 'Partial',
        colors: { accent: '#ff8800', background: '#123456' },
    });
    assert.equal(theme.colors.accent, '#ff8800');
    assert.equal(theme.colors.background, '#123456');
    // The rest fall back rather than the whole file being rejected.
    assert.equal(theme.colors.surface, DEFAULT_THEME.colors.surface);
    assert.equal(theme.colors.text, DEFAULT_THEME.colors.text);
});

test('an unparseable colour falls back instead of poisoning the palette', () => {
    const theme = normalizeTheme({
        name: 'Typo',
        colors: { background: '#nothex', surface: '#1f2937' },
    });
    assert.equal(theme.colors.background, DEFAULT_THEME.colors.background);
    assert.equal(theme.colors.surface, '#1f2937');
});

test('shorthand and uppercase hex are accepted and canonicalised', () => {
    const theme = normalizeTheme({ name: 'Short', colors: { accent: '#F0A' } });
    assert.equal(theme.colors.accent, '#ff00aa');
});

test('an unnamed theme still gets a usable name', () => {
    assert.equal(normalizeTheme({ colors: {} }).name, 'Untitled');
    assert.equal(normalizeTheme({ name: '   ', colors: {} }).name, 'Untitled');
});

// ── Serialisation ────────────────────────────────────────────────────────────

// ── Readability advice ───────────────────────────────────────────────────────

test('the stock palette raises no readability warnings', () => {
    assert.deepEqual(findContrastWarnings(DEFAULT_THEME.colors), []);
    assert.deepEqual(findContrastWarnings(LIGHT.colors), []);
});

// ── Labels on an accent fill ─────────────────────────────────────────────────

const NORD_COLORS = {
    background: '#2e3440', surface: '#3b4252', border: '#4c566a',
    text: '#eceff4', text_muted: '#a3adbf', accent: '#88c0d0',
    danger: '#bf616a', warning: '#ebcb8b', success: '#a3be8c',
};

test('the stock accent keeps white button labels', () => {
    // White on blue-500 is 3.7:1 — below AA for body text but fine for the bold
    // labels buttons use. Flipping it would change the app's default look, so
    // this asserts the engine leaves it alone.
    assert.equal(resolveOnAccent(DEFAULT_THEME.colors), '#ffffff');
    assert.equal(resolveTheme(DEFAULT_THEME).onAccent, '#ffffff');
});

test('a pale accent flips button labels to the dark end of the theme', () => {
    // Nord's accent is light enough that its near-white text would vanish; the
    // label swaps to the theme's own background colour rather than to raw black.
    const onAccent = resolveOnAccent(NORD_COLORS);
    assert.equal(onAccent, NORD_COLORS.background);
    assert.ok(
        contrastRatio(onAccent, NORD_COLORS.accent) >= 3,
        `expected a readable label, got ${contrastRatio(onAccent, NORD_COLORS.accent)}`
    );
});

test('a dark accent keeps the light text colour', () => {
    const colors = { ...NORD_COLORS, accent: '#1e3a8a' };
    assert.equal(resolveOnAccent(colors), colors.text);
});

test('light themes produce white button labels on saturated accents', () => {
    const lightThemeColors = {
        background: '#ffffff', surface: '#f3f4f6', border: '#d1d5db',
        text: '#111827', text_muted: '#6b7280', accent: '#2563eb',
        danger: '#dc2626', warning: '#d97706', success: '#16a34a',
    };
    assert.equal(resolveOnAccent(lightThemeColors), '#ffffff');
    assert.ok(
        contrastRatio(resolveOnAccent(lightThemeColors), lightThemeColors.accent) >= 4,
        `expected high contrast on light theme accent button`
    );
});

test('button labels are readable across wildly different accents', () => {
    for (const accent of ['#88c0d0', '#ffff00', '#000000', '#ffffff', '#ff0000', '#3b82f6']) {
        const colors = { ...NORD_COLORS, accent };
        const ratio = contrastRatio(resolveOnAccent(colors), accent);
        assert.ok(ratio >= 3, `accent ${accent} gave only ${ratio.toFixed(2)}:1`);
    }
});

test('readability advice stays silent about accent labels, which are handled', () => {
    // The engine guarantees them, so warning would report a solved problem.
    const pairs = findContrastWarnings(NORD_COLORS).map((w) => w.pair);
    assert.ok(!pairs.some((p) => p.includes('on accent') && p.includes('text')), pairs.join(', '));
});

test('text that vanishes into the background is flagged', () => {
    const warnings = findContrastWarnings({
        background: '#111827', surface: '#1f2937', border: '#374151',
        text: '#1a2233', text_muted: '#151d2b', accent: '#3b82f6',
        danger: '#ef4444', warning: '#f59e0b', success: '#22c55e',
    });
    const pairs = warnings.map((w) => w.pair);
    assert.ok(pairs.includes('Text on background'), JSON.stringify(pairs));
    assert.ok(pairs.includes('Muted text on background'), JSON.stringify(pairs));
});

test('contrast is symmetric and bounded by black on white', () => {
    assert.equal(Math.round(contrastRatio('#000000', '#ffffff')), 21);
    assert.equal(contrastRatio('#ffffff', '#000000'), contrastRatio('#000000', '#ffffff'));
    assert.equal(contrastRatio('#3b82f6', '#3b82f6'), 1);
});

// ── Serialisation ────────────────────────────────────────────────────────────

test('serialised themes carry every colour and stay readable', () => {
    const toml = themeToToml({ ...LIGHT, author: 'Fede' });
    assert.match(toml, /^name = "Light"$/m);
    assert.match(toml, /^author = "Fede"$/m);
    assert.match(toml, /^\[colors\]$/m);
    for (const key of ['background', 'surface', 'border', 'text', 'text_muted', 'accent']) {
        assert.match(toml, new RegExp(`^${key}\\s+= "#[0-9a-f]{6}"`, 'm'), key);
    }
});

test('quotes in a theme name cannot break out of the TOML string', () => {
    const toml = themeToToml({ ...DEFAULT_THEME, name: 'He said "hi"\\done' });
    assert.match(toml, /^name = "He said \\"hi\\"\\\\done"$/m);
});

test('an author is omitted rather than written empty', () => {
    assert.ok(!themeToToml(DEFAULT_THEME).includes('author'));
});

// ── Cover chrome ─────────────────────────────────────────────────────────────

test('cover chrome defaults to a dark scrim but is settable', () => {
    // The default exists because artwork is arbitrary; it is not a reason to
    // withhold the control, so both colours have to be overridable.
    const stock = resolveTheme(LIGHT);
    assert.equal(stock.media.scrim, '#09090b');
    assert.equal(stock.media.ink, '#ffffff');

    const custom = resolveTheme({
        name: 'Custom covers',
        colors: { ...LIGHT.colors, media_scrim: '#1b3a5c', media_ink: '#ffe066' },
    });
    assert.equal(custom.media.scrim, '#1b3a5c');
    assert.equal(custom.media.ink, '#ffe066');
});

test('cover chrome is written to the file and survives a reload', () => {
    assert.ok(!themeToToml(LIGHT).includes('media_scrim'));
    const toml = themeToToml({
        ...LIGHT,
        colors: { ...LIGHT.colors, media_scrim: '#1b3a5c', media_ink: '#ffe066' },
    });
    assert.match(toml, /^media_scrim\s+= "#1b3a5c"/m);
    assert.match(toml, /^media_ink\s+= "#ffe066"/m);

    const reloaded = normalizeTheme({
        name: 'X',
        colors: { ...LIGHT.colors, media_scrim: '#1b3a5c', media_ink: '#ffe066' },
    });
    assert.equal(reloaded.colors.media_scrim, '#1b3a5c');
    assert.equal(reloaded.colors.media_ink, '#ffe066');
});

test('the picture layout survives the file format', () => {
    // These were written to disk but not read back, so every reload silently
    // reset them — the reason the settings appeared not to save.
    const toml = themeToToml({
        ...LIGHT,
        backgroundImage: {
            path: 'assets/w.png', opacity: 0.5, blur: 0,
            fit: 'tile', offset_x: 25, offset_y: 75, tile_scale: 40,
        },
    });
    assert.match(toml, /^fit\s+= "tile"/m);
    assert.match(toml, /^offset_x = 25/m);
    assert.match(toml, /^offset_y = 75/m);
    assert.match(toml, /^tile_scale = 40/m);

    const reloaded = normalizeTheme({
        name: 'X',
        colors: LIGHT.colors,
        backgroundImage: {
            path: 'assets/w.png', fit: 'tile', offset_x: 25, offset_y: 75, tile_scale: 40,
        },
    });
    assert.equal(reloaded.backgroundImage?.fit, 'tile');
    assert.equal(reloaded.backgroundImage?.offset_x, 25);
    assert.equal(reloaded.backgroundImage?.offset_y, 75);
    assert.equal(reloaded.backgroundImage?.tile_scale, 40);
});

test('a pattern is sized as a share of the window, not by its pixel size', () => {
    // `background-size: auto` would pin a tile to the file's own dimensions and
    // ignore the scale entirely, which is what made the setting look inert.
    const root = fakeRoot();
    applyTheme(
        {
            ...LIGHT,
            backgroundImage: { path: 'a.png', opacity: 1, blur: 0, fit: 'tile', tile_scale: 40 },
        },
        root,
        'data:image/png;base64,AAAA'
    );
    assert.equal(root.props.get('--r2-background-size'), '40% auto');
    assert.equal(root.props.get('--r2-background-repeat'), 'repeat');
});

test('stretch distorts to the window while cover crops to it', () => {
    const root = fakeRoot();
    const withFit = (fit: 'fill' | 'cover' | 'contain') => {
        applyTheme(
            { ...LIGHT, backgroundImage: { path: 'a.png', opacity: 1, blur: 0, fit } },
            root,
            'data:image/png;base64,AAAA'
        );
        return root.props.get('--r2-background-size');
    };
    assert.equal(withFit('fill'), '100% 100%');
    assert.equal(withFit('cover'), 'cover');
    assert.equal(withFit('contain'), 'contain');
});
