import type { IconName } from '../components/ui/icons';
import type { KeybindActionId } from './keybinds';
import { fuzzyScore } from './fuzzy.ts';

/**
 * The command palette's model.
 *
 * Everything the palette can offer — a game, a profile, a settings panel, an
 * action on the open profile — is one flat kind of thing with a `run`. Views
 * contribute the items they know how to perform; ranking and grouping happen
 * here, once, with no React in sight.
 */

export type CommandGroup = 'Actions' | 'Profiles' | 'Games' | 'Settings';

/** Home starts with what can be selected there: games, then profiles. */
export const GROUP_ORDER: readonly CommandGroup[] = ['Games', 'Profiles', 'Actions', 'Settings'];

export interface CommandItem {
    /** Stable across rebuilds, so the highlight survives a re-render. */
    id: string;
    title: string;
    subtitle?: string;
    group: CommandGroup;
    /** Picks the glyph the row is drawn with, when there is no artwork. */
    icon?: IconName;
    /**
     * Cover art for the row's tile. A game is far quicker to recognise by its
     * artwork than by a glyph that is identical for every entry.
     */
    image?: string;
    /**
     * A small circle over the tile's corner. A profile shows the game's cover
     * with its own picture on top, which says both what it is and which game it
     * belongs to without reading a word.
     */
    badge?: {
        image?: string;
        /** Drawn when there is no picture, over `gradient`. */
        initial?: string;
        gradient?: string;
    };
    /** Shown right-aligned, e.g. the keyboard shortcut that also runs this. */
    hint?: string;
    /** Runs this row when its configured shortcut is pressed inside Spotlight. */
    shortcut?: KeybindActionId;
    /** Marked as where the user already is. */
    current?: boolean;
    /** The game this belongs to, so a scoped search can narrow to one. */
    game?: string;
    /** The profile this action belongs to. */
    profile?: string;
    /** Selecting a game/profile can drill into its context without closing. */
    nextScope?: CommandScope;
    /** Hidden from the universal list; useful only after a context is pinned. */
    contextOnly?: boolean;
    run: () => void;
}

/** The game a scoped search is pinned to, shown as a removable tag. */
export interface GameTag {
    identifier: string;
    name: string;
    image?: string;
}

/** The profile selected inside a game context. */
export interface ProfileTag {
    identifier: string;
    name: string;
    image?: string;
    initial?: string;
    gradient?: string;
}

/**
 * A narrowed palette.
 *
 * The magnifier on a game's profile page opens the search already pinned to
 * that game — the question there is always "which of *these* profiles", never
 * "which of all of them". Clearing the tag widens it back to everything, so
 * the narrow case is a starting point rather than a separate feature.
 */
export interface CommandScope {
    group: CommandGroup;
    game?: GameTag;
    profile?: ProfileTag;
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

/** Rank one item by its visible name, or null when it does not match. */
export function scoreItem(item: CommandItem, term: string): number | null {
    const onTitle = fuzzyScore(term, item.title);
    if (onTitle !== null) return onTitle;

    // The subtitle is a weaker match: a game found by its identifier should
    // still rank below one found by name.
    const onSubtitle = item.subtitle ? fuzzyScore(term, item.subtitle) : null;
    return onSubtitle === null ? null : onSubtitle - 12;
}

/**
 * The grouped, ranked, capped results for a query.
 *
 * `scope` narrows the palette to one group. Searching profiles from the profile
 * page is a different job from searching the whole app: offering to launch a
 * game or open Preferences there would be answering a question nobody asked.
 * Empty groups are dropped rather than rendered as bare headings.
 */
export function buildSections(
    items: CommandItem[],
    raw: string,
    scope: CommandScope | null = null
): CommandSection[] {
    const term = raw.trim();

    const eligible = scope?.profile
        ? items.filter((item) => item.profile === scope.profile!.identifier)
        : scope?.game
          ? items.filter((item) =>
                item.game === scope.game!.identifier &&
                // A game tag may list profiles, but it cannot expose actions
                // for one of them until that profile itself is selected.
                !(item.group === 'Actions' && item.profile)
            )
          : items.filter((item) => !item.contextOnly);

    const scored = eligible
        .map((item) => ({ item, score: scoreItem(item, term) }))
        .filter((entry): entry is { item: CommandItem; score: number } => entry.score !== null);

    // A game pin is context, not a request to hide everything except one
    // result type. Profile management actions (new/import) belong beside that
    // game's profiles. A plain group scope keeps the older one-group behaviour.
    const scopedGroups: readonly CommandGroup[] = scope?.profile
        ? ['Actions']
        : scope?.game
        ? Array.from(new Set<CommandGroup>(['Actions', scope.group]))
        : scope
          ? [scope.group]
          : GROUP_ORDER;

    return scopedGroups.map((group) => ({
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

/** Resolve a configured shortcut only among rows available in this context. */
export function findShortcutItem(
    items: CommandItem[],
    shortcut: KeybindActionId,
    scope: CommandScope | null = null
): CommandItem | undefined {
    return flattenSections(buildSections(items, '', scope)).find((item) => item.shortcut === shortcut);
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
