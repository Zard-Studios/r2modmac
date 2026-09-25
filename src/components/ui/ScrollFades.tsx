export interface ScrollFadeState {
    top: boolean;
    bottom: boolean;
}

export function readScrollFades(element: HTMLElement): ScrollFadeState {
    return {
        top: element.scrollTop > 1,
        bottom: element.scrollTop + element.clientHeight < element.scrollHeight - 1,
    };
}

export function ScrollFades({ top, bottom, surface = 'gray-900' }: ScrollFadeState & { surface?: 'gray-800' | 'gray-900' }) {
    const color = surface === 'gray-800' ? 'from-gray-800 via-gray-800/80' : 'from-gray-900 via-gray-900/80';
    return (
        <>
            <div aria-hidden="true" className={`pointer-events-none absolute inset-x-0 top-0 z-10 h-7 bg-gradient-to-b ${color} to-transparent transition-opacity duration-200 ease-out ${top ? 'opacity-100' : 'opacity-0'}`} />
            <div aria-hidden="true" className={`pointer-events-none absolute inset-x-0 bottom-0 z-10 h-7 bg-gradient-to-t ${color} to-transparent transition-opacity duration-200 ease-out ${bottom ? 'opacity-100' : 'opacity-0'}`} />
        </>
    );
}
