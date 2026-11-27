#!/usr/bin/env node
// Regenerates src-tauri/src/models/loader_map.json from the Thunderstore
// ecosystem schema.
//
// The schema is the only authoritative statement of which loader a community
// uses (`games[*].r2modman[*].packageLoader`) and which packages ship that
// loader (`modloaderPackages`). Guessing "BepInEx" for every community is what
// made ReturnOfModding games such as Hades II ask for a BepInEx repair that
// could never succeed, so the map is generated, never hand-written.
//
// Usage: node scripts/generate-loader-map.mjs

import { writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const SCHEMA_URL = 'https://thunderstore.io/api/experimental/schema/dev/latest/';
const ROOT = dirname(dirname(fileURLToPath(import.meta.url)));
const OUTPUT = join(ROOT, 'src-tauri/src/models/loader_map.json');
// The frontend only needs to recognise a loader package by name (to spot one
// already staged in a profile, and to reinstall the right one on repair), so it
// gets the package lists without the community table.
const FRONTEND_OUTPUT = join(ROOT, 'src/data/loaderPackages.ts');

const response = await fetch(SCHEMA_URL);
if (!response.ok) {
  throw new Error(`Ecosystem schema request failed: ${response.status} ${response.statusText}`);
}
const schema = await response.json();

const communities = {};
const shimloaderGames = {};
// Where each game lives in a Steam library, as the schema states it. Finding a
// game by comparing its name to the folder names only works while the two
// happen to look alike; `Ride` is RV There Yet?, `20MinuteTillDawn` is
// 20 Minutes Till Dawn. The app id and the folder are exact.
const steamGames = {};
for (const [gameKey, game] of Object.entries(schema.games ?? {})) {
  for (const entry of game.r2modman ?? []) {
    // A game entry names its community through its package index URL; the game
    // key itself is not always the community slug (`gtfo` vs `gtfo` differ for
    // some titles, and several games share one community).
    const match = /\/c\/([^/]+)\//.exec(entry.packageIndex ?? '');
    const community = match ? match[1] : gameKey;

    // Dedicated servers have their own entry and their own app id; the game
    // is what a profile launches.
    if ((entry.gameInstanceType ?? 'game') === 'game') {
      const steam = (steamGames[community] ??= { appIds: [], folders: [], exeNames: [] });
      for (const distribution of entry.distributions ?? []) {
        const appId = String(distribution.identifier ?? '').trim();
        if (distribution.platform === 'steam' && appId && !steam.appIds.includes(appId)) {
          steam.appIds.push(appId);
        }
      }
      // Some entries name a folder inside the install (`Hades II/Ship`); the
      // install folder is the first part.
      const folder = String(entry.steamFolderName ?? '').split(/[\\/]/)[0].trim();
      if (folder && !steam.folders.includes(folder)) steam.folders.push(folder);
      // The executables tell a real install from a folder that only kept
      // saves or mod files after the game was uninstalled.
      for (const exe of entry.exeNames ?? []) {
        const name = String(exe).trim();
        if (name && !steam.exeNames.includes(name)) steam.exeNames.push(name);
      }
    }

    const loader = entry.packageLoader;
    if (!loader) continue;
    const known = communities[community];
    if (known && known !== loader) {
      throw new Error(`Community ${community} declares conflicting loaders: ${known} vs ${loader}`);
    }
    communities[community] = loader;

    if (loader === 'shimloader') {
      // Shimloader hands the game its directories on the command line and
      // proxies UE4SS from `<game>/<dataFolderName>/Binaries/Win64`, so the
      // data folder is part of installing it, not just of naming the game.
      const pakRule = (entry.installRules ?? []).find(rule => rule.route === 'shimloader/pak');
      shimloaderGames[community] = {
        dataFolder: entry.dataFolderName ?? '',
        pakExtensions: pakRule?.defaultFileExtensions ?? [],
      };
    }
  }
}

const loaderPackages = {};
for (const entry of schema.modloaderPackages ?? []) {
  const loader = entry.loader;
  if (!loader || !entry.packageId) continue;
  (loaderPackages[loader] ??= []).push({
    packageId: entry.packageId,
    rootFolder: entry.rootFolder ?? '',
  });
}

for (const [community, steam] of Object.entries(steamGames)) {
  steam.appIds.sort();
  steam.folders.sort((a, b) => a.localeCompare(b));
  if (steam.appIds.length === 0 && steam.folders.length === 0 && steam.exeNames.length === 0) {
    delete steamGames[community];
  }
}

const sortObject = (value) => Object.fromEntries(Object.entries(value).sort(([a], [b]) => a.localeCompare(b)));
for (const loader of Object.keys(loaderPackages)) {
  loaderPackages[loader].sort((a, b) => a.packageId.localeCompare(b.packageId));
}

const map = {
  source: SCHEMA_URL,
  schemaVersion: schema.schemaVersion ?? null,
  generatedFrom: 'thunderstore ecosystem schema',
  communities: sortObject(communities),
  loaderPackages: sortObject(loaderPackages),
  shimloaderGames: sortObject(shimloaderGames),
  steamGames: sortObject(steamGames),
};

writeFileSync(OUTPUT, `${JSON.stringify(map, null, 2)}\n`);

const frontendPackages = sortObject(Object.fromEntries(
  Object.entries(loaderPackages).map(([loader, packages]) => [loader, packages.map(entry => entry.packageId)])
));
writeFileSync(
  FRONTEND_OUTPUT,
  '// Generated by scripts/generate-loader-map.mjs from the Thunderstore ecosystem schema.\n' +
  '// Do not edit by hand: it states which packages ship each mod loader.\n' +
  `export const LOADER_PACKAGES: Record<string, string[]> = ${JSON.stringify(frontendPackages, null, 2)};\n`
);

console.log(
  `Wrote ${OUTPUT}: ${Object.keys(map.communities).length} communities, ` +
  `${Object.keys(map.loaderPackages).length} loaders, ` +
  `${Object.keys(map.steamGames).length} Steam games`
);
console.log(`Wrote ${FRONTEND_OUTPUT}`);
