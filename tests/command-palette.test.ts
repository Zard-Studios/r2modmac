import assert from 'node:assert/strict';
import test from 'node:test';

import {
    GROUP_ORDER,
    MAX_PER_GROUP,
    buildSections,
    findShortcutItem,
    flattenSections,
    moveHighlight,
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
    item({ id: 'a1', title: 'Launch game (modded)', group: 'Actions' }),
    item({ id: 'a2', title: 'Launch game (unmodded)', group: 'Actions' }),
    item({ id: 'a3', title: 'Apply mods to game', group: 'Actions' }),
    item({ id: 'p1', title: 'Modded', group: 'Profiles' }),
    item({ id: 'p2', title: 'Vanilla run', group: 'Profiles' }),
    item({ id: 'g1', title: 'Lethal Company', group: 'Games', subtitle: 'lethal-company' }),
    item({ id: 'g2', title: 'Outer Wilds', group: 'Games', subtitle: 'outerwilds' }),
    item({ id: 's1', title: 'Theme', group: 'Settings' }),
];

// ── Ordinary search ──────────────────────────────────────────────────────────

test('actions are found by their visible name', () => {
    const titles = flattenSections(buildSections(catalogue, 'launch')).map((i) => i.title);
    assert.deepEqual(titles, ['Launch game (modded)', 'Launch game (unmodded)']);
});

test('a slash is an ordinary character, not a separate command mode', () => {
    assert.deepEqual(buildSections(catalogue, '/'), []);
    assert.equal(scoreItem(item({ id: 'p1', title: 'co-op / modded', group: 'Profiles' }), '/ mod') !== null, true);
});

test('a shortcut resolves to the action offered by the active context', () => {
    const actions = [
        item({
            id: 'new:muck',
            title: 'New profile',
            group: 'Actions',
            game: 'muck',
            shortcut: 'new-profile',
        }),
        item({
            id: 'new:peak',
            title: 'New profile',
            group: 'Actions',
            game: 'peak',
            shortcut: 'new-profile',
        }),
    ];

    const found = findShortcutItem(actions, 'new-profile', {
        group: 'Profiles',
        game: { identifier: 'muck', name: 'Muck' },
    });
    assert.equal(found?.id, 'new:muck');
});

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
    assert.equal(flat[0].group, 'Games');
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
    const sections = buildSections(catalogue, '', { group: 'Profiles' });
    assert.deepEqual(sections.map((s) => s.group), ['Profiles']);
    assert.deepEqual(flattenSections(sections).map((i) => i.title), ['Modded', 'Vanilla run']);
});

test('a slash with no matching profile yields no scoped results', () => {
    const sections = buildSections(catalogue, '/', { group: 'Profiles' });
    assert.deepEqual(sections.map((s) => s.group), []);
});

test('a scoped search matches a slash as an ordinary character', () => {
    const profiles = [item({ id: 'p1', title: 'co-op / modded', group: 'Profiles' })];
    const titles = flattenSections(buildSections(profiles, 'op / mod', { group: 'Profiles' })).map((i) => i.title);
    assert.deepEqual(titles, ['co-op / modded']);
});

test('a scoped search with no match yields nothing rather than falling back', () => {
    assert.deepEqual(buildSections(catalogue, 'lethal', { group: 'Profiles' }), []);
});

test('an item with no title does not take the whole search down', () => {
    // Real game lists contain the occasional entry with nothing readable on it.
    const messy = [
        item({ id: 'g1', title: undefined as unknown as string }),
        item({ id: 'g2', title: 'Lethal Company' }),
    ];
    assert.deepEqual(flattenSections(buildSections(messy, 'lethal')).map((i) => i.title), ['Lethal Company']);
});

// ── Pinned to one game ───────────────────────────────────────────────────────

const acrossGames: CommandItem[] = [
    item({ id: 'p1', title: 'Modded', group: 'Profiles', game: 'lethal-company' }),
    item({ id: 'p2', title: 'Modded', group: 'Profiles', game: 'outerwilds' }),
    item({ id: 'p3', title: 'Casual', group: 'Profiles', game: 'lethal-company' }),
    item({ id: 'g1', title: 'Lethal Company', group: 'Games', game: 'lethal-company' }),
];

test('a search pinned to a game sees only that game profiles', () => {
    // On a game profile page the question is always which of *these* profiles.
    const sections = buildSections(acrossGames, '', {
        group: 'Profiles',
        game: { identifier: 'lethal-company', name: 'Lethal Company' },
    });
    assert.deepEqual(flattenSections(sections).map((i) => i.id), ['p3', 'p1']);
});

test('the pin narrows before the search runs, not after', () => {
    // "Modded" exists under two games; only the pinned one may come back.
    const sections = buildSections(acrossGames, 'Modded', {
        group: 'Profiles',
        game: { identifier: 'outerwilds', name: 'Outer Wilds' },
    });
    assert.deepEqual(flattenSections(sections).map((i) => i.id), ['p2']);
});

test('dropping the pin widens the search back to everything', () => {
    // What clearing the tag has to do: the narrow case is a starting point,
    // not a separate mode with its own results.
    const widened = flattenSections(buildSections(acrossGames, 'Modded', null));
    assert.deepEqual(widened.map((i) => i.id).sort(), ['p1', 'p2']);
});

test('an item belonging to no game is excluded while a pin is on', () => {
    const withGlobal = [...acrossGames, item({ id: 's1', title: 'Preferences', group: 'Settings' })];
    const sections = buildSections(withGlobal, '', {
        group: 'Settings',
        game: { identifier: 'lethal-company', name: 'Lethal Company' },
    });
    assert.deepEqual(sections, []);
});

test('a game pin includes actions belonging to that game', () => {
    const items = [
        ...acrossGames,
        item({ id: 'a1', title: 'New profile', group: 'Actions', game: 'lethal-company' }),
        item({ id: 'a2', title: 'New profile', group: 'Actions', game: 'outerwilds' }),
    ];
    const sections = buildSections(items, '', {
        group: 'Profiles',
        game: { identifier: 'lethal-company', name: 'Lethal Company' },
    });
    assert.deepEqual(sections.map((section) => section.group), ['Actions', 'Profiles']);
    assert.deepEqual(flattenSections(sections).map((entry) => entry.id), ['a1', 'p3', 'p1']);
});

test('a game pin never leaks actions from a previously active profile', () => {
    const items = [
        ...acrossGames,
        item({ id: 'browse', title: 'Browse mods', group: 'Actions', game: 'lethal-company' }),
        item({
            id: 'launch:p1',
            title: 'Launch game (modded)',
            group: 'Actions',
            game: 'lethal-company',
            profile: 'p1',
        }),
    ];
    const sections = buildSections(items, '', {
        group: 'Profiles',
        game: { identifier: 'lethal-company', name: 'Lethal Company' },
    });
    const ids = flattenSections(sections).map((entry) => entry.id);

    assert.ok(ids.includes('browse'));
    assert.ok(ids.includes('p1'));
    assert.ok(!ids.includes('launch:p1'));
});

test('home orders games before profiles', () => {
    const sections = buildSections(acrossGames, '');
    assert.deepEqual(sections.map((section) => section.group), ['Games', 'Profiles']);
});

test('a profile pin shows only actions for that profile', () => {
    const items = [
        item({ id: 'a1', title: 'Launch', group: 'Actions', game: 'lethal-company', profile: 'p1' }),
        item({ id: 'a2', title: 'Launch', group: 'Actions', game: 'lethal-company', profile: 'p2' }),
        ...acrossGames,
    ];
    const sections = buildSections(items, '', {
        group: 'Profiles',
        game: { identifier: 'lethal-company', name: 'Lethal Company' },
        profile: { identifier: 'p1', name: 'Modded' },
    });
    assert.deepEqual(sections.map((section) => section.group), ['Actions']);
    assert.deepEqual(flattenSections(sections).map((entry) => entry.id), ['a1']);
});

test('context-only actions stay hidden until their game is pinned', () => {
    const browse = item({
        id: 'browse',
        title: 'Browse mods',
        group: 'Actions',
        game: 'lethal-company',
        contextOnly: true,
    });
    assert.ok(!flattenSections(buildSections([...acrossGames, browse], '')).some((entry) => entry.id === 'browse'));

    const scoped = buildSections([...acrossGames, browse], '', {
        group: 'Profiles',
        game: { identifier: 'lethal-company', name: 'Lethal Company' },
    });
    assert.ok(flattenSections(scoped).some((entry) => entry.id === 'browse'));
});
