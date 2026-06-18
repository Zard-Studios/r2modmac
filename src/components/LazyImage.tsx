/* eslint-disable react-refresh/only-export-components */
import { useEffect, useRef, useState } from 'react';

interface LazyImageProps {
    src: string;
    alt: string;
    className?: string;
    fallback?: React.ReactNode;
    eager?: boolean; // Load immediately without waiting for intersection
}

const loadedImageUrls = new Set<string>();
const failedImageUrls = new Set<string>();
const pendingImageLoads = new Map<string, Promise<void>>();

export function warmImageCache(src: string) {
    if (loadedImageUrls.has(src)) {
        return Promise.resolve();
    }
    if (failedImageUrls.has(src)) {
        return Promise.reject(new Error('Image failed to load'));
    }

    const existingPromise = pendingImageLoads.get(src);
    if (existingPromise) {
        return existingPromise;
    }

    const promise = new Promise<void>((resolve, reject) => {
        const img = new Image();
        img.decoding = 'async';
        img.onload = () => {
            loadedImageUrls.add(src);
            pendingImageLoads.delete(src);
            window.dispatchEvent(new CustomEvent('lazy-image-loaded', { detail: src }));
            resolve();
        };
        img.onerror = () => {
            failedImageUrls.add(src);
            pendingImageLoads.delete(src);
            window.dispatchEvent(new CustomEvent('lazy-image-error', { detail: src }));
            reject(new Error('Image failed to load'));
        };
        img.src = src;
    });

    pendingImageLoads.set(src, promise);
    return promise;
}

/**
 * LazyImage - uses native browser lazy loading with eager option for initial images.
 * This is the most performant approach as it lets the browser optimize.
 */
export function LazyImage({ src, alt, className, fallback, eager = false }: LazyImageProps) {
    const wrapperRef = useRef<HTMLDivElement | null>(null);

    const [prevSrc, setPrevSrc] = useState(src);
    const [prevEager, setPrevEager] = useState(eager);
    const [hasError, setHasError] = useState(failedImageUrls.has(src));
    const [isLoaded, setIsLoaded] = useState(loadedImageUrls.has(src));
    const [shouldLoad, setShouldLoad] = useState(eager || loadedImageUrls.has(src));

    if (src !== prevSrc || eager !== prevEager) {
        setPrevSrc(src);
        setPrevEager(eager);
        setHasError(failedImageUrls.has(src));
        setIsLoaded(loadedImageUrls.has(src));
        setShouldLoad(eager || loadedImageUrls.has(src));
    }

    useEffect(() => {
        const handleLoaded = (event: Event) => {
            const customEvent = event as CustomEvent<string>;
            if (customEvent.detail !== src) return;
            setShouldLoad(true);
            setIsLoaded(true);
            setHasError(false);
        };

        const handleErrored = (event: Event) => {
            const customEvent = event as CustomEvent<string>;
            if (customEvent.detail !== src) return;
            setHasError(true);
        };

        window.addEventListener('lazy-image-loaded', handleLoaded as EventListener);
        window.addEventListener('lazy-image-error', handleErrored as EventListener);
        return () => {
            window.removeEventListener('lazy-image-loaded', handleLoaded as EventListener);
            window.removeEventListener('lazy-image-error', handleErrored as EventListener);
        };
    }, [src]);

    useEffect(() => {
        if (eager || loadedImageUrls.has(src)) {
            return;
        }

        const node = wrapperRef.current;
        if (!node) return;

        const observer = new IntersectionObserver(
            (entries) => {
                if (entries.some((entry) => entry.isIntersecting)) {
                    setShouldLoad(true);
                    observer.disconnect();
                }
            },
            {
                // Preload a bit below the fold so scrolling feels instant, but
                // don't keep hundreds of off-screen images in memory.
                rootMargin: '600px 0px',
                threshold: 0,
            }
        );

        observer.observe(node);
        return () => observer.disconnect();
    }, [src, eager]);

    useEffect(() => {
        if (!shouldLoad || hasError || isLoaded) {
            return;
        }

        let cancelled = false;
        warmImageCache(src)
            .then(() => {
                if (!cancelled) {
                    setIsLoaded(true);
                }
            })
            .catch(() => {
                if (!cancelled) {
                    setHasError(true);
                }
            });

        return () => {
            cancelled = true;
        };
    }, [src, shouldLoad, hasError, isLoaded]);

    if (hasError && fallback) {
        return <>{fallback}</>;
    }

    return (
        <div ref={wrapperRef} className={`${className} relative`}>
            {shouldLoad && (
                <img
                    src={src}
                    alt={alt}
                    loading="eager"
                    decoding="async"
                    fetchPriority={eager ? 'high' : 'auto'}
                    className={`w-full h-full object-cover ${isLoaded ? 'opacity-100' : 'opacity-0'}`}
                    onLoad={() => {
                        loadedImageUrls.add(src);
                        setIsLoaded(true);
                    }}
                    onError={() => {
                        failedImageUrls.add(src);
                        setHasError(true);
                    }}
                />
            )}
            {/* Show placeholder until loaded */}
            {!isLoaded && !hasError && (
                <div className="absolute inset-0 bg-gray-700 animate-pulse" />
            )}
        </div>
    );
}
