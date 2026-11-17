import { Suspense, lazy, useState, type ComponentType } from 'react';

/**
 * Wraps a modal so its code is split out of the main bundle. Nothing is mounted
 * until the modal is opened for the first time; after that it stays mounted
 * exactly like the statically imported version did, so open/close animations
 * and internal state behave the same. `preload` fetches the chunk ahead of the
 * first open so the first click is not delayed.
 */
export function createLazyModal<M, P extends { isOpen: boolean }>(
    load: () => Promise<M>,
    pick: (module: M) => ComponentType<P>,
) {
    const Inner = lazy(() => load().then(module => ({ default: pick(module) })));
    function LazyModal(props: P) {
        const [everOpened, setEverOpened] = useState(props.isOpen);
        if (props.isOpen && !everOpened) setEverOpened(true);
        if (!everOpened) return null;
        return (
            <Suspense fallback={null}>
                <Inner {...(props as P & React.JSX.IntrinsicAttributes)} />
            </Suspense>
        );
    }
    return Object.assign(LazyModal, { preload: () => load().then(() => undefined) });
}
