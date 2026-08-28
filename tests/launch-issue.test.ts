import assert from 'node:assert/strict';
import test from 'node:test';

import { describeLaunchIssue, isLaunchCancelled, LAUNCH_CANCELLED_MESSAGE } from '../src/utils/launchIssue.ts';

// The exact strings the backend produces, so a reworded message on either side
// shows up here rather than as a mislabelled dialog in front of a user.
const CLOUD_CONFLICT =
    'This game has a Steam Cloud conflict. Resolve the conflict in Steam before launching.';
const PENDING_UPDATE =
    'This game has a pending Steam update. Wait for the update to finish before launching.';
const UPDATE_RUNNING =
    'Steam is currently updating this game. Wait for the update to finish before launching.';
const CORRUPT_FILES =
    "Steam reports corrupted game files. Verify the game's files in Steam, then try again.";
const COLD_STEAM =
    'The game did not start in time. Steam may still be signing in; check the Steam window, then press Play again.';

test('a Steam Cloud conflict is titled as such and points at Steam', () => {
    const issue = describeLaunchIssue(CLOUD_CONFLICT);
    assert.equal(issue.title, 'Steam Cloud Conflict');
    assert.equal(issue.pointsAtSteam, true);
    assert.equal(issue.message, CLOUD_CONFLICT);
});

test('a pending update is not mistaken for a generic failure', () => {
    const issue = describeLaunchIssue(PENDING_UPDATE);
    assert.equal(issue.title, 'Pending Steam update');
    assert.equal(issue.pointsAtSteam, true);
});

test('an in-progress update is reported as an update', () => {
    assert.equal(
        describeLaunchIssue(UPDATE_RUNNING).title,
        'Pending Steam update'
    );
});

test('corrupt files win over the word "update" appearing elsewhere', () => {
    // This message contains neither "update" nor "cloud", but the ordering
    // guard matters if the wording ever changes.
    const issue = describeLaunchIssue(CORRUPT_FILES);
    assert.equal(issue.title, 'Game File Issue');
});

test('a cold Steam that ran out of time is explained, not swallowed', () => {
    const issue = describeLaunchIssue(COLD_STEAM);
    assert.equal(issue.title, 'Game Did Not Start');
    assert.equal(issue.pointsAtSteam, true);
});

test('an already-running game does not blame Steam', () => {
    const issue = describeLaunchIssue('Game is already running.');
    assert.equal(issue.title, 'Game Already Running');
    assert.equal(issue.pointsAtSteam, false);
});

test('an unrecognised failure still produces a usable dialog', () => {
    const issue = describeLaunchIssue('Something exploded');
    assert.equal(issue.title, "Game Launch Failed");
    assert.equal(issue.message, 'Something exploded');
});

test('an empty error never renders a blank dialog', () => {
    const issue = describeLaunchIssue('   ');
    assert.equal(issue.message, 'The game could not be started.');
    assert.ok(issue.title.length > 0);
});

test('a Steam game with no reachable client points at Settings, not at Steam', () => {
    // Issue #25: the game lives in a Steam library outside the bottle, so the
    // client that owns it cannot be matched. Steam has nothing to fix here.
    const issue = describeLaunchIssue(
        'This game is installed through Steam, but r2modmac could not find the Windows Steam client that owns it. Set the Windows Steam directory (the folder that contains steam.exe, inside your CrossOver/Wine bottle) in Settings and try again.'
    );
    assert.equal(issue.title, 'Steam Client Not Found');
    assert.equal(issue.pointsAtSteam, false);
});

test('a Steam that is not signed in is named, not lumped into a generic failure', () => {
    // Native macOS launches surface this: Steam is up but logged out, so the
    // run request is accepted and then quietly refused.
    const issue = describeLaunchIssue(
        'Steam could not start the game because it is not signed in. Open Steam, sign in, then press Play again.'
    );
    assert.equal(issue.title, 'Steam Not Signed In');
    assert.equal(issue.pointsAtSteam, true);
});

test('a native launch that timed out is reported as the game not starting', () => {
    const issue = describeLaunchIssue(
        'Steam accepted the launch but the game did not start in time. Check the Steam window, then press Play again.'
    );
    assert.equal(issue.title, 'Game Did Not Start');
    assert.equal(issue.pointsAtSteam, true);
});

test('a game that started without its mods is named as such, not as a launch failure', () => {
    const issue = describeLaunchIssue(
        'The game started, but BepInEx never loaded, so it is running unmodded. The loader could not attach to the game, this is not a problem with your mods. The r2modmac logs in the game folder record what Doorstop reported.'
    );
    assert.equal(issue.title, 'Mods Did Not Load');
    assert.equal(issue.pointsAtSteam, false);
});

test('a launch the user cancelled is recognised, whatever shape the error arrives in', () => {
    assert.equal(isLaunchCancelled(LAUNCH_CANCELLED_MESSAGE), true);
    assert.equal(isLaunchCancelled(new Error(LAUNCH_CANCELLED_MESSAGE)), true);
    assert.equal(isLaunchCancelled({ message: LAUNCH_CANCELLED_MESSAGE }), true);
    assert.equal(isLaunchCancelled('invoke error: Launch cancelled.'), true);
});

test('a real launch failure is not mistaken for a cancellation', () => {
    for (const failure of [
        'Steam accepted the launch but the game did not start. Open Steam to check for a prompt or an error waiting for you there.',
        'This game has a Steam Cloud conflict. Resolve the conflict in Steam before launching.',
        'Game did not start in time.',
        '',
        undefined,
        null,
    ]) {
        assert.equal(isLaunchCancelled(failure), false, String(failure));
    }
});

test('the cancellation message matches the one the backend sends', () => {
    // The two sides agree by string only.
    assert.equal(LAUNCH_CANCELLED_MESSAGE, 'Launch cancelled.');
});

test('a pending steam update lands in the same dialog as one already downloading', () => {
    const downloading = describeLaunchIssue(
        'Steam is currently updating this game. Wait for the update to finish before launching.'
    );
    const waiting = describeLaunchIssue(
        'Steam has an update waiting for this game. Install it in Steam, then launch again.'
    );
    assert.equal(downloading.title, 'Pending Steam update');
    assert.equal(waiting.title, 'Pending Steam update');
    assert.equal(waiting.pointsAtSteam, true);
});
