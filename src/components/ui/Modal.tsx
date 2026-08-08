import React from 'react';

interface ModalProps {
    isOpen: boolean;
    onClose: () => void;
    title?: React.ReactNode;
    children: React.ReactNode;
    size?: 'sm' | 'md' | 'lg' | 'xl';
}

const sizeStyles = {
    sm: 'max-w-sm',
    md: 'max-w-md',
    lg: 'max-w-lg',
    xl: 'max-w-2xl',
};

export function Modal({ isOpen, onClose, title, children, size = 'md' }: ModalProps) {
    if (!isOpen) return null;

    return (
        <div className="fixed inset-0 z-50 flex items-center justify-center">
            {/* Backdrop */}
            <div
                className="absolute inset-0 bg-black/60 backdrop-blur-sm"
                onClick={onClose}
            />

            {/* Content */}
            <div className={`relative ${sizeStyles[size]} w-full mx-4 bg-gray-800 rounded-xl shadow-2xl border border-gray-700`}>
                {title && (
                    <div className="flex items-center justify-between px-6 py-4 border-b border-gray-700">
                        {typeof title === 'string' ? (
                            <h2 className="text-lg font-bold text-white">{title}</h2>
                        ) : (
                            <div className="flex-1 min-w-0 pr-4">{title}</div>
                        )}
                        <button
                            onClick={onClose}
                            className="p-2 -mr-2 rounded-xl hover:bg-gray-700 text-gray-400 hover:text-white transition-all active:scale-95 focus:outline-none focus:ring-2 focus:ring-gray-700"
                            aria-label="Close"
                        >
                            <svg xmlns="http://www.w3.org/2000/svg" className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M6 18L18 6M6 6l12 12" />
                            </svg>
                        </button>
                    </div>
                )}
                <div className="p-6">
                    {children}
                </div>
            </div>
        </div>
    );
}
