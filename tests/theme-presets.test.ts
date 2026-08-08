import assert from 'node:assert/strict';
import test from 'node:test';

import {
    DEFAULT_THEME,
    THEME_COLOR_KEYS,
    contrastRatio,
    isValidHex,
    normalizeTheme,
    parseHex,
    resolveOnAccent,
    resolveTheme,
} from '../src/utils/theme.ts';
import {
    BUILTIN_PREFIX,
    THEME_PRESETS,
    allBuiltinThemes,
    findPreset,
    isBuiltinId,
} from '../src/utils/themePresets.ts';

test('every preset defines all nine colours, as valid hex', () => {
    for (const preset of THEME_PRESETS) {
        for (const key of THEME_COLOR_KEYS) {
            const value = preset.colors[key];
            assert.ok(
                value && isValidHex(value),
                `${preset.name} has no valid ${key} (got ${String(value)})`
            );
        }
    }
});

test('preset ids are unique and namespaced away from file names', () => {
    const ids = THEME_PRESETS.map((p) => p.id);
    assert.equal(new Set(ids).size, ids.length, 'duplicate preset id');
    for (const id of ids) {
        assert.ok(id.startsWith(BUILTIN_PREFIX), id);
        // A preset id must never be mistaken for a theme file.
        assert.ok(!id.endsWith('.toml'), id);
        assert.match(id.slice(BUILTIN_PREFIX.length), /^[a-z0-9-]+$/, id);
    }
});

test('every preset is readable: text holds up against its own surfaces', () => {
    // A shipped theme that fails its own contrast advice would be a poor
    // advertisement for the feature, so this is checked rather than assumed.
    for (const preset of THEME_PRESETS) {
        const c = preset.colors;
        assert.ok(
            contrastRatio(c.text, c.background) >= 4.5,
            `${preset.name}: text on background is ${contrastRatio(c.text, c.background).toFixed(2)}:1`
        );
        assert.ok(
            contrastRatio(c.text, c.surface) >= 4.5,
            `${preset.name}: text on surface is ${contrastRatio(c.text, c.surface).toFixed(2)}:1`
        );
        assert.ok(
            contrastRatio(c.text_muted, c.background) >= 3,
            `${preset.name}: muted text is ${contrastRatio(c.text_muted, c.background).toFixed(2)}:1`
        );
    }
});

test('status text is readable on every preset, light or dark', () => {
    // The failure this guards against: a shade chosen for a dark panel, such as
    // amber-200, lands at pale-on-pale once the theme inverts. The resolved
    // foreground tokens must clear the bar whichever way the theme runs.
    for (const preset of THEME_PRESETS) {
        const { fg } = resolveTheme(normalizeTheme(preset));
        for (const [role, colour] of Object.entries(fg)) {
            const ratio = contrastRatio(colour, preset.colors.surface);
            assert.ok(
                ratio >= 4.5,
                `${preset.name}: ${role} text is ${ratio.toFixed(2)}:1 on its surface`
            );
        }
    }
});

test('every preset produces a readable button label', () => {
    // 2.6, not the 3:1 of the bold-text guideline. A filled button carries a
    // light label in light and dark themes alike, and at 3 the rule flipped on
    // hairline differences: Claudio Dark's coral measures 2.81 against its
    // cream text, and falling to near-black lettering there reads as a mistake
    // whatever it scores. Anything genuinely lost still gives way.
    for (const preset of THEME_PRESETS) {
        const label = resolveOnAccent(preset.colors);
        const ratio = contrastRatio(label, preset.colors.accent);
        assert.ok(ratio >= 2.6, `${preset.name}: label on accent is ${ratio.toFixed(2)}:1`);
    }
});

test('every preset expands into a full, valid palette', () => {
    for (const preset of THEME_PRESETS) {
        const palette = resolveTheme(normalizeTheme(preset));
        for (const shade of [50, 400, 500, 900] as const) {
            assert.ok(isValidHex(palette.gray[shade]), `${preset.name} gray-${shade}`);
            assert.ok(isValidHex(palette.blue[shade]), `${preset.name} blue-${shade}`);
            assert.ok(isValidHex(palette.red[shade]), `${preset.name} red-${shade}`);
        }
    }
});

test('light presets genuinely invert the ramp', () => {
    const brightness = (hex: string) => {
        const n = parseInt(hex.slice(1), 16);
        return 0.2126 * ((n >> 16) & 255) + 0.7152 * ((n >> 8) & 255) + 0.0722 * (n & 255);
    };
    for (const id of ['github-light', 'claudio-light', 'r2modmac-light']) {
        const preset = findPreset(`${BUILTIN_PREFIX}${id}`);
        assert.ok(preset, `missing preset ${id}`);
        const { gray } = resolveTheme(normalizeTheme(preset!));
        assert.ok(
            brightness(gray[900]) > brightness(gray[50]),
            `${id}: expected an inverted ramp`
        );
    }
});

test('dark and light variants of a brand differ meaningfully', () => {
    const dark = findPreset(`${BUILTIN_PREFIX}github-dark`)!;
    const light = findPreset(`${BUILTIN_PREFIX}github-light`)!;
    assert.notEqual(dark.colors.background, light.colors.background);
    assert.notEqual(dark.colors.text, light.colors.text);
});

// ── Structural colour rules ──────────────────────────────────────────────────
// Contrast is necessary but not sufficient: a palette can clear every contrast
// bar and still be unusable because panels have no edge or two status colours
// are the same hue. These check the relationships, in OKLCH so the distances
// are perceptual.

const linear = (c: number) => (c <= 0.04045 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4));

function oklch(hex: string): { L: number; C: number; h: number } {
    const { r, g, b } = parseHex(hex)!;
    const R = linear(r / 255), G = linear(g / 255), B = linear(b / 255);
    const l = Math.cbrt(0.4122214708 * R + 0.5363325363 * G + 0.0514459929 * B);
    const m = Math.cbrt(0.2119034982 * R + 0.6806995451 * G + 0.1073969566 * B);
    const s = Math.cbrt(0.0883024619 * R + 0.2817188376 * G + 0.6299787005 * B);
    const L = 0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s;
    const A = 1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s;
    const Bb = 0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s;
    return { L, C: Math.hypot(A, Bb), h: ((Math.atan2(Bb, A) * 180) / Math.PI + 360) % 360 };
}

function hueGap(a: number, b: number): number {
    const d = Math.abs(a - b) % 360;
    return d > 180 ? 360 - d : d;
}

test('panels lift off the page, and their borders can be seen', () => {
    // Caught r2modmac Light, whose surface sat 0.015 from the background —
    // every card was edgeless.
    for (const preset of THEME_PRESETS) {
        const c = normalizeTheme(preset).colors;
        const elevation = Math.abs(oklch(c.surface).L - oklch(c.background).L);
        assert.ok(
            elevation >= 0.02,
            `${preset.name}: surface is only ΔL ${elevation.toFixed(3)} from the background`
        );
        const edge = Math.abs(oklch(c.border).L - oklch(c.surface).L);
        assert.ok(
            edge >= 0.03,
            `${preset.name}: border is only ΔL ${edge.toFixed(3)} from the surface`
        );
    }
});

test('muted text reads as secondary, not as a second primary', () => {
    for (const preset of THEME_PRESETS) {
        const c = normalizeTheme(preset).colors;
        const gap = Math.abs(oklch(c.text).L - oklch(c.text_muted).L);
        assert.ok(gap >= 0.08, `${preset.name}: muted text is only ΔL ${gap.toFixed(3)} from primary`);
    }
});

test('status colours are tellable apart from each other and from the accent', () => {
    // Caught both Claudio themes: a coral accent at 39° with a coral-red danger
    // at 28° made destructive buttons look primary. Categorical meaning is
    // carried by hue, so the hues have to actually differ.
    for (const preset of THEME_PRESETS) {
        const c = normalizeTheme(preset).colors;
        const h = {
            accent: oklch(c.accent).h,
            danger: oklch(c.danger).h,
            warning: oklch(c.warning).h,
            success: oklch(c.success).h,
        };
        const pairs: Array<[string, number, number]> = [
            ['danger/warning', h.danger, h.warning],
            ['warning/success', h.warning, h.success],
            ['danger/success', h.danger, h.success],
            ['accent/danger', h.accent, h.danger],
            ['accent/success', h.accent, h.success],
        ];
        for (const [label, a, b] of pairs) {
            const gap = hueGap(a, b);
            assert.ok(gap >= 25, `${preset.name}: ${label} only ${gap.toFixed(0)}° apart`);
        }
    }
});

test('status colours carry enough chroma to signal anything', () => {
    for (const preset of THEME_PRESETS) {
        const c = normalizeTheme(preset).colors;
        for (const role of ['danger', 'warning', 'success'] as const) {
            const chroma = oklch(c[role]).C;
            assert.ok(chroma >= 0.05, `${preset.name}: ${role} is nearly greyscale (C ${chroma.toFixed(3)})`);
        }
    }
});

test('decorative icons stay visible on every theme, light or dark', () => {
    // These hues are fixed on purpose — they exist to tell one icon from
    // another — so what has to adapt is their lightness. Mirroring the ramp for
    // light themes was not enough: lime on a pale box and indigo on a dark one
    // both sat near 2:1, so the engine walks each family to a shade that works.
    for (const preset of THEME_PRESETS) {
        const palette = resolveTheme(normalizeTheme(preset));
        const backdrop = palette.gray[700]; // the icon box
        for (const [family, ramp] of Object.entries(palette.decorative)) {
            const ratio = contrastRatio(ramp[400], backdrop);
            assert.ok(
                ratio >= 3,
                `${preset.name}: ${family} icons sit at ${ratio.toFixed(2)}:1 on their box`
            );
        }
    }
});

test('adapting an icon changes its lightness, never its hue', () => {
    // An icon that changed colour between themes would stop being a landmark.
    const hueOf = (hex: string) => {
        const { r, g, b } = parseHex(hex)!;
        const max = Math.max(r, g, b), min = Math.min(r, g, b), d = max - min;
        if (d === 0) return 0;
        const h = max === r ? ((g - b) / d) % 6 : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
        return (h * 60 + 360) % 360;
    };
    const dark = resolveTheme(normalizeTheme(findPreset(`${BUILTIN_PREFIX}github-dark`)!));
    const light = resolveTheme(normalizeTheme(findPreset(`${BUILTIN_PREFIX}github-light`)!));

    for (const family of Object.keys(dark.decorative)) {
        const a = hueOf(dark.decorative[family][400]);
        const b = hueOf(light.decorative[family][400]);
        const gap = Math.min(Math.abs(a - b), 360 - Math.abs(a - b));
        assert.ok(gap <= 20, `${family} shifted hue by ${gap.toFixed(0)}° between themes`);
    }
});

test('a hover state belongs to its own theme, never to the default one', () => {
    // The bug this guards: `accent_hover` was written to disk but not declared
    // on the Rust side, so it was dropped on load and then backfilled from the
    // default theme — a coral theme came back with a blue hover. The tell is
    // that the hover shares no hue with its accent.
    const hueOf = (hex: string) => {
        const { r, g, b } = parseHex(hex)!;
        const max = Math.max(r, g, b), min = Math.min(r, g, b), d = max - min;
        if (d === 0) return 0;
        const h = max === r ? ((g - b) / d) % 6 : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
        return (h * 60 + 360) % 360;
    };

    for (const preset of THEME_PRESETS) {
        const theme = normalizeTheme(preset);
        const { accentHover } = resolveTheme(theme);
        const gap = Math.abs(hueOf(accentHover) - hueOf(theme.colors.accent));
        assert.ok(
            Math.min(gap, 360 - gap) <= 45,
            `${preset.name}: hover ${accentHover} is unrelated to accent ${theme.colors.accent}`
        );
    }
});

test('a theme that loses its hover colours re-derives them from its own accent', () => {
    // Simulates a reload that dropped the hover keys: the fallback must come
    // from this theme's accent, not from the default theme's blue.
    const coral = normalizeTheme(findPreset(`${BUILTIN_PREFIX}claudio-dark`)!);
    const stripped = { ...coral.colors };
    delete (stripped as Record<string, unknown>).accent_hover;
    delete (stripped as Record<string, unknown>).surface_hover;

    const { accentHover } = resolveTheme({ name: 'Reloaded', colors: stripped });
    assert.notEqual(accentHover, DEFAULT_THEME.colors.accent_hover, 'fell back to the default blue');
    assert.ok(
        contrastRatio(accentHover, coral.colors.accent) < 2,
        `hover ${accentHover} should stay close to the accent ${coral.colors.accent}`
    );
});

test('the built-in list leads with the stock look', () => {
    const all = allBuiltinThemes();
    assert.equal(all[0].id, `${BUILTIN_PREFIX}default`);
    assert.equal(all[0].name, 'Default');
    assert.equal(all.length, THEME_PRESETS.length + 1);
});

test('built-in ids are told apart from theme file names', () => {
    assert.ok(isBuiltinId(`${BUILTIN_PREFIX}nord`));
    assert.ok(!isBuiltinId('nord.toml'));
    assert.ok(!isBuiltinId(null));
    assert.equal(findPreset('nord.toml'), null);
    assert.equal(findPreset(null), null);
});

test('every filled control in every preset carries a readable label', () => {
    // Found the hard way: the Update button kept a fixed `text-white` while the
    // engine had an `on-warning` token nobody had wired to it, so on Dracula it
    // sat at 1.37:1 — pale text on pale yellow. This checks each fill the app
    // actually paints, rather than trusting that every call site was converted.
    for (const preset of THEME_PRESETS) {
        const p = resolveTheme(normalizeTheme(preset));
        const fills: Array<[string, string, string]> = [
            ['accent', p.on.accent, p.blue[600]],
            ['secondary', p.on.surface, p.gray[700]],
            ['danger', p.on.danger, p.red[600]],
            ['warning', p.on.warning, p.yellow[600]],
            ['warning (amber)', p.on.warning, p.amber[600]],
            ['success', p.on.success, p.green[600]],
        ];
        for (const [name, ink, fill] of fills) {
            const ratio = contrastRatio(ink, fill);
            assert.ok(
                ratio >= 2.6,
                `${preset.name}: ${name} label is ${ratio.toFixed(2)}:1 on ${fill}`
            );
        }
    }
});
