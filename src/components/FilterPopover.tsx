import { useState, useEffect, useRef } from 'react';

export interface FilterOptions {
    sort: string;
    sortDirection: 'asc' | 'desc';
    nsfw: boolean;
    deprecated: boolean;
    mods: boolean;
    modpacks: boolean;
    categories: string[];
}

interface FilterPopoverProps {
    options: FilterOptions;
    onChange: (options: FilterOptions) => void;
    availableCategories: string[];
}

// Special tags that filter by boolean fields, not categories
const SPECIAL_TAGS = ['Mods', 'Modpacks', 'NSFW', 'Deprecated'];

export function FilterPopover({ options, onChange, availableCategories }: FilterPopoverProps) {
    const [isOpen, setIsOpen] = useState(false);
    const popoverRef = useRef<HTMLDivElement>(null);

    useEffect(() => {
        function handleClickOutside(event: MouseEvent) {
            if (popoverRef.current && !popoverRef.current.contains(event.target as Node)) {
                setIsOpen(false);
            }
        }
        document.addEventListener("mousedown", handleClickOutside);
        return () => document.removeEventListener("mousedown", handleClickOutside);
    }, []);

    const updateOption = <K extends keyof FilterOptions>(key: K, value: FilterOptions[K]) => {
        onChange({ ...options, [key]: value });
    };

    const toggleCategory = (cat: string) => {
        const current = options.categories || [];
        if (current.includes(cat)) {
            updateOption('categories', current.filter(c => c !== cat));
        } else {
            updateOption('categories', [...current, cat]);
        }
    };

    const toggleSpecialTag = (tag: string) => {
        if (tag === 'NSFW') {
            updateOption('nsfw', !options.nsfw);
        } else if (tag === 'Deprecated') {
            updateOption('deprecated', !options.deprecated);
        } else if (tag === 'Mods') {
            updateOption('mods', !options.mods);
        } else if (tag === 'Modpacks') {
            updateOption('modpacks', !options.modpacks);
        }
    };

    const isSpecialTagActive = (tag: string) => {
        if (tag === 'NSFW') return options.nsfw;
        if (tag === 'Deprecated') return options.deprecated;
        if (tag === 'Mods') return options.mods;
        if (tag === 'Modpacks') return options.modpacks;
        return false;
    };

    const activeFilterCount = (options.nsfw ? 1 : 0) + (options.deprecated ? 1 : 0) + (options.mods ? 1 : 0) + (options.modpacks ? 1 : 0) + (options.categories?.length || 0);

    return (
        <div className="relative" ref={popoverRef}>
            <button
                onClick={() => setIsOpen(!isOpen)}
                className={`flex items-center gap-2 px-3 py-2 rounded-lg border transition-colors ${isOpen
                    ? 'bg-blue-600 border-blue-500 text-white'
                    : 'bg-gray-800 border-gray-700 text-gray-300 hover:text-white hover:bg-gray-700'
                    }`}
            >
                <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                    <polygon points="22 3 2 3 10 12.46 10 19 14 21 14 12.46 22 3" />
                </svg>
                <span>Filter</span>
                {activeFilterCount > 0 && (
                    <span className="bg-blue-500 text-white text-xs px-1.5 py-0.5 rounded-full">{activeFilterCount}</span>
                )}
            </button>

            {isOpen && (
                <div className="absolute top-full mt-2 right-0 w-96 max-h-[70vh] overflow-y-auto bg-gray-800 border border-gray-700 rounded-xl shadow-2xl z-50 p-5">
                    <div className="space-y-5">
                        {/* Sort By */}
                        <div>
                            <label className="block text-sm font-medium text-gray-400 mb-2">Sort By</label>
                            <select
                                value={options.sort}
                                onChange={(e) => updateOption('sort', e.target.value)}
                                className="w-full h-8 bg-gray-900 border border-gray-700 rounded-lg px-3 text-white focus:outline-none focus:border-blue-500"
                            >
                                <option value="downloads">Most Downloaded</option>
                                <option value="rating">Top Rated</option>
                                <option value="updated">Last Updated</option>
                                <option value="alphabetical">Alphabetical</option>
                            </select>
                        </div>

                        {/* Sort Direction */}
                        <div>
                            <label className="block text-sm font-medium text-gray-400 mb-2">Order</label>
                            <div className="flex bg-gray-900 rounded-lg p-1 border border-gray-700">
                                <button
                                    onClick={() => updateOption('sortDirection', 'desc')}
                                    className={`flex-1 py-2 px-3 rounded-md text-sm font-medium transition-colors ${options.sortDirection === 'desc'
                                        ? 'bg-gray-700 text-white shadow-sm'
                                        : 'text-gray-400 hover:text-gray-300'
                                        }`}
                                >
                                    Descending
                                </button>
                                <button
                                    onClick={() => updateOption('sortDirection', 'asc')}
                                    className={`flex-1 py-2 px-3 rounded-md text-sm font-medium transition-colors ${options.sortDirection === 'asc'
                                        ? 'bg-gray-700 text-white shadow-sm'
                                        : 'text-gray-400 hover:text-gray-300'
                                        }`}
                                >
                                    Ascending
                                </button>
                            </div>
                        </div>

                        {/* Tags Section */}
                        <div className="pt-4 border-t border-gray-700">
                            <label className="block text-sm font-medium text-gray-400 mb-3">Filter Tags</label>

                            {/* Custom Tag Input */}
                            <div className="flex gap-2 mb-3">
                                <input
                                    type="text"
                                    placeholder="Add custom filter..."
                                    className="flex-1 bg-gray-900 border border-gray-700 rounded-lg px-3 py-2 text-white text-sm placeholder-gray-500 focus:outline-none focus:border-blue-500"
                                    onKeyDown={(e) => {
                                        if (e.key === 'Enter') {
                                            const input = e.target as HTMLInputElement;
                                            const value = input.value.trim();
                                            if (value && !options.categories?.includes(value) && !SPECIAL_TAGS.includes(value)) {
                                                updateOption('categories', [...(options.categories || []), value]);
                                            }
                                            input.value = '';
                                        }
                                    }}
                                />
                            </div>

                            {/* Active Tags (Special + Custom) */}
                            {activeFilterCount > 0 && (
                                <div className="flex flex-wrap gap-2 mb-3">
                                    {/* Active Special Tags */}
                                    {SPECIAL_TAGS.filter(tag => isSpecialTagActive(tag)).map(tag => (
                                        <button
                                            key={tag}
                                            onClick={() => toggleSpecialTag(tag)}
                                            className="px-3 py-1.5 rounded-full text-sm font-medium bg-red-600 text-white shadow-lg flex items-center gap-1"
                                        >
                                            #{tag}
                                            <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                                                <line x1="18" y1="6" x2="6" y2="18"></line>
                                                <line x1="6" y1="6" x2="18" y2="18"></line>
                                            </svg>
                                        </button>
                                    ))}
                                    {/* Active Custom Tags */}
                                    {options.categories?.map(cat => (
                                        <button
                                            key={cat}
                                            onClick={() => toggleCategory(cat)}
                                            className="px-3 py-1.5 rounded-full text-sm font-medium bg-blue-600 text-white shadow-lg flex items-center gap-1"
                                        >
                                            #{cat}
                                            <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                                                <line x1="18" y1="6" x2="6" y2="18"></line>
                                                <line x1="6" y1="6" x2="18" y2="18"></line>
                                            </svg>
                                        </button>
                                    ))}
                                    <button
                                        onClick={() => {
                                            updateOption('categories', []);
                                            updateOption('nsfw', false);
                                            updateOption('deprecated', false);
                                        }}
                                        className="text-sm text-gray-400 hover:text-white transition-colors"
                                    >
                                        Clear all
                                    </button>
                                </div>
                            )}

                            {/* All Available Tags (Special + Categories) */}
                            <div className="flex flex-wrap gap-2">
                                {/* Special Tags First */}
                                {SPECIAL_TAGS.filter(tag => !isSpecialTagActive(tag)).map(tag => (
                                    <button
                                        key={tag}
                                        onClick={() => toggleSpecialTag(tag)}
                                        className="px-3 py-1.5 rounded-full text-sm font-medium bg-gray-700 text-gray-300 hover:bg-gray-600 hover:text-white transition-all"
                                    >
                                        #{tag}
                                    </button>
                                ))}
                                {/* Then Categories */}
                                {availableCategories.filter(cat => !options.categories?.includes(cat)).map(cat => (
                                    <button
                                        key={cat}
                                        onClick={() => toggleCategory(cat)}
                                        className="px-3 py-1.5 rounded-full text-sm font-medium bg-gray-700 text-gray-300 hover:bg-gray-600 hover:text-white transition-all"
                                    >
                                        #{cat}
                                    </button>
                                ))}
                            </div>
                        </div>
                    </div>
                </div>
            )}
        </div>
    );
}
