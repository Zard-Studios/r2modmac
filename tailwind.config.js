/** @type {import('tailwindcss').Config} */

// Themeable palette families. Each shade resolves through a CSS custom property
// holding space-separated RGB channels, so `<alpha-value>` keeps working and
// every existing opacity modifier (bg-gray-800/95, text-gray-400/70, …) is
// unaffected. The defaults in index.css are Tailwind's own values, so a build
// with no theme applied renders exactly as it did before theming existed.
const SHADES = [50, 100, 200, 300, 400, 500, 600, 700, 800, 900, 950];

const themeable = (family) =>
    Object.fromEntries(
        SHADES.map((shade) => [shade, `rgb(var(--r2-${family}-${shade}) / <alpha-value>)`])
    );

export default {
    content: [
        "./index.html",
        "./src/**/*.{js,ts,jsx,tsx}",
    ],
    theme: {
        extend: {
            colors: {
                // Surfaces, borders and secondary text.
                gray: themeable('gray'),
                // A handful of screens (UninstallModal, GameSelector) were built
                // on slate rather than gray. Rather than restyle them, slate is
                // themeable too: it defaults to real slate values, and a custom
                // theme drives it from the same anchors as gray.
                slate: themeable('slate'),
                // Accent: buttons, links, selection.
                blue: themeable('blue'),
                // Status. red/amber/green are driven directly by the theme's
                // danger/warning/success; yellow and emerald shadow amber and
                // green, keeping their own hue offset so they stay distinct.
                red: themeable('red'),
                amber: themeable('amber'),
                yellow: themeable('yellow'),
                green: themeable('green'),
                emerald: themeable('emerald'),
                // Primary text (also toggle knobs and slider thumbs, which
                // intentionally follow the text colour).
                white: 'rgb(var(--r2-white) / <alpha-value>)',
                // Labels painted on an accent fill (primary buttons, selected
                // tabs). Chosen per-theme so a pale accent cannot swallow them.
                // A theme carries one text colour, but text lands on several
                // different fills. Each of these is resolved per theme against
                // the fill it names, so a label stays readable on a grey
                // secondary button and a red destructive one alike.
                'on-accent': 'rgb(var(--r2-on-accent) / <alpha-value>)',
                'on-surface': 'rgb(var(--r2-on-surface) / <alpha-value>)',
                'on-danger': 'rgb(var(--r2-on-danger) / <alpha-value>)',
                'on-warning': 'rgb(var(--r2-on-warning) / <alpha-value>)',
                'on-success': 'rgb(var(--r2-on-success) / <alpha-value>)',
                'accent-hover': 'rgb(var(--r2-accent-hover) / <alpha-value>)',
                'surface-hover': 'rgb(var(--r2-surface-hover) / <alpha-value>)',
                // Status colours as *text on a panel*. A fixed shade cannot do
                // this: `text-amber-200` assumes a dark panel and drops to
                // ~1.6:1 once the theme inverts. These are resolved per theme
                // against its surface, so they read either way.
                'fg-accent': 'rgb(var(--r2-fg-accent) / <alpha-value>)',
                'fg-danger': 'rgb(var(--r2-fg-danger) / <alpha-value>)',
                'fg-warning': 'rgb(var(--r2-fg-warning) / <alpha-value>)',
                'fg-success': 'rgb(var(--r2-fg-success) / <alpha-value>)',
                // Icon hues. Fixed identities — no theme colour drives them —
                // but routed through variables so the engine can mirror their
                // lightness under a light theme, keeping a glyph visible the
                // way a status-bar icon flips to suit what is behind it.
                purple: themeable('purple'),
                cyan: themeable('cyan'),
                violet: themeable('violet'),
                sky: themeable('sky'),
                indigo: themeable('indigo'),
                fuchsia: themeable('fuchsia'),
                rose: themeable('rose'),
                orange: themeable('orange'),
                teal: themeable('teal'),
                pink: themeable('pink'),
                lime: themeable('lime'),

                // Chrome that sits on top of artwork — cover images, screenshots
                // — rather than on an app surface. Deliberately NOT themed: the
                // picture underneath is arbitrary and usually vivid, so these
                // must contrast with *it*, not with the palette. Tying them to
                // the theme turns every badge white under a light theme and
                // leaves dark icons floating on bright cover art.
                scrim: 'rgb(var(--r2-scrim) / <alpha-value>)',
                'on-media': 'rgb(var(--r2-on-media) / <alpha-value>)',
                // `black` is deliberately left alone: it backs modal scrims,
                // which should stay neutral regardless of the active theme.
            },
            // Mod READMEs and changelogs render through `prose prose-invert`,
            // whose colours the typography plugin hardcodes. Remapped onto the
            // same tokens (identical default values) so long-form mod text
            // follows the theme like everything else.
            typography: {
                invert: {
                    css: {
                        '--tw-prose-invert-body': 'rgb(var(--r2-gray-300))',
                        '--tw-prose-invert-headings': 'rgb(var(--r2-white))',
                        '--tw-prose-invert-lead': 'rgb(var(--r2-gray-400))',
                        '--tw-prose-invert-links': 'rgb(var(--r2-white))',
                        '--tw-prose-invert-bold': 'rgb(var(--r2-white))',
                        '--tw-prose-invert-counters': 'rgb(var(--r2-gray-400))',
                        '--tw-prose-invert-bullets': 'rgb(var(--r2-gray-600))',
                        '--tw-prose-invert-hr': 'rgb(var(--r2-gray-700))',
                        '--tw-prose-invert-quotes': 'rgb(var(--r2-gray-100))',
                        '--tw-prose-invert-quote-borders': 'rgb(var(--r2-gray-700))',
                        '--tw-prose-invert-captions': 'rgb(var(--r2-gray-400))',
                        '--tw-prose-invert-code': 'rgb(var(--r2-white))',
                        '--tw-prose-invert-pre-code': 'rgb(var(--r2-gray-300))',
                        '--tw-prose-invert-th-borders': 'rgb(var(--r2-gray-600))',
                        '--tw-prose-invert-td-borders': 'rgb(var(--r2-gray-700))',
                    },
                },
            },
        },
    },
    plugins: [
        require('@tailwindcss/typography'),
    ],
}
