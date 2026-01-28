import React from 'react';

type ButtonVariant = 'primary' | 'secondary' | 'ghost' | 'danger' | 'outline' | 'purple';
type ButtonSize = 'sm' | 'md' | 'lg';

interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
    variant?: ButtonVariant;
    size?: ButtonSize;
    fullWidth?: boolean;
    children: React.ReactNode;
}

const variantStyles: Record<ButtonVariant, string> = {
    primary: 'bg-blue-600 text-white hover:bg-blue-500',
    secondary: 'bg-gray-700 text-white hover:bg-gray-600',
    ghost: 'bg-transparent text-gray-400 hover:text-white hover:bg-gray-700',
    danger: 'bg-red-600 text-white hover:bg-red-500',
    outline: 'bg-transparent text-gray-400 hover:text-white hover:bg-gray-800 border border-gray-700 hover:border-gray-600',
    purple: 'bg-purple-600 text-white hover:bg-purple-500',
};

const sizeStyles: Record<ButtonSize, string> = {
    sm: 'px-3 py-1.5 text-sm',
    md: 'px-4 py-2 text-sm',
    lg: 'px-6 py-3 text-base',
};

export function Button({
    variant = 'primary',
    size = 'md',
    fullWidth = false,
    className = '',
    disabled,
    children,
    ...props
}: ButtonProps) {
    return (
        <button
            className={`
                ${variantStyles[variant]}
                ${sizeStyles[size]}
                ${fullWidth ? 'w-full' : ''}
                rounded-lg font-semibold transition-colors
                disabled:opacity-50 disabled:cursor-not-allowed
                ${className}
            `.trim().replace(/\s+/g, ' ')}
            disabled={disabled}
            {...props}
        >
            {children}
        </button>
    );
}
