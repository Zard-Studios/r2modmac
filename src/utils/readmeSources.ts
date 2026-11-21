import type { ModSource } from '../types/thunderstore';
import type { RichTextSource } from './richText';
import { sourceName } from './storeProviders.ts';

/**
 * A mod can have a README on Thunderstore, on GitHub, on both or on neither.
 * These helpers decide whether one is worth showing and make a GitHub README
 * work outside GitHub.
 */

/**
 * Where a README can come from: the store a mod is listed on, or the GitHub
 * repository the mod points to. A new store or host is one more id here, one
 * more name below and one more logo in `StoreLogo`.
 */
export type ReadmeSourceId = ModSource | 'github';

export function readmeSourceName(id: ReadmeSourceId): string {
    return id === 'github' ? 'GitHub' : sourceName(id);
}

/** The sources a mod's README can be read from, the store's first. */
export function availableReadmeSources(store: ModSource | undefined, hasGithubRepo: boolean): ReadmeSourceId[] {
    return hasGithubRepo ? [store ?? 'thunderstore', 'github'] : [store ?? 'thunderstore'];
}

/** Below this much visible text a README is taken as empty. */
export const MIN_USEFUL_README_CHARS = 200;

/**
 * How much of a README a reader can actually read: its text, without the
 * markup, the addresses or the pictures.
 */
export function visibleTextLength(source: RichTextSource): number {
    const text =
        source.html !== undefined
            ? source.html.replace(/<[^>]*>/g, ' ')
            : (source.markdown ?? '')
                  .replace(/!\[[^\]]*\]\([^)]*\)/g, ' ')
                  .replace(/\[([^\]]*)\]\([^)]*\)/g, '$1')
                  .replace(/<[^>]*>/g, ' ')
                  .replace(/[#>*_`~\\|-]+/g, ' ');
    return text
        .replace(/&[a-z#0-9]+;/gi, ' ')
        .replace(/\s+/g, ' ')
        .trim().length;
}

/** Whether a README says enough to be worth showing. */
export function hasUsefulReadme(source: RichTextSource | null): boolean {
    return source !== null && visibleTextLength(source) >= MIN_USEFUL_README_CHARS;
}

/** Where the relative pictures and links of a GitHub README point to. */
export function githubBases(owner: string, repo: string): { image: string; link: string } {
    return {
        image: `https://raw.githubusercontent.com/${owner}/${repo}/HEAD/`,
        link: `https://github.com/${owner}/${repo}/blob/HEAD/`,
    };
}

/**
 * A relative address made absolute against a base, the way GitHub does when it
 * shows a README (`docs/a.png` and `/docs/a.png` both mean the repository's
 * own `docs/a.png`). Addresses that are already complete, or that only point
 * inside the page, stay as they are, and so does one that would climb out of
 * the repository.
 */
export function resolveAgainst(value: string, base: string): string {
    const text = value.trim();
    if (!text || text.startsWith('#') || text.startsWith('//') || /^[a-z][a-z0-9+.-]*:/i.test(text)) return value;
    try {
        const resolved = new URL(text.replace(/^\.?\/+/, ''), base).toString();
        return resolved.startsWith(base) ? resolved : value;
    } catch {
        return value;
    }
}
