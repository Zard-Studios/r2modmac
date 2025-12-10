import type { ReactNode } from 'react';

interface LayoutProps {
    sidebar: ReactNode;
    main: ReactNode;
    isSidebarOpen: boolean;
    onToggleSidebar: () => void;
}

export function Layout({ sidebar, main, isSidebarOpen, onToggleSidebar }: LayoutProps) {
    return (
        <div className="flex h-full bg-gray-900 text-white relative">
            {/* Sidebar */}
            <div
                className={`flex-shrink-0 transition-all duration-300 ease-[cubic-bezier(0.25,0.1,0.25,1)] overflow-hidden ${isSidebarOpen ? 'w-80 opacity-100' : 'w-0 opacity-0'
                    }`}
            >
                <div className="w-80 h-full relative">
                    {sidebar}
                </div>
            </div>

            {/* Toggle Handle - Positioned absolutely relative to the flex container or wrapper */}
            {/* We place it in a zero-width container between sections so it moves with the expansion */}
            <div className="relative z-50 h-full w-0 flex flex-col justify-center items-center">
                {/* Hover Zone - Center on border (-left-4 = -16px, w-8 = 32px) */}
                <div className="absolute w-8 h-full -left-4 flex items-center justify-center group cursor-pointer">
                    {/* Button - Positioned to the RIGHT of the border (left-1/2 of 32px zone = 16px start point = 0px relative to container) */}
                    <button
                        onClick={onToggleSidebar}
                        className="absolute left-1/2 w-6 h-12 bg-gray-800 border-y border-r border-gray-700 rounded-r-lg flex items-center justify-center text-gray-400 hover:text-white transition-all duration-300 shadow-lg opacity-0 group-hover:opacity-100 translate-x-[-10px] group-hover:translate-x-0"
                        title={isSidebarOpen ? "Close Sidebar" : "Open Sidebar"}
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            {isSidebarOpen ? (
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 19l-7-7 7-7" />
                            ) : (
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
                            )}
                        </svg>
                    </button>
                    {/* Visual hint for the zone when button is hidden (optional, but requested to be invisible 'sparire') */}
                </div>
            </div>

            {/* Main Content */}
            <div className="flex-1 flex flex-col overflow-hidden">
                {main}
            </div>
        </div>
    );
}
