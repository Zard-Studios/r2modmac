/**
 * Subsequence matching for the command palette.
 *
 * Lives apart from what searches with it: profiles, games, settings rows and
 * slash commands all rank through the same scorer, and none of them is the
 * one it belongs to.
 */

/**
 * Score `candidate` against a subsequence `query`, or null when it does not
 * match at all.
 *
 * Higher is better. Runs of adjacent characters and matches at a word boundary
 * score more, so typing "bl" puts "Best Lethal" above "Bumbling", which is what
 * someone reaching for a profile by initials expects.
 */
export function fuzzyScore(query: string, candidate: string): number | null {
    if (!query) return 0;

    const needle = query.toLowerCase();
    const haystack = candidate.toLowerCase();

    let score = 0;
    let cursor = 0;
    let previousIndex = -1;

    for (const character of needle) {
        const index = haystack.indexOf(character, cursor);
        if (index === -1) return null;

        if (index === previousIndex + 1) score += 8;
        if (index === 0 || /[\s\-_/.]/.test(haystack[index - 1])) score += 5;
        score -= Math.min(index - cursor, 6);

        previousIndex = index;
        cursor = index + 1;
    }

    // Prefer the tighter of two equally-ordered matches.
    return score - Math.max(0, candidate.length - query.length) * 0.05;
}
