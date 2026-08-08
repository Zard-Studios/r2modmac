import { Button } from '../ui';

interface UninstallModalProps {
    isOpen: boolean;
    modName: string;
    modIcon?: string;
    orphanDeps: { name: string; icon?: string }[];
    allDeps: { name: string; icon?: string }[];
    allDepsCount: number;
    onCancel: () => void;
    onModOnly: () => void;
    onWithOrphans: () => void;
    onWithAllDeps: () => void;
}

export function UninstallModal({
    isOpen,
    modName,
    modIcon,
    orphanDeps,
    allDeps,
    allDepsCount,
    onCancel,
    onModOnly,
    onWithOrphans,
    onWithAllDeps
}: UninstallModalProps) {
    if (!isOpen) return null;

    const orphanCount = orphanDeps.length;
    const orphanNames = new Set(orphanDeps.map((dep) => dep.name.toLowerCase()));

    return (
        <div
            className="fixed inset-0 bg-black/80 backdrop-blur-sm flex items-center justify-center z-[60] p-4"
            onClick={onCancel}
        >
            <div
                className="bg-slate-800 rounded-xl max-w-lg w-full border border-slate-700 shadow-2xl overflow-hidden flex flex-col max-h-[90vh]"
                onClick={(e) => e.stopPropagation()}
            >
                <div className="p-6">
                    <div className="flex items-start gap-4 mb-6">
                        <div className="w-16 h-16 rounded-lg bg-slate-800 flex-shrink-0 overflow-hidden border border-slate-700 shadow-sm relative">
                            {modIcon ? (
                                <img src={modIcon} alt={modName} className="w-full h-full object-cover" />
                            ) : (
                                <div className="w-full h-full flex items-center justify-center text-slate-500">
                                    <svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                                        <path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"></path>
                                    </svg>
                                </div>
                            )}
                        </div>
                        <div>
                            <h3 className="text-xl font-bold text-white mb-1">Uninstall {modName}</h3>
                            <p className="text-slate-400 text-sm">
                                How would you like to uninstall this mod?
                            </p>
                        </div>
                    </div>

                    {allDeps.length > 0 && (
                        <div className="mb-6">
                            <p className="text-xs font-semibold text-slate-400 uppercase tracking-wider mb-2 pl-1">
                                Installed dependencies
                            </p>
                            <div className="bg-slate-900/50 rounded-lg p-3 border border-slate-700/50 overflow-x-auto">
                                <div className="flex gap-3">
                                    {allDeps.map((dep, i) => (
                                        <div key={i} className="flex flex-col items-center w-20 flex-shrink-0 group relative">
                                            <div className="w-11 h-11 rounded bg-slate-800 mb-1.5 overflow-hidden border border-slate-700 group-hover:border-slate-500 transition-colors shadow-sm">
                                                {dep.icon ? (
                                                    <img src={dep.icon} alt={dep.name} className="w-full h-full object-cover" />
                                                ) : (
                                                    <div className="w-full h-full flex items-center justify-center text-slate-600">
                                                        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                                                            <path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"></path>
                                                        </svg>
                                                    </div>
                                                )}
                                            </div>
                                            <span className="text-[10px] text-slate-400 text-center truncate w-full px-0.5 group-hover:text-slate-300 transition-colors">
                                                {dep.name}
                                            </span>
                                            {orphanNames.has(dep.name.toLowerCase()) && (
                                                <span className="mt-1 px-1.5 py-0.5 rounded text-[9px] font-semibold uppercase tracking-wide bg-indigo-500/20 text-indigo-200 border border-indigo-400/20">
                                                    Unused
                                                </span>
                                            )}
                                        </div>
                                    ))}
                                </div>
                            </div>
                        </div>
                    )}

                    <div className="flex flex-col gap-3">
                        {/* Mod Only Button */}
                        <button
                            onClick={onModOnly}
                            className="w-full p-4 bg-slate-700/50 hover:bg-slate-700 text-white rounded-lg font-medium transition-all text-left group border border-transparent hover:border-slate-600"
                        >
                            <div className="flex items-center justify-between mb-1">
                                <span className="text-base font-semibold">Remove mod only</span>
                                <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" className="text-slate-500 group-hover:text-white transition-colors">
                                    <path d="M9 18l6-6-6-6" />
                                </svg>
                            </div>
                            <p className="text-xs text-slate-400 group-hover:text-slate-300">Keep all dependencies</p>
                        </button>

                        {/* Orphan Deps Button */}
                        {orphanCount > 0 && (
                            <button
                                onClick={onWithOrphans}
                                className="w-full p-4 bg-indigo-600 hover:bg-indigo-500 text-white rounded-lg font-medium transition-all text-left group shadow-lg shadow-indigo-900/20"
                            >
                                <div className="flex items-center justify-between mb-1">
                                    <span className="text-base font-semibold">Remove + unused dependencies</span>
                                    <span className="bg-indigo-500/80 px-2 py-0.5 rounded text-xs font-bold text-white group-hover:bg-indigo-400 transition-colors">{orphanCount}</span>
                                </div>
                                <p className="text-xs text-indigo-200 group-hover:text-indigo-100">Recommended: Cleans up unused files</p>
                            </button>
                        )}

                        {/* All Deps Button */}
                        {allDepsCount > orphanCount && (
                            <button
                                onClick={onWithAllDeps}
                                className="w-full p-4 bg-red-500/10 hover:bg-red-500/20 text-fg-danger rounded-lg font-medium transition-all text-left border border-red-500/20 hover:border-red-500/30 group"
                            >
                                <div className="flex items-center justify-between mb-1">
                                    <span className="text-base font-semibold text-fg-danger group-hover:text-fg-danger">Remove + ALL dependencies</span>
                                    <span className="bg-red-900/40 px-2 py-0.5 rounded text-xs font-bold text-fg-danger group-hover:bg-red-900/60 transition-colors border border-red-500/20">{allDepsCount}</span>
                                </div>
                                <div className="flex items-center gap-1.5 mt-0.5">
                                    <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-amber-500">
                                        <circle cx="12" cy="12" r="10"></circle>
                                        <line x1="12" y1="8" x2="12" y2="12"></line>
                                        <line x1="12" y1="16" x2="12.01" y2="16"></line>
                                    </svg>
                                    <p className="text-xs text-fg-danger/80 group-hover:text-fg-danger">May break other mods</p>
                                </div>
                            </button>
                        )}
                    </div>
                </div>

                <div className="bg-slate-800 p-4 flex justify-end">
                    <Button variant="ghost" onClick={onCancel}>
                        Cancel
                    </Button>
                </div>
            </div>
        </div>
    );
}
