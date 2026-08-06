/**
 * Concurrency helpers for mod install/sync pipelines.
 *
 * These pipelines used to run in fixed batches: take N tasks, `Promise.all`
 * them, then take the next N. Because every batch ends on a barrier, one slow
 * task stalls the whole batch — and mod sizes differ by three orders of
 * magnitude (a 19 KB config patch next to a 182 MB texture pack), so the
 * barrier is hit constantly. A sliding pool keeps all N slots busy instead:
 * as soon as any task finishes, the next one starts.
 */

/**
 * Run `tasks` with at most `maxConcurrency` in flight at a time.
 *
 * All tasks are run to completion even if some reject; the first rejection is
 * re-thrown afterwards. That matches the previous `allSettled`-based behaviour,
 * where a failing mod did not cancel the downloads already in progress.
 */
export async function runWithConcurrency(
    tasks: Array<() => void | Promise<unknown>>,
    maxConcurrency: number
): Promise<void> {
    if (tasks.length === 0) return;

    const limit = Math.max(1, Math.min(Math.floor(maxConcurrency), tasks.length));
    let nextIndex = 0;
    let firstError: unknown;
    let hasError = false;

    const worker = async () => {
        while (true) {
            const index = nextIndex++;
            if (index >= tasks.length) return;
            try {
                await tasks[index]();
            } catch (error) {
                if (!hasError) {
                    hasError = true;
                    firstError = error;
                }
            }
        }
    };

    await Promise.all(Array.from({ length: limit }, () => worker()));

    if (hasError) throw firstError;
}

/**
 * Coalesce bursty state updates onto animation frames.
 *
 * Download progress arrives per-mod every ~120 ms, so ten parallel downloads
 * produce ~80 events/second, each of which previously triggered its own React
 * state update and re-render. Collapsing them to at most one update per frame
 * keeps the UI responsive without losing the latest value.
 */
export function createFrameScheduler(): {
    schedule: (callback: () => void) => void;
    flush: () => void;
    cancel: () => void;
} {
    let handle: number | null = null;
    let pending: (() => void) | null = null;

    const run = () => {
        handle = null;
        const callback = pending;
        pending = null;
        callback?.();
    };

    return {
        schedule(callback: () => void) {
            // Only the most recent callback matters: progress is absolute, not
            // incremental, so superseded frames can be dropped outright.
            pending = callback;
            if (handle !== null) return;
            handle = typeof requestAnimationFrame === 'function'
                ? requestAnimationFrame(run)
                : (setTimeout(run, 16) as unknown as number);
        },
        flush() {
            if (handle === null) return;
            if (typeof cancelAnimationFrame === 'function') cancelAnimationFrame(handle);
            else clearTimeout(handle as unknown as ReturnType<typeof setTimeout>);
            run();
        },
        cancel() {
            if (handle === null) return;
            if (typeof cancelAnimationFrame === 'function') cancelAnimationFrame(handle);
            else clearTimeout(handle as unknown as ReturnType<typeof setTimeout>);
            handle = null;
            pending = null;
        },
    };
}
