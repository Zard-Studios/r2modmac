import { useCallback, useLayoutEffect, useRef, useState } from 'react';

import { DialogLayer } from '../ui';
import { readScrollFades, ScrollFades } from '../ui/ScrollFades';

export interface DependencyRow {
    name: string;
    version?: string | null;
    icon?: string;
    /** Marks a dependency nothing else needs. */
    unused?: boolean;
}

export interface DependencyOption {
    id: string;
    label: string;
    hint: string;
    count?: number;
    /** Which rows this option acts on; the others are dimmed while it is chosen. */
    affects: (row: DependencyRow) => boolean;
}

interface DependencyChoiceDialogProps {
    title: string;
    subtitle: string;
    mod: { name: string; icon?: string };
    listLabel: string;
    rows: DependencyRow[];
    options: DependencyOption[];
    defaultOptionId: string;
    confirmLabel: string;
    confirmTone?: 'primary' | 'danger';
    onConfirm: (optionId: string) => void;
    onCancel: () => void;
}

const ModIcon = ({ icon, name, className }: { icon?: string; name: string; className: string }) => (
    <div className={`flex-shrink-0 overflow-hidden rounded-lg border border-gray-700 bg-gray-900 ${className}`}>
        {icon ? <img src={icon} alt={name} className="h-full w-full object-cover" /> : (
            <div className="flex h-full w-full items-center justify-center text-gray-600">
                <svg xmlns="http://www.w3.org/2000/svg" width="45%" height="45%" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                    <path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z" />
                </svg>
            </div>
        )}
    </div>
);

/**
 * One layout for every "what happens to the dependencies?" question: the mod,
 * the dependencies it touches, a short choice, and Cancel / confirm.
 */
export function DependencyChoiceDialog({
    title,
    subtitle,
    mod,
    listLabel,
    rows,
    options,
    defaultOptionId,
    confirmLabel,
    confirmTone = 'primary',
    onConfirm,
    onCancel,
}: DependencyChoiceDialogProps) {
    const [selectedId, setSelectedId] = useState(defaultOptionId);
    const selected = options.find((option) => option.id === selectedId) ?? options[0];

    const listRef = useRef<HTMLUListElement>(null);
    const [scrollFades, setScrollFades] = useState({ top: false, bottom: false });
    const updateScrollFades = useCallback((element: HTMLElement) => {
        const next = readScrollFades(element);
        setScrollFades((current) => current.top === next.top && current.bottom === next.bottom ? current : next);
    }, []);
    useLayoutEffect(() => {
        const element = listRef.current;
        if (!element) return;
        updateScrollFades(element);
        const resizeObserver = new ResizeObserver(() => updateScrollFades(element));
        resizeObserver.observe(element);
        return () => resizeObserver.disconnect();
    }, [rows.length, updateScrollFades]);

    return (
        <div
            className="fixed inset-0 z-[90] flex items-center justify-center bg-black/70 p-4 backdrop-blur-sm"
            onMouseDown={(event) => { if (event.target === event.currentTarget) onCancel(); }}
        >
            <DialogLayer
                onDismiss={onCancel}
                aria-labelledby="dependency-choice-title"
                className="flex max-h-[85vh] w-full max-w-lg flex-col overflow-hidden rounded-2xl border border-gray-700 bg-gray-800 shadow-2xl"
            >
                <form
                    className="contents"
                    onSubmit={(event) => { event.preventDefault(); onConfirm(selected.id); }}
                >
                    <div className="flex items-center gap-4 border-b border-gray-700 px-6 py-5">
                        <ModIcon icon={mod.icon} name={mod.name} className="h-12 w-12" />
                        <div className="min-w-0">
                            <h2 id="dependency-choice-title" className="truncate text-xl font-bold text-white">{title}</h2>
                            <p className="mt-0.5 text-sm text-gray-400">{subtitle}</p>
                        </div>
                    </div>

                    <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
                        <p className="px-6 pt-4 text-xs font-semibold uppercase tracking-widest text-gray-400">{listLabel}</p>
                        <div className="relative flex min-h-24 max-h-72 flex-col">
                        <ul
                            ref={listRef}
                            onScroll={(event) => updateScrollFades(event.currentTarget)}
                            className="min-h-0 flex-1 overflow-y-auto p-3"
                        >
                            {rows.map((row) => (
                                <li
                                    key={row.name}
                                    className={`flex items-center gap-3 rounded-xl px-3 py-2 transition-opacity ${selected.affects(row) ? '' : 'opacity-45'}`}
                                >
                                    <ModIcon icon={row.icon} name={row.name} className="h-9 w-9" />
                                    <div className="min-w-0 flex-1">
                                        <div className="truncate text-sm font-medium text-gray-100">{row.name}</div>
                                        {row.version ? <div className="text-xs text-gray-400">v{row.version}</div> : null}
                                    </div>
                                    {row.unused ? (
                                        <span className="rounded bg-gray-700 px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-gray-300">Unused</span>
                                    ) : null}
                                </li>
                            ))}
                        </ul>
                        <ScrollFades {...scrollFades} surface="gray-800" />
                        </div>

                        <div role="radiogroup" aria-label="What to do with the dependencies" className="flex-shrink-0 space-y-1 border-t border-gray-700 p-3">
                            {options.map((option) => {
                                const checked = option.id === selected.id;
                                return (
                                    <label
                                        key={option.id}
                                        className={`flex cursor-pointer items-center gap-3 rounded-xl px-3 py-2.5 transition-colors focus-within:ring-1 focus-within:ring-blue-400/70 ${checked ? 'bg-gray-700/60' : 'hover:bg-gray-700/30'}`}
                                    >
                                        <input
                                            type="radio"
                                            name="dependency-choice"
                                            className="sr-only"
                                            checked={checked}
                                            onChange={() => setSelectedId(option.id)}
                                        />
                                        <span
                                            aria-hidden="true"
                                            className={`flex h-4 w-4 flex-shrink-0 items-center justify-center rounded-full border ${checked ? 'border-fg-accent' : 'border-gray-500'}`}
                                        >
                                            {checked ? <span className="h-2 w-2 rounded-full bg-fg-accent" /> : null}
                                        </span>
                                        <span className="min-w-0 flex-1">
                                            <span className="block text-sm font-medium text-gray-100">{option.label}</span>
                                            <span className="block text-xs text-gray-400">{option.hint}</span>
                                        </span>
                                        {option.count !== undefined ? (
                                            <span className="text-xs font-semibold text-gray-400">{option.count}</span>
                                        ) : null}
                                    </label>
                                );
                            })}
                        </div>
                    </div>

                    <div className="grid grid-cols-2 gap-3 border-t border-gray-700 p-4">
                        <button
                            type="button"
                            onClick={onCancel}
                            className="rounded-xl border border-gray-600 bg-gray-700 px-4 py-2.5 text-sm font-medium text-gray-200 transition-colors hover:bg-gray-600"
                        >
                            Cancel
                        </button>
                        <button
                            type="submit"
                            data-dialog-primary
                            className={confirmTone === 'danger'
                                ? 'rounded-xl bg-red-600 px-4 py-2.5 text-sm font-bold text-[#ffffff] transition-colors hover:bg-red-500'
                                : 'rounded-xl border border-blue-500 bg-blue-600 px-4 py-2.5 text-sm font-bold text-on-accent transition-colors hover:bg-accent-hover'}
                        >
                            {confirmLabel}
                        </button>
                    </div>
                </form>
            </DialogLayer>
        </div>
    );
}
