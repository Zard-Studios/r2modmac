import { useState } from 'react';

interface LazyImageProps {
    src: string;
    alt: string;
    className?: string;
    fallback?: React.ReactNode;
    eager?: boolean; // Load immediately without waiting for intersection
}

/**
 * LazyImage - uses native browser lazy loading with eager option for initial images.
 * This is the most performant approach as it lets the browser optimize.
 */
export function LazyImage({ src, alt, className, fallback, eager = false }: LazyImageProps) {
    const [hasError, setHasError] = useState(false);
    const [isLoaded, setIsLoaded] = useState(false);

    if (hasError && fallback) {
        return <>{fallback}</>;
    }

    return (
        <div className={`${className} relative`}>
            <img
                src={src}
                alt={alt}
                loading={eager ? 'eager' : 'lazy'}
                decoding="async"
                className={`w-full h-full object-cover transition-opacity duration-150 ${isLoaded ? 'opacity-100' : 'opacity-0'}`}
                onLoad={() => setIsLoaded(true)}
                onError={() => setHasError(true)}
            />
            {/* Show placeholder until loaded */}
            {!isLoaded && !hasError && (
                <div className="absolute inset-0 bg-gray-700 animate-pulse" />
            )}
        </div>
    );
}
