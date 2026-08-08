/**
 * The app's switch.
 *
 * Lives here rather than inside a screen because it was previously copied: the
 * theme editor grew its own, which drifted — no hover state, a different easing
 * and a flatter shadow — so two switches sat two panels apart looking subtly
 * unlike each other. One component means they cannot diverge again.
 *
 * The knob is a fixed white rather than the theme's text colour on purpose: it
 * sits on a filled track, so it has to contrast with that fill, not with the
 * page behind it.
 */
export function Toggle({
    value,
    onChange,
    label,
    disabled = false,
}: {
    value: boolean;
    onChange: (next: boolean) => void;
    label?: string;
    disabled?: boolean;
}) {
    return (
        <button
            type="button"
            role="switch"
            onClick={() => onChange(!value)}
            disabled={disabled}
            className={`relative w-11 h-6 rounded-full transition-colors duration-200 ease-in-out flex-shrink-0 focus:outline-none focus:ring-2 focus:ring-blue-500/50 disabled:cursor-not-allowed disabled:opacity-50 ${
                value ? 'bg-blue-600' : 'bg-gray-700 hover:bg-gray-600'
            }`}
            aria-checked={value}
            aria-label={label}
        >
            <span
                className={`absolute top-0.5 left-0.5 w-5 h-5 bg-[#ffffff] rounded-full shadow-[0_2px_5px_rgba(0,0,0,0.2)] transition-transform duration-200 ease-[cubic-bezier(0.4,0,0.2,1)] ${
                    value ? 'translate-x-5' : 'translate-x-0'
                }`}
            />
        </button>
    );
}
