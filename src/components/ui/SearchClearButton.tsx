import { AppIcon } from './icons';

interface SearchClearButtonProps {
    filled: boolean;
    onClear: () => void;
    className?: string;
    iconClassName?: string;
}

export function SearchClearButton({ filled, onClear, className = '', iconClassName = 'h-5 w-5' }: SearchClearButtonProps) {
    const transition = 'transition-[opacity,transform] duration-200 ease-out motion-reduce:transition-none';
    return (
        <button
            type="button"
            onClick={(event) => {
                const input = event.currentTarget.parentElement?.querySelector('input');
                onClear();
                input?.focus();
            }}
            disabled={!filled}
            aria-label="Clear search"
            tabIndex={filled ? 0 : -1}
            className={`${className} ${filled ? 'cursor-pointer text-fg-accent hover:text-white' : 'pointer-events-none text-gray-500'}`}
        >
            <span className={`relative block ${iconClassName}`} aria-hidden="true">
                <span className={`absolute inset-0 ${transition} ${filled ? '-rotate-45 scale-75 opacity-0' : 'rotate-0 scale-100 opacity-100'}`}>
                    <AppIcon name="search" className="h-full w-full" strokeWidth={2} />
                </span>
                <span className={`absolute inset-0 ${transition} ${filled ? 'rotate-0 scale-100 opacity-100' : 'rotate-45 scale-75 opacity-0'}`}>
                    <AppIcon name="close" className="h-full w-full" strokeWidth={2} />
                </span>
            </span>
        </button>
    );
}
