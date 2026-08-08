import { useCommandSource } from '../store/useCommandStore';
import type { CommandItem } from '../utils/commandPalette';

interface CommandSourceProps {
    /** Stable per view; a second source with the same id replaces the first. */
    id: string;
    items: () => CommandItem[];
}

/**
 * Contributes commands from a place that cannot call a hook.
 *
 * `App` decides what to render in a plain `if`/`else`, so the branch holding
 * the launch and apply handlers is not a component and cannot register them
 * itself. Rendering this there does the same job.
 */
export function CommandSource({ id, items }: CommandSourceProps) {
    useCommandSource(id, items);
    return null;
}
