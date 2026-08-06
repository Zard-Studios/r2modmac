import { Modal, Button } from '../ui';
import type { LaunchIssue } from '../../utils/launchIssue';

interface LaunchIssueModalProps {
    issue: LaunchIssue | null;
    onClose: () => void;
}

/**
 * In-app presentation of a launch failure.
 */
export function LaunchIssueModal({ issue, onClose }: LaunchIssueModalProps) {
    if (!issue) return null;

    return (
        <Modal isOpen onClose={onClose} size="md">
            <div className="flex items-center gap-2.5 mb-3">
                <svg className="h-6 w-6 text-amber-400 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
                </svg>
                <h2 className="text-lg font-bold text-white leading-snug whitespace-nowrap">{issue.title}</h2>
            </div>

            <p className="text-sm leading-relaxed text-gray-300">
                {issue.message}
            </p>

            <div className="mt-5 flex justify-end">
                <Button variant="primary" onClick={onClose}>
                    Got it
                </Button>
            </div>
        </Modal>
    );
}



