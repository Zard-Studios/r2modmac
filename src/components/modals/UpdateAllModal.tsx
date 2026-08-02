import type { ProfileModUpdate } from '../../hooks/useModActions';

interface UpdateAllModalProps {
    isOpen: boolean;
    updates: ProfileModUpdate[];
    isUpdating: boolean;
    onClose: () => void;
    onConfirm: () => void;
}

export function UpdateAllModal({ isOpen, updates, isUpdating, onClose, onConfirm }: UpdateAllModalProps) {
    if (!isOpen) return null;

    return (
        <div className="fixed inset-0 z-[70] flex items-center justify-center bg-black/70 p-4 backdrop-blur-sm">
            <div
                role="dialog"
                aria-modal="true"
                aria-labelledby="update-all-title"
                className="flex max-h-[78vh] w-full max-w-lg flex-col overflow-hidden rounded-2xl border border-gray-700 bg-gray-800 shadow-2xl"
            >
                <div className="border-b border-gray-700 px-6 py-5">
                    <h2 id="update-all-title" className="text-xl font-bold text-white">Update {updates.length} mod{updates.length === 1 ? '' : 's'}?</h2>
                    <p className="mt-1 text-sm text-gray-400">
                        Versions and dependencies will be staged safely. Existing files are kept until Apply succeeds.
                    </p>
                </div>

                <div className="min-h-0 flex-1 overflow-y-auto p-3">
                    {updates.map(({ mod, pkg, version }) => (
                        <div key={pkg.full_name} className="flex items-center gap-3 rounded-xl px-3 py-2.5 hover:bg-gray-700/50">
                            <div className="h-10 w-10 flex-shrink-0 overflow-hidden rounded-lg border border-gray-700 bg-gray-900">
                                {version.icon ? <img src={version.icon} alt="" className="h-full w-full object-cover" /> : null}
                            </div>
                            <div className="min-w-0 flex-1">
                                <div className="truncate text-sm font-medium text-gray-100">{pkg.name}</div>
                                <div className="flex items-center gap-2 text-xs text-gray-400">
                                    <span>v{mod.versionNumber}</span>
                                    <span aria-hidden="true">→</span>
                                    <span className="text-amber-300">v{version.version_number}</span>
                                    {!mod.enabled ? <span className="rounded bg-gray-700 px-1.5 py-0.5 text-[10px] uppercase">Disabled</span> : null}
                                </div>
                            </div>
                        </div>
                    ))}
                </div>

                <div className="grid grid-cols-2 gap-3 border-t border-gray-700 p-4">
                    <button
                        type="button"
                        onClick={onClose}
                        disabled={isUpdating}
                        className="rounded-xl border border-gray-600 bg-gray-700 px-4 py-2.5 text-sm font-medium text-gray-200 disabled:opacity-50"
                    >
                        Cancel
                    </button>
                    <button
                        type="button"
                        onClick={onConfirm}
                        disabled={isUpdating || updates.length === 0}
                        className="rounded-xl border border-blue-500 bg-blue-600 px-4 py-2.5 text-sm font-bold text-white disabled:cursor-not-allowed disabled:opacity-50"
                    >
                        {isUpdating ? 'Preparing…' : `Update all (${updates.length})`}
                    </button>
                </div>
            </div>
        </div>
    );
}
