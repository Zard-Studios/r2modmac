import { Modal } from '../ui/Modal';
import type { LaunchIssue } from '../../utils/launchIssue';

export { describeLaunchIssue } from '../../utils/launchIssue';
export type { LaunchIssue } from '../../utils/launchIssue';

interface LaunchIssueModalProps {
    issue: LaunchIssue | null;
    onClose: () => void;
}

/**
 * In-app presentation of a launch failure.
 *
 * These problems are almost always resolvable in a few seconds once the user
 * knows what they are, so they are shown inside r2modmac rather than as a
 * native OS alert that reads like the app crashed.
 */
export function LaunchIssueModal({ issue, onClose }: LaunchIssueModalProps) {
    if (!issue) return null;

    return (
        <Modal isOpen onClose={onClose} size="md">
            <div className="flex gap-4">
                <div className="flex h-11 w-11 flex-shrink-0 items-center justify-center rounded-xl border border-amber-500/20 bg-amber-500/10 text-amber-400">
                    <svg className="h-6 w-6" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M12 9v3.75m9-.75a9 9 0 1 1-18 0 9 9 0 0 1 18 0Zm-9 3.75h.008v.008H12v-.008Z" />
                    </svg>
                </div>
                <div className="min-w-0">
                    <h2 className="text-[17px] font-semibold leading-snug text-white">{issue.title}</h2>
                    <p className="mt-2 text-[14px] leading-relaxed text-gray-300">{issue.message}</p>
                    {issue.pointsAtSteam && (
                        <p className="mt-3 text-[13px] leading-relaxed text-gray-500">
                            Your mods are already applied — Steam is holding the launch, not r2modmac.
                            Deal with it in Steam, then press Play again.
                        </p>
                    )}
                </div>
            </div>
            <div className="mt-6 flex justify-end">
                <button
                    type="button"
                    onClick={onClose}
                    className="rounded-lg bg-blue-600 px-5 py-2 text-sm font-medium text-white transition-all hover:bg-blue-500 active:scale-95"
                >
                    Got it
                </button>
            </div>
        </Modal>
    );
}
