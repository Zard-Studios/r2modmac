import type { SVGProps } from 'react';

interface HeartIconProps extends SVGProps<SVGSVGElement> {}

function HeartIcon({ className, ...props }: HeartIconProps) {
    return (
        <svg
            xmlns="http://www.w3.org/2000/svg"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth={2}
            strokeLinecap="round"
            strokeLinejoin="round"
            aria-hidden="true"
            focusable="false"
            className={className}
            {...props}
        >
            <path d="M2 9.5a5.5 5.5 0 0 1 9.591-3.676.56.56 0 0 0 .818 0A5.49 5.49 0 0 1 22 9.5c0 2.29-1.5 4-3 5.5l-5.492 5.313a2 2 0 0 1-3 .019L5 15c-1.5-1.5-3-3.2-3-5.5" />
        </svg>
    );
}

interface LikeStatProps {
    count: number | null | undefined;
    className?: string;
    iconClassName?: string;
    title?: string;
}

export function LikeStat({
    count,
    className = '',
    iconClassName = 'w-3.5 h-3.5',
    title = 'Likes',
}: LikeStatProps) {
    const safeCount = typeof count === 'number' && Number.isFinite(count) ? count : 0;

    return (
        <span className={`inline-flex items-center ${className}`} title={title}>
            <HeartIcon className={iconClassName} />
            <span>{safeCount.toLocaleString()}</span>
        </span>
    );
}
