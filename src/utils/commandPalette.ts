import { fuzzyScore } from './fuzzy.ts';

/**
 * The command palette's model.
 *
 * Everything the palette can offer — a game, a profile, a settings panel, an
 * action on the open profile — is one flat kind of thing with a `run`. Views
 * contribute the items they know how to perform; ranking, grouping and slash
 * commands happen here, once, with no React in sight.
 */

export type CommandGroup = 'Actions' | 'Profiles' | 'Games' | 'Settings';

/** Group order in the results, most immediately useful first. */
export const GROUP_ORDER: readonly CommandGroup[] = ['Actions', 'Profiles', 'Games', 'Settings'];

export interface CommandItem {
    /** Stable across rebuilds, so the highlight survives a re-render. */
    id: string;
    title: string;
    subtitle?: string;
    group: CommandGroup;
    /** Picks the glyph the row is drawn with. */
    icon?: 'play' | 'stop' | 'apply' | 'profile' | 'game' | 'settings' | 'theme' | 'keyboard' | 'plus' | 'copy' | 'browse';
    /**
     * Typed after a slash to reach this item directly: `/launch`. Only items
     * carrying one can be reached in slash mode, which is what keeps `/` a list
     * of things that *do* something rather than everything in the app.
     */
    slash?: string;
    /** Shown right-aligned, e.g. the keyboard shortcut that also runs this. */
    hint?: string;
    /** Marked as where the user already is. */
    current?: boolean;
    run: () => void;
}

/** A group with its matching items, ready to render. */
export interface CommandSection {
    group: CommandGroup;
    items: CommandItem[];
}

/**
 * At most this many per group.
 *
 * A palette is for reaching one thing quickly; a full game list scrolled past
 * the fold buries the profile the user actually wanted underneath it.
 */
export const MAX_PER_GROUP = 6;

export interface ParsedQuery {
    /** True once the query opens with a slash. */
    slashMode: boolean;
    /** What to match against — the text after the slash, or the whole query. */
    term: string;
}

/**
 * Split a raw query into how to search and what to search for.
 *
 * A bare `/` is slash mode with an empty term, which lists every command: that
 * is how the feature announces itself to someone who just typed a slash to see
 * what happens.
 */
export function parseQuery(raw: string): ParsedQuery {
    const trimmed = raw.trimStart();
    if (!trimmed.startsWith('/')) {
        return { slashMode: false, term: raw.trim() };
    }
    return { slashMode: true, term: trimmed.slice(1).trim() };
}

/**
 * Rank one item against a parsed query, or null when it does not belong.
 *
 * In slash mode only the slash token is matched — someone typing `/lau` means
 * the launch command, not a profile that happens to contain those letters.
 */
export function scoreItem(item: CommandItem, query: ParsedQuery): number | null {
    if (query.slashMode) {
        if (!item.slash) return null;
        return fuzzyScore(query.term, item.slash);
    }

    const onTitle = fuzzyScore(query.term, item.title);
    if (onTitle !== null) return onTitle;

    // The subtitle is a weaker match: a game found by its identifier should
    // still rank below one found by name.
    const onSubtitle = item.subtitle ? fuzzyScore(query.term, item.subtitle) : null;
    return onSubtitle === null ? null : onSubtitle - 12;
}

/**
 * The grouped, ranked, capped results for a query.
 *
 * `scope` narrows the palette to one group. Searching profiles from the profile
 * page is a different job from searching the whole app: offering to launch a
 * game or open Preferences there would be answering a question nobody asked.
 * Scoped searching also drops slash commands, so a `/` typed into it is just a
 * character in a profile name.
 *
 * Empty groups are dropped rather than rendered as bare headings.
 */
export function buildSections(
    items: CommandItem[],
    raw: string,
    scope: CommandGroup | null = null
): CommandSection[] {
    const query = scope ? { slashMode: false, term: raw.trim() } : parseQuery(raw);

    const scored = items
        .map((item) => ({ item, score: scoreItem(item, query) }))
        .filter((entry): entry is { item: CommandItem; score: number } => entry.score !== null);

    return (scope ? [scope] : GROUP_ORDER).map((group) => ({
        group,
        items: scored
            .filter((entry) => entry.item.group === group)
            // A stable tiebreak on title keeps the order from shifting under the
            // cursor between two items that score the same.
            .sort((a, b) => b.score - a.score || a.item.title.localeCompare(b.item.title))
            .slice(0, MAX_PER_GROUP)
            .map((entry) => entry.item),
    })).filter((section) => section.items.length > 0);
}

/** The sections flattened back into the order the arrow keys walk. */
export function flattenSections(sections: CommandSection[]): CommandItem[] {
    return sections.flatMap((section) => section.items);
}

/**
 * Move the highlight by `step`, wrapping at both ends.
 *
 * Wrapping matters more than it sounds: the fastest way to the last command is
 * one press of Up, and a palette that stops dead at the top loses that.
 */
export function moveHighlight(current: number, step: number, count: number): number {
    if (count === 0) return 0;
    return (current + step + count) % count;
}
