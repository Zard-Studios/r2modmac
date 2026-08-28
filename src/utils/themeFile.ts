import {
    COVER_COLOR_KEYS,
    MANUAL_COLOR_KEYS,
    THEME_COLOR_KEYS,
    type Theme,
    type ThemeColors,
} from './theme.ts';

/**
 * Editing a theme file in place.
 *
 * The visual editor used to save by regenerating the whole file from the theme
 * it had in memory. That theme has been through `normalizeTheme`, which fills
 * every colour it does not find from the defaults — so a file holding three
 * colours and a page of the author's own comments came back holding twenty
 * colours and the app's boilerplate. Partial files are a supported, documented
 * way to write a theme, and the TOML editor deliberately preserves whatever it
 * does not understand; the visual side quietly did the opposite.
 *
 * So the visual editor now describes its work as a list of changes against the
 * theme it loaded, and those changes are applied to the file's own text. A
 * colour the user never touched is never written, and everything between the
 * keys — comments, ordering, blank lines, keys this app has never heard of —
 * survives untouched.
 *
 * The patcher is deliberately line-based rather than a full TOML parser. This
 * format is flat: sections of `key = value`. Values spanning several lines, or
 * inline tables, are not something either editor can produce, and a key it
 * cannot find is appended rather than corrupting what is there.
 */

/** Where a key lives. The empty string is the file's own top level. */
type Section = '' | 'colors' | 'opacity' | 'options' | 'icons' | 'background_image';

export type ThemeEdit =
    | { kind: 'set'; section: Section; key: string; value: string | number | boolean }
    | { kind: 'unset'; section: Section; key: string }
    | { kind: 'drop-section'; section: Section };

const ALL_COLOR_KEYS = [
    ...THEME_COLOR_KEYS,
    ...MANUAL_COLOR_KEYS,
    ...COVER_COLOR_KEYS,
] as const;

const BACKGROUND_KEYS = [
    'path',
    'opacity',
    'blur',
    'fit',
    'offset_x',
    'offset_y',
    'tile_scale',
] as const;

/** Whole numbers stay whole; anything else keeps two decimals, as before. */
function round2(value: number): number {
    return Math.round(value * 100) / 100;
}

/**
 * The changes that turn `base` into `next`.
 *
 * `base` is the theme as it was read from the file, so a value equal to it
 * produces no edit at all — which is what keeps an untouched key out of the
 * saved file even when the theme in memory carries a default for it.
 */
export function themeEdits(base: Theme, next: Theme): ThemeEdit[] {
    const edits: ThemeEdit[] = [];

    if (next.name !== base.name) {
        edits.push({ kind: 'set', section: '', key: 'name', value: next.name });
    }
    if (next.author !== base.author) {
        edits.push(
            next.author
                ? { kind: 'set', section: '', key: 'author', value: next.author }
                : { kind: 'unset', section: '', key: 'author' }
        );
    }

    for (const key of ALL_COLOR_KEYS) {
        const from = base.colors[key as keyof ThemeColors];
        const to = next.colors[key as keyof ThemeColors];
        if (from === to) continue;
        edits.push(
            to
                ? { kind: 'set', section: 'colors', key, value: to }
                : { kind: 'unset', section: 'colors', key }
        );
    }

    for (const key of ALL_COLOR_KEYS) {
        const from = base.opacity?.[key as keyof ThemeColors];
        const to = next.opacity?.[key as keyof ThemeColors];
        if (from === to) continue;
        edits.push(
            typeof to === 'number'
                ? { kind: 'set', section: 'opacity', key, value: round2(to) }
                : { kind: 'unset', section: 'opacity', key }
        );
    }

    if (next.options?.autoContrast !== base.options?.autoContrast) {
        edits.push({
            kind: 'set',
            section: 'options',
            key: 'auto_contrast',
            value: next.options?.autoContrast ?? true,
        });
    }
    if (next.options?.adaptSvg !== base.options?.adaptSvg) {
        edits.push({
            kind: 'set',
            section: 'options',
            key: 'adapt_svg',
            value: next.options?.adaptSvg ?? true,
        });
    }
    if (next.options?.interfaceBlur !== base.options?.interfaceBlur) {
        edits.push({
            kind: 'set',
            section: 'options',
            key: 'interface_blur',
            value: Math.round(next.options?.interfaceBlur ?? 0),
        });
    }

    const iconNames = new Set([
        ...Object.keys(base.icons ?? {}),
        ...Object.keys(next.icons ?? {}),
    ]);
    for (const name of [...iconNames].sort()) {
        const from = base.icons?.[name];
        const to = next.icons?.[name];
        if (from === to) continue;
        edits.push(
            to
                ? { kind: 'set', section: 'icons', key: name, value: to }
                : { kind: 'unset', section: 'icons', key: name }
        );
    }

    if (!next.backgroundImage) {
        // Dropping the whole section rather than each key: a `[background_image]`
        // left behind with no path is exactly the half-configured state the
        // loader throws away, and it would sit in the file confusing anyone
        // reading it.
        if (base.backgroundImage) edits.push({ kind: 'drop-section', section: 'background_image' });
    } else {
        for (const key of BACKGROUND_KEYS) {
            const from = base.backgroundImage?.[key];
            const to = next.backgroundImage[key];
            if (from === to || to === undefined) continue;
            edits.push({
                kind: 'set',
                section: 'background_image',
                key,
                value: typeof to === 'number' ? round2(to) : to,
            });
        }
    }

    return edits;
}

// ── Writing the changes into the file's own text ─────────────────────────────

interface Row {
    text: string;
    /** The section this line sits in; a header belongs to the one it opens. */
    section: string;
    /** The key assigned on this line, if it is an assignment. */
    key: string | null;
    isHeader: boolean;
}

const HEADER = /^\[([^\]]+)\]/;
const ASSIGNMENT = /^([A-Za-z0-9_-]+)\s*=/;

function scan(source: string): Row[] {
    let section = '';
    return source.split('\n').map((text) => {
        const trimmed = text.trim();
        const header = HEADER.exec(trimmed);
        if (header) {
            section = header[1].trim();
            return { text, section, key: null, isHeader: true };
        }
        if (trimmed.startsWith('#') || trimmed === '') {
            return { text, section, key: null, isHeader: false };
        }
        const assignment = ASSIGNMENT.exec(trimmed);
        return { text, section, key: assignment ? assignment[1] : null, isHeader: false };
    });
}

function literal(value: string | number | boolean): string {
    if (typeof value === 'string') {
        return `"${value.replace(/\\/g, '\\\\').replace(/"/g, '\\"')}"`;
    }
    return String(value);
}

/**
 * Swap the value on an assignment line, leaving everything else alone.
 *
 * Keeps the key's alignment padding and any trailing comment: those are the
 * author's, and rewriting the line wholesale is what this module exists to
 * avoid. A `#` inside a quoted string is part of the value, not a comment, so
 * strings are walked to their closing quote rather than cut at the first hash.
 */
function replaceValue(line: string, value: string): string {
    const equals = line.indexOf('=');
    if (equals === -1) return line;

    const head = line.slice(0, equals + 1);
    const tail = line.slice(equals + 1);
    const gap = /^[ \t]*/.exec(tail)?.[0] ?? '';
    const rest = tail.slice(gap.length);

    let end: number;
    if (rest.startsWith('"')) {
        end = 1;
        while (end < rest.length) {
            if (rest[end] === '\\') { end += 2; continue; }
            if (rest[end] === '"') { end += 1; break; }
            end += 1;
        }
    } else {
        const hash = rest.indexOf('#');
        end = hash === -1 ? rest.length : hash;
        while (end > 0 && /\s/.test(rest[end - 1])) end -= 1;
    }

    return `${head}${gap || ' '}${value}${rest.slice(end)}`;
}

/** Where a new key belongs: after the section's last assignment. */
function insertionPoint(rows: Row[], section: string): number | null {
    let last: number | null = null;
    let header: number | null = null;
    for (let i = 0; i < rows.length; i += 1) {
        if (rows[i].section !== section) continue;
        if (rows[i].isHeader) header = i;
        else if (rows[i].key !== null) last = i;
    }
    if (last !== null) return last + 1;
    if (header !== null) return header + 1;
    // The top level needs no header, so an empty file still has a home for it.
    return section === '' ? 0 : null;
}

/** Apply the visual editor's changes to a theme file, keeping the rest of it. */
export function patchThemeToml(source: string, edits: ThemeEdit[]): string {
    const rows = scan(source);

    for (const edit of edits) {
        if (edit.kind === 'drop-section') {
            for (let i = rows.length - 1; i >= 0; i -= 1) {
                if (rows[i].section === edit.section) rows.splice(i, 1);
            }
            continue;
        }

        const at = rows.findIndex((row) => row.section === edit.section && row.key === edit.key);

        if (edit.kind === 'unset') {
            if (at !== -1) rows.splice(at, 1);
            continue;
        }

        const value = literal(edit.value);
        if (at !== -1) {
            rows[at] = { ...rows[at], text: replaceValue(rows[at].text, value) };
            continue;
        }

        const row: Row = {
            text: `${edit.key} = ${value}`,
            section: edit.section,
            key: edit.key,
            isHeader: false,
        };
        const point = insertionPoint(rows, edit.section);
        if (point !== null) {
            rows.splice(point, 0, row);
        } else {
            // A section the file has never had. Opened at the end, after a blank
            // line, so it reads as a new block rather than running on from the
            // last one.
            while (rows.length > 0 && rows[rows.length - 1].text.trim() === '') rows.pop();
            rows.push(
                { text: '', section: edit.section, key: null, isHeader: false },
                { text: `[${edit.section}]`, section: edit.section, key: null, isHeader: true },
                row,
                { text: '', section: edit.section, key: null, isHeader: false }
            );
        }
    }

    return rows.map((row) => row.text).join('\n');
}
