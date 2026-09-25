import { SearchClearButton } from './ui/SearchClearButton';

interface SearchBarProps {
    value: string;
    onChange: (value: string) => void;
    placeholder?: string;
}

export function SearchBar({ value, onChange, placeholder = "Search mods..." }: SearchBarProps) {
    return (
        <div className="relative group">
            <SearchClearButton filled={!!value} onClear={() => onChange('')} className="absolute inset-y-0 left-0 flex items-center pl-3 group-focus-within:text-fg-accent" />
            <input
                type="text"
                value={value}
                onChange={(e) => onChange(e.target.value)}
                placeholder={placeholder}
                spellCheck={false}
                autoCorrect="off"
                autoCapitalize="none"
                autoComplete="off"
                className="w-full pl-10 pr-4 py-2.5 bg-gray-900/50 border border-gray-700 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-blue-500 focus:ring-1 focus:ring-blue-500 transition-all duration-200"
            />
        </div>
    );
}
