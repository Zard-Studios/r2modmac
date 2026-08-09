import { AppIcon } from './icons';

export function Checkbox({
    checked,
    onChange,
    label,
    disabled = false,
}: {
    checked: boolean;
    onChange: (checked: boolean) => void;
    label: string;
    disabled?: boolean;
}) {
    return (
        <label className="group flex cursor-pointer items-center gap-2.5 text-sm text-gray-400 transition-colors hover:text-gray-300 has-[:disabled]:cursor-not-allowed has-[:disabled]:opacity-50">
            <input
                type="checkbox"
                checked={checked}
                onChange={(event) => onChange(event.target.checked)}
                disabled={disabled}
                className="peer sr-only"
            />
            <span
                className={`flex h-5 w-5 shrink-0 items-center justify-center rounded-md border transition-colors peer-focus-visible:ring-2 peer-focus-visible:ring-blue-500/60 peer-focus-visible:ring-offset-2 peer-focus-visible:ring-offset-gray-900 ${
                    checked
                        ? 'border-blue-500 bg-blue-600 text-on-accent'
                        : 'border-gray-600 bg-gray-800 text-transparent group-hover:border-gray-500'
                }`}
                aria-hidden="true"
            >
                <AppIcon name="apply" className="h-3.5 w-3.5" strokeWidth={2.5} />
            </span>
            <span>{label}</span>
        </label>
    );
}
