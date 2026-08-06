/**
 * Classification of game-launch failures for presentation in r2modmac's own UI.
 *
 * The backend returns user-facing sentences (see
 * `src-tauri/src/commands/game_commands/steam_state.rs`) rather than error
 * codes, because the useful part is the instruction, not the category. This
 * only picks a headline and decides whether to add the "this is Steam, not us"
 * reassurance — which matters, since a launch that does nothing right after
 * applying mods reads like r2modmac broke the game.
 */

export interface LaunchIssue {
    /** Short headline describing the category of problem. */
    title: string;
    /** The specific, actionable explanation from the backend. */
    message: string;
    /** True when the problem is something the user resolves inside Steam. */
    pointsAtSteam: boolean;
}

export function describeLaunchIssue(raw: string): LaunchIssue {
    const message = raw.trim() || 'The game could not be started.';
    const lower = message.toLowerCase();

    // Ordered most specific first: a cloud-conflict message also contains the
    // word "prompt", and a files message can mention "update".
    if (lower.includes('cloud')) {
        return {
            title: 'Steam is waiting on a Steam Cloud conflict',
            message,
            pointsAtSteam: true,
        };
    }
    if (lower.includes('corrupt') || lower.includes('missing game files') || lower.includes('verify')) {
        return { title: 'Steam reports a problem with the game files', message, pointsAtSteam: true };
    }
    // 'updat' rather than 'update': the in-progress message says "updating",
    // which does not contain "update".
    if (lower.includes('updat')) {
        return { title: 'This game has a pending Steam update', message, pointsAtSteam: true };
    }
    if (lower.includes('waiting for an answer') || lower.includes('prompt')) {
        return { title: 'Steam is waiting for an answer', message, pointsAtSteam: true };
    }
    if (lower.includes('already running')) {
        return { title: 'The game is already running', message, pointsAtSteam: false };
    }
    if (lower.includes('did not start') || lower.includes('was not running')) {
        return { title: 'The game did not start', message, pointsAtSteam: true };
    }
    return { title: "The game couldn't be started", message, pointsAtSteam: false };
}
