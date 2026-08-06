import assert from 'node:assert/strict';
import test from 'node:test';

import { describeLaunchIssue } from '../src/utils/launchIssue.ts';

// The exact strings the backend produces, so a reworded message on either side
// shows up here rather than as a mislabelled dialog in front of a user.
const CLOUD_CONFLICT =
    'Steam is waiting for an answer to a Steam Cloud conflict for this game. Open Steam, respond to the cloud sync prompt, then try again.';
const PENDING_UPDATE =
    'This game has a pending Steam update. Steam will not start it until the update is installed — open Steam and let it download, then try again.';
const UPDATE_RUNNING =
    'Steam is currently updating this game. Wait for the download to finish, then try again.';
const CORRUPT_FILES =
    "Steam reports corrupted game files. Verify the game's files in Steam, then try again.";
const COLD_STEAM =
    'Steam was not running, so r2modmac started it first — but the game did not start in time. Steam may still be signing in; check the Steam window, then press Play again.';

test('a Steam Cloud conflict is titled as such and points at Steam', () => {
    const issue = describeLaunchIssue(CLOUD_CONFLICT);
    assert.equal(issue.title, 'Steam is waiting on a Steam Cloud conflict');
    assert.equal(issue.pointsAtSteam, true);
    assert.equal(issue.message, CLOUD_CONFLICT);
});

test('a pending update is not mistaken for a generic failure', () => {
    const issue = describeLaunchIssue(PENDING_UPDATE);
    assert.equal(issue.title, 'This game has a pending Steam update');
    assert.equal(issue.pointsAtSteam, true);
});

test('an in-progress update is reported as an update', () => {
    assert.equal(
        describeLaunchIssue(UPDATE_RUNNING).title,
        'This game has a pending Steam update'
    );
});

test('corrupt files win over the word "update" appearing elsewhere', () => {
    // This message contains neither "update" nor "cloud", but the ordering
    // guard matters if the wording ever changes.
    const issue = describeLaunchIssue(CORRUPT_FILES);
    assert.equal(issue.title, 'Steam reports a problem with the game files');
});

test('a cold Steam that ran out of time is explained, not swallowed', () => {
    const issue = describeLaunchIssue(COLD_STEAM);
    assert.equal(issue.title, 'The game did not start');
    assert.equal(issue.pointsAtSteam, true);
});

test('an already-running game does not blame Steam', () => {
    const issue = describeLaunchIssue('Game is already running.');
    assert.equal(issue.title, 'The game is already running');
    assert.equal(issue.pointsAtSteam, false);
});

test('an unrecognised failure still produces a usable dialog', () => {
    const issue = describeLaunchIssue('Something exploded');
    assert.equal(issue.title, "The game couldn't be started");
    assert.equal(issue.message, 'Something exploded');
});

test('an empty error never renders a blank dialog', () => {
    const issue = describeLaunchIssue('   ');
    assert.equal(issue.message, 'The game could not be started.');
    assert.ok(issue.title.length > 0);
});
