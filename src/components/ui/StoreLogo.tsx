import type { ModSource } from '../../types/thunderstore';
import { sourceName } from '../../utils/storeProviders';

export function StoreLogo({ source, className = 'h-4 w-4' }: { source?: ModSource; className?: string }) {
    const name = sourceName(source);
    if (source === 'hexium') {
        return (
            <svg className={className} viewBox="0 0 32 32" role="img" aria-label={name}>
                <title>{name}</title>
                <path d="M16 2.5 28 9.25v13.5L16 29.5 4 22.75V9.25Z" fill="#6d28d9" stroke="#a78bfa" strokeWidth="1.5" />
                <path d="M10 9v14h4.2v-5.2h3.6V23H22V9h-4.2v5.1h-3.6V9Z" fill="white" />
            </svg>
        );
    }
    if (source === 'outerwilds') {
        return (
            <svg className={className} viewBox="0 0 32 32" role="img" aria-label={name}>
                <title>{name}</title>
                <circle cx="16" cy="16" r="13" fill="#d97706" />
                <path d="M16 7a9 9 0 1 0 0 18 9 9 0 0 0 0-18Zm0 4a5 5 0 1 1 0 10 5 5 0 0 1 0-10Z" fill="white" />
            </svg>
        );
    }
    return (
        <svg className={`${className} text-teal-400`} viewBox="0 0 1000 896" role="img" aria-label={name}>
            <title>{name}</title>
            <path
                d="M13.4223 496.845L209.485 838.17L300 650.202L200.99 477.966C189.992 458.897 189.992 436.945 200.99 417.779L324.555 202.755C335.561 183.611 354.447 172.666 376.421 172.675H442.857L314.286 462.366H473.143L257.143 881.384L690.941 361.014H557.588L648.593 172.675H808.03H900.762L1000 2.28882e-05H715.868H526.836H298.96C263.138 0.0084323 232.393 17.8324 214.461 48.9346L13.4223 398.9C-4.46781 430.078 -4.48036 465.827 13.4223 496.845ZM313.959 895.833H701.066C736.813 895.833 767.63 878.005 785.612 846.819L986.655 496.836C1004.44 465.827 1004.44 430.078 986.655 398.892L906.26 258.947H707.808L799.079 417.779C809.985 436.961 809.984 458.91 799.049 477.974L675.531 693.049C664.454 712.222 645.555 723.15 623.555 723.15H533.795L471.429 722.446L313.959 895.833Z"
                fill="currentColor"
            />
        </svg>
    );
}
