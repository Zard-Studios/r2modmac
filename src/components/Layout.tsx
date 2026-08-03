import {
    useEffect,
    useRef,
    useState,
    type KeyboardEvent as ReactKeyboardEvent,
    type PointerEvent as ReactPointerEvent,
    type ReactNode,
} from 'react';

const SIDEBAR_WIDTH_STORAGE_KEY = 'r2modmac:profile-sidebar-width:v1';
const DEFAULT_SIDEBAR_WIDTH = 320;
const MIN_SIDEBAR_WIDTH = 300;
const MAX_SIDEBAR_WIDTH = 640;
const MIN_MAIN_WIDTH = 840;
const KEYBOARD_RESIZE_STEP = 16;

function clampSidebarWidth(width: number, availableWidth: number) {
    const responsiveMaximum = Math.max(
        MIN_SIDEBAR_WIDTH,
        Math.min(MAX_SIDEBAR_WIDTH, availableWidth - MIN_MAIN_WIDTH),
    );

    return Math.min(Math.max(width, MIN_SIDEBAR_WIDTH), responsiveMaximum);
}

function readSavedSidebarWidth() {
    if (typeof window === 'undefined') return DEFAULT_SIDEBAR_WIDTH;

    const savedValue = window.localStorage.getItem(SIDEBAR_WIDTH_STORAGE_KEY);
    if (savedValue === null) return DEFAULT_SIDEBAR_WIDTH;

    const savedWidth = Number(savedValue);
    if (!Number.isFinite(savedWidth)) return DEFAULT_SIDEBAR_WIDTH;

    return clampSidebarWidth(savedWidth, window.innerWidth);
}

interface LayoutProps {
    sidebar: ReactNode;
    main: ReactNode;
    isSidebarOpen: boolean;
    onToggleSidebar: () => void;
}

export function Layout({ sidebar, main, isSidebarOpen, onToggleSidebar }: LayoutProps) {
    const [sidebarWidth, setSidebarWidth] = useState(readSavedSidebarWidth);
    const [availableWidth, setAvailableWidth] = useState(() => (
        typeof window === 'undefined' ? DEFAULT_SIDEBAR_WIDTH + MIN_MAIN_WIDTH : window.innerWidth
    ));
    const [isResizing, setIsResizing] = useState(false);
    const layoutRef = useRef<HTMLDivElement>(null);
    const sidebarShellRef = useRef<HTMLDivElement>(null);
    const sidebarContentRef = useRef<HTMLDivElement>(null);
    const currentWidthRef = useRef(sidebarWidth);
    const activePointerIdRef = useRef<number | null>(null);
    const pendingPointerXRef = useRef<number | null>(null);
    const resizeFrameRef = useRef<number | null>(null);

    const getAvailableWidth = () => (
        layoutRef.current?.getBoundingClientRect().width ?? window.innerWidth
    );

    const renderWidth = (width: number) => {
        currentWidthRef.current = width;

        if (sidebarContentRef.current) {
            sidebarContentRef.current.style.width = `${width}px`;
        }
        if (sidebarShellRef.current && isSidebarOpen) {
            sidebarShellRef.current.style.width = `${width}px`;
        }
    };

    const commitWidth = (width: number) => {
        const nextWidth = clampSidebarWidth(width, getAvailableWidth());
        renderWidth(nextWidth);
        setSidebarWidth(nextWidth);
        window.localStorage.setItem(SIDEBAR_WIDTH_STORAGE_KEY, String(nextWidth));
    };

    useEffect(() => {
        const handleWindowResize = () => {
            const measuredWidth = layoutRef.current?.getBoundingClientRect().width ?? window.innerWidth;
            const fittedWidth = clampSidebarWidth(sidebarWidth, measuredWidth);

            setAvailableWidth(measuredWidth);

            if (sidebarContentRef.current) {
                sidebarContentRef.current.style.width = `${fittedWidth}px`;
            }
            if (sidebarShellRef.current && isSidebarOpen) {
                sidebarShellRef.current.style.width = `${fittedWidth}px`;
            }
            currentWidthRef.current = fittedWidth;
        };

        window.addEventListener('resize', handleWindowResize);
        return () => window.removeEventListener('resize', handleWindowResize);
    }, [isSidebarOpen, sidebarWidth]);

    useEffect(() => () => {
        if (resizeFrameRef.current !== null) {
            window.cancelAnimationFrame(resizeFrameRef.current);
        }
    }, []);

    const renderPointerWidth = (clientX: number) => {
        const layoutBounds = layoutRef.current?.getBoundingClientRect();
        const layoutLeft = layoutBounds?.left ?? 0;
        const layoutWidth = layoutBounds?.width ?? window.innerWidth;
        renderWidth(clampSidebarWidth(clientX - layoutLeft, layoutWidth));
    };

    const flushPendingPointerWidth = () => {
        resizeFrameRef.current = null;
        const pendingPointerX = pendingPointerXRef.current;
        pendingPointerXRef.current = null;
        if (pendingPointerX !== null) renderPointerWidth(pendingPointerX);
    };

    const handleResizePointerDown = (event: ReactPointerEvent<HTMLDivElement>) => {
        if (event.button !== 0) return;

        activePointerIdRef.current = event.pointerId;
        event.currentTarget.setPointerCapture(event.pointerId);
        setIsResizing(true);
    };

    const handleResizePointerMove = (event: ReactPointerEvent<HTMLDivElement>) => {
        if (activePointerIdRef.current !== event.pointerId) return;

        pendingPointerXRef.current = event.clientX;
        if (resizeFrameRef.current === null) {
            resizeFrameRef.current = window.requestAnimationFrame(flushPendingPointerWidth);
        }
    };

    const finishPointerResize = (event: ReactPointerEvent<HTMLDivElement>) => {
        if (activePointerIdRef.current !== event.pointerId) return;

        if (event.currentTarget.hasPointerCapture(event.pointerId)) {
            event.currentTarget.releasePointerCapture(event.pointerId);
        }
        event.currentTarget.blur();
        if (resizeFrameRef.current !== null) {
            window.cancelAnimationFrame(resizeFrameRef.current);
            flushPendingPointerWidth();
        }
        activePointerIdRef.current = null;
        setIsResizing(false);
        commitWidth(currentWidthRef.current);
    };

    const handleResizeKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>) => {
        let nextWidth = currentWidthRef.current;

        if (event.key === 'ArrowLeft') nextWidth -= KEYBOARD_RESIZE_STEP;
        else if (event.key === 'ArrowRight') nextWidth += KEYBOARD_RESIZE_STEP;
        else if (event.key === 'Home') nextWidth = MIN_SIDEBAR_WIDTH;
        else if (event.key === 'End') nextWidth = MAX_SIDEBAR_WIDTH;
        else return;

        event.preventDefault();
        commitWidth(nextWidth);
    };

    const displayedSidebarWidth = clampSidebarWidth(sidebarWidth, availableWidth);
    const availableSidebarMaximum = Math.floor(Math.max(
        MIN_SIDEBAR_WIDTH,
        Math.min(MAX_SIDEBAR_WIDTH, availableWidth - MIN_MAIN_WIDTH),
    ));

    return (
        <div
            ref={layoutRef}
            className={`flex h-full bg-gray-900 text-white relative ${isResizing ? 'cursor-col-resize select-none' : ''}`}
        >
            {/* Sidebar, resize handle & toggle - Only render if sidebar exists */}
            {sidebar ? (
                <>
                    <div
                        ref={sidebarShellRef}
                        className={`relative z-30 flex-shrink-0 overflow-hidden ${isResizing ? '' : 'transition-[width] duration-[360ms] ease-[cubic-bezier(0.32,0.72,0,1)]'}`}
                        style={{ width: isSidebarOpen ? displayedSidebarWidth : 0 }}
                    >
                        <div
                            ref={sidebarContentRef}
                            className={`sidebar-content-motion h-full min-w-0 relative ${isSidebarOpen ? 'sidebar-content-open' : 'sidebar-content-closed'}`}
                            style={{ width: displayedSidebarWidth }}
                        >
                            {sidebar}
                        </div>
                    </div>

                    <div className="relative z-20 h-full w-0 pointer-events-none">
                        {isSidebarOpen ? (
                            <div
                                role="separator"
                                aria-label="Resize sidebar"
                                aria-orientation="vertical"
                                aria-valuemin={MIN_SIDEBAR_WIDTH}
                                aria-valuemax={availableSidebarMaximum}
                                aria-valuenow={Math.round(displayedSidebarWidth)}
                                tabIndex={0}
                                onPointerDown={handleResizePointerDown}
                                onPointerMove={handleResizePointerMove}
                                onPointerUp={finishPointerResize}
                                onPointerCancel={finishPointerResize}
                                onKeyDown={handleResizeKeyDown}
                                className="pointer-events-auto absolute inset-y-0 -left-1 w-2 cursor-col-resize touch-none group/resize focus:outline-none"
                            >
                                <div className={`absolute inset-y-0 left-1/2 w-px -translate-x-1/2 transition-colors ${isResizing ? 'bg-blue-400' : 'bg-gray-700 group-hover/resize:bg-blue-400 group-focus/resize:bg-blue-400'}`} />
                            </div>
                        ) : null}

                        {/* Small hover zone keeps the toggle from intercepting the sidebar wheel. */}
                        <div className="pointer-events-auto absolute top-1/2 left-0 -translate-y-1/2 w-10 h-20 flex items-center justify-start group">
                            <button
                                type="button"
                                onClick={onToggleSidebar}
                                className="w-6 h-12 bg-gray-800 border-y border-r border-gray-700 rounded-r-lg flex items-center justify-center text-gray-400 hover:text-white transition-all duration-300 shadow-lg opacity-0 -translate-x-2 group-hover:opacity-100 group-hover:translate-x-0 focus:opacity-100 focus:translate-x-0"
                                aria-label={isSidebarOpen ? 'Close sidebar' : 'Open sidebar'}
                            >
                                <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" aria-hidden="true">
                                    {isSidebarOpen ? (
                                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 19l-7-7 7-7" />
                                    ) : (
                                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
                                    )}
                                </svg>
                            </button>
                        </div>
                    </div>
                </>
            ) : null}

            {/* Main Content */}
            <div className="min-w-0 flex-1 flex flex-col overflow-hidden">
                {main}
            </div>
        </div>
    );
}
