import type { ReactNode } from 'react';

interface LayoutProps {
    sidebar: ReactNode;
    main: ReactNode;
}

export function Layout({ sidebar, main }: LayoutProps) {
    return (
        <div className="flex h-full bg-gray-900 text-white">
            {/* Sidebar */}
            <div className="flex-shrink-0">
                {sidebar}
            </div>

            {/* Main Content */}
            <div className="flex-1 flex flex-col overflow-hidden">
                {main}
            </div>
        </div>
    );
}
