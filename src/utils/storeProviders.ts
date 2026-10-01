import type { ModSource } from '../types/thunderstore';

/** The stores the browse list can be narrowed to. */
export type FilterProvider = Extract<ModSource, 'thunderstore' | 'hexium'>;

/** Both are on by default. */
export const ALL_PROVIDERS: FilterProvider[] = ['thunderstore', 'hexium'];

export const sourceName = (source?: ModSource) => {
    if (source === 'hexium') return 'Hexium';
    if (source === 'outerwilds') return 'Outer Wilds Mods';
    return 'Thunderstore';
};

/**
 * The list of stores after one is switched. At least one always stays on, so
 * the list is never emptied by a stray click, and the order is stable.
 */
export function toggleProvider(
    current: FilterProvider[],
    provider: FilterProvider,
): FilterProvider[] {
    if (current.includes(provider)) {
        return current.length === 1 ? current : current.filter(candidate => candidate !== provider);
    }
    return ALL_PROVIDERS.filter(candidate => candidate === provider || current.includes(candidate));
}

/** Does the filter hide anything? Both stores on is the same as no filter. */
export function isProviderFilterActive(selected: FilterProvider[]): boolean {
    return ALL_PROVIDERS.some(provider => !selected.includes(provider));
}
