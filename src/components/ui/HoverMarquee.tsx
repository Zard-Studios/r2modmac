import { useCallback, useLayoutEffect, useRef, useState, type CSSProperties } from 'react';

interface HoverMarqueeProps {
    text: string;
    className?: string;
    /** Defer measurement until hover/focus for large lists. */
    lazy?: boolean;
}

type MarqueeStyle = CSSProperties & {
    '--marquee-distance': string;
    '--marquee-duration': string;
};

export function HoverMarquee({ text, className = '', lazy = false }: HoverMarqueeProps) {
    const containerRef = useRef<HTMLDivElement>(null);
    const measurementRef = useRef<HTMLSpanElement>(null);
    const [metrics, setMetrics] = useState({ overflowing: false, distance: 0 });

    const measure = useCallback(() => {
        const container = containerRef.current;
        const content = measurementRef.current;
        if (!container || !content) return;
        const textWidth = content.getBoundingClientRect().width;
        const nextMetrics = {
            overflowing: textWidth > container.clientWidth + 1,
            distance: Math.ceil(textWidth + 32),
        };
        setMetrics(current => (
            current.overflowing === nextMetrics.overflowing && current.distance === nextMetrics.distance
                ? current
                : nextMetrics
        ));
    }, []);

    useLayoutEffect(() => {
        if (lazy) return;
        measure();
        const observer = new ResizeObserver(measure);
        if (containerRef.current) observer.observe(containerRef.current);
        if (measurementRef.current) observer.observe(measurementRef.current);
        return () => observer.disconnect();
    }, [lazy, measure, text]);

    const style: MarqueeStyle = {
        '--marquee-distance': `${metrics.distance}px`,
        '--marquee-duration': `${Math.max(5.5, metrics.distance / 28).toFixed(2)}s`,
    };

    return (
        <div
            ref={containerRef}
            className={`hover-marquee min-w-0 max-w-full flex-shrink overflow-hidden ${metrics.overflowing ? 'is-overflowing' : ''} ${className}`}
            title={text}
            style={style}
            onPointerEnter={lazy ? measure : undefined}
            onFocus={lazy ? measure : undefined}
        >
            <span ref={measurementRef} className="pointer-events-none absolute invisible whitespace-nowrap" aria-hidden="true">
                {text}
            </span>
            <span className="hover-marquee-static block whitespace-nowrap">{text}</span>
            {metrics.overflowing && (
                <span className="hover-marquee-viewport pointer-events-none absolute inset-0 opacity-0" aria-hidden="true">
                    <span className="hover-marquee-track inline-flex w-max items-center whitespace-nowrap">
                        <span>{text}</span>
                        <span className="ml-8">{text}</span>
                    </span>
                </span>
            )}
        </div>
    );
}
