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
            {/* Sidebar & Toggle - Only render if sidebar exists */}
            {sidebar && (
                <>
                    <div
                        className={`flex-shrink-0 transition-all duration-300 ease-[cubic-bezier(0.25,0.1,0.25,1)] overflow-hidden ${isSidebarOpen ? 'w-80 opacity-100' : 'w-0 opacity-0'
                            }`}
                    >
                        <div className="w-80 h-full relative">
                            {sidebar}
                        </div>
                    </div>

                    {/* Collapse toggle with a small hover zone (not full-height) to avoid wheel interception */}
                    <div className="relative z-50 h-full w-0 pointer-events-none">
                        <div className="pointer-events-auto absolute top-1/2 left-0 -translate-y-1/2 w-10 h-20 flex items-center justify-start group">
                            <button
                                onClick={onToggleSidebar}
                                className="w-6 h-12 bg-gray-800 border-y border-r border-gray-700 rounded-r-lg flex items-center justify-center text-gray-400 hover:text-white transition-all duration-300 shadow-lg opacity-0 -translate-x-2 group-hover:opacity-100 group-hover:translate-x-0 focus:opacity-100 focus:translate-x-0"
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
                        </div>
                    </div>
                </>
            )}

            {/* Main Content */}
            <div className="flex-1 flex flex-col overflow-hidden">
                {main}
            </div>
        </div>
    );
}
