import assert from 'node:assert/strict';
import test from 'node:test';

import {
    GROUP_ORDER,
    MAX_PER_GROUP,
    buildSections,
    flattenSections,
    moveHighlight,
    parseQuery,
    scoreItem,
    type CommandItem,
} from '../src/utils/commandPalette.ts';

/**
 * What the palette offers, and in what order.
 *
 * The behaviour that matters here is ranking and reachability: an item that
 * exists but never surfaces is the same as one that does not exist.
 */

function item(overrides: Partial<CommandItem> & { id: string; title: string }): CommandItem {
    return { group: 'Games', run: () => {}, ...overrides } as CommandItem;
}

const catalogue: CommandItem[] = [
    item({ id: 'a1', title: 'Launch game (modded)', group: 'Actions', slash: 'launch' }),
    item({ id: 'a2', title: 'Launch game (unmodded)', group: 'Actions', slash: 'vanilla' }),
    item({ id: 'a3', title: 'Apply mods to game', group: 'Actions', slash: 'apply' }),
    item({ id: 'p1', title: 'Modded', group: 'Profiles' }),
    item({ id: 'p2', title: 'Vanilla run', group: 'Profiles' }),
    item({ id: 'g1', title: 'Lethal Company', group: 'Games', subtitle: 'lethal-company' }),
    item({ id: 'g2', title: 'Outer Wilds', group: 'Games', subtitle: 'outerwilds' }),
    item({ id: 's1', title: 'Theme', group: 'Settings', slash: 'theme' }),
];

// ── Slash commands ───────────────────────────────────────────────────────────

test('a leading slash switches to command mode', () => {
    assert.deepEqual(parseQuery('/lau'), { slashMode: true, term: 'lau' });
    assert.deepEqual(parseQuery('lau'), { slashMode: false, term: 'lau' });
});

test('a bare slash lists every command', () => {
    // How someone who typed "/" to see what happens finds out what there is.
    const sections = buildSections(catalogue, '/');
    const titles = flattenSections(sections).map((i) => i.title);
    assert.deepEqual(titles.sort(), [
        'Apply mods to game',
        'Launch game (modded)',
        'Launch game (unmodded)',
        'Theme',
    ]);
});

test('slash mode matches the command word, not the item title', () => {
    // "/lau" means the launch command. A profile called "Launcher" is not one.
    const withProfile = [...catalogue, item({ id: 'p3', title: 'Launcher', group: 'Profiles' })];
    const titles = flattenSections(buildSections(withProfile, '/lau')).map((i) => i.title);
    assert.deepEqual(titles, ['Launch game (modded)']);
});

test('items with no command word are unreachable by slash', () => {
    assert.equal(scoreItem(item({ id: 'p1', title: 'Modded', group: 'Profiles' }), parseQuery('/mod')), null);
});

test('leading whitespace does not defeat slash mode', () => {
    assert.equal(parseQuery('  /apply').slashMode, true);
});

// ── Ordinary search ──────────────────────────────────────────────────────────

test('results are grouped in a fixed order', () => {
    const groups = buildSections(catalogue, '').map((s) => s.group);
    assert.deepEqual(groups, [...GROUP_ORDER]);
});

test('a group with nothing in it is dropped rather than left as a heading', () => {
    const groups = buildSections(catalogue, 'lethal').map((s) => s.group);
    assert.deepEqual(groups, ['Games']);
});

test('a match on the name outranks a match on the identifier', () => {
    // Both "Outer Wilds" and its identifier contain the letters; the readable
    // name is what the user was aiming at.
    const games = [
        item({ id: 'g1', title: 'Outer Wilds', subtitle: 'outerwilds' }),
        item({ id: 'g2', title: 'Risk of Rain 2', subtitle: 'outerwilds-adjacent' }),
    ];
    const titles = flattenSections(buildSections(games, 'outer')).map((i) => i.title);
    assert.equal(titles[0], 'Outer Wilds');
});

test('an identifier-only match still surfaces', () => {
    const games = [item({ id: 'g1', title: 'Lethal Company', subtitle: 'lethal-company' })];
    assert.equal(flattenSections(buildSections(games, 'lethal-comp')).length, 1);
});

test('each group is capped so one long list cannot bury the others', () => {
    const manyGames = Array.from({ length: 40 }, (_, n) =>
        item({ id: `g${n}`, title: `Game ${n}`, group: 'Games' })
    );
    const sections = buildSections([...manyGames, item({ id: 'p1', title: 'Game profile', group: 'Profiles' })], 'game');

    const games = sections.find((s) => s.group === 'Games')!;
    assert.equal(games.items.length, MAX_PER_GROUP);
    assert.ok(sections.some((s) => s.group === 'Profiles'), 'the profile is still reachable');
});

test('equally scored items keep a stable order', () => {
    // Otherwise two identical scores could swap between renders and move a row
    // out from under the cursor.
    const tied = [
        item({ id: 'b', title: 'Beta' }),
        item({ id: 'a', title: 'Alpha' }),
    ];
    const first = flattenSections(buildSections(tied, '')).map((i) => i.title);
    const second = flattenSections(buildSections([...tied].reverse(), '')).map((i) => i.title);
    assert.deepEqual(first, ['Alpha', 'Beta']);
    assert.deepEqual(second, first, 'input order does not change the result');
});

test('a query matching nothing yields no sections', () => {
    assert.deepEqual(buildSections(catalogue, 'zzzzqqq'), []);
});

// ── Moving through the results ───────────────────────────────────────────────

test('the arrow keys walk every item across group boundaries', () => {
    const flat = flattenSections(buildSections(catalogue, ''));
    assert.ok(flat.length > MAX_PER_GROUP, 'more than one group is in play');
    assert.equal(flat[0].group, 'Actions');
});

test('the highlight wraps at both ends', () => {
    // One press of Up is the fastest way to the last item.
    assert.equal(moveHighlight(0, -1, 5), 4);
    assert.equal(moveHighlight(4, 1, 5), 0);
    assert.equal(moveHighlight(1, 1, 5), 2);
});

test('moving through an empty list stays put instead of dividing by zero', () => {
    assert.equal(moveHighlight(0, 1, 0), 0);
});

// ── Scoped search ────────────────────────────────────────────────────────────

test('a scoped search offers only that group', () => {
    // The magnifier on the profile page means "find a profile". Offering to
    // launch a game or open Preferences there answers a question nobody asked.
    const sections = buildSections(catalogue, '', 'Profiles');
    assert.deepEqual(sections.map((s) => s.group), ['Profiles']);
    assert.deepEqual(flattenSections(sections).map((i) => i.title), ['Modded', 'Vanilla run']);
});

test('a slash in a scoped search does not turn it into a command list', () => {
    // The profile magnifier must stay a profile magnifier whatever is typed.
    const sections = buildSections(catalogue, '/', 'Profiles');
    assert.deepEqual(sections.map((s) => s.group), []);
    assert.deepEqual(
        buildSections(catalogue, '/launch', 'Profiles'),
        [],
        'not even a real command word reaches through'
    );
});

test('a scoped search matches a slash as an ordinary character', () => {
    const profiles = [item({ id: 'p1', title: 'co-op / modded', group: 'Profiles' })];
    const titles = flattenSections(buildSections(profiles, 'op / mod', 'Profiles')).map((i) => i.title);
    assert.deepEqual(titles, ['co-op / modded']);
});

test('a scoped search with no match yields nothing rather than falling back', () => {
    assert.deepEqual(buildSections(catalogue, 'lethal', 'Profiles'), []);
});
