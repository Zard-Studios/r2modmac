import React from 'react';

interface InputProps extends React.InputHTMLAttributes<HTMLInputElement> {
    label?: string;
    error?: string;
}

export function Input({ label, error, className = '', ...props }: InputProps) {
    return (
        <div className="w-full">
            {label && (
                <label className="block text-sm font-medium text-gray-300 mb-1.5">
                    {label}
                </label>
            )}
            <input
                className={`
                    w-full bg-gray-900 border border-gray-700 rounded-lg
                    px-4 py-2 text-white placeholder-gray-500
                    focus:outline-none focus:border-blue-500 focus:ring-1 focus:ring-blue-500
                    disabled:opacity-50 disabled:cursor-not-allowed
                    transition-colors
                    ${error ? 'border-red-500' : ''}
                    ${className}
                `.trim().replace(/\s+/g, ' ')}
                {...props}
            />
            {error && (
                <p className="mt-1 text-sm text-red-400">{error}</p>
            )}
        </div>
    );
}
