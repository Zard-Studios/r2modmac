import type { Package, PackageVersion } from '../types/thunderstore';

const VERSION_SUFFIX = /-(\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?)$/;

export interface PackageReference {
    packageName: string;
    version: string | null;
    fullName: string;
}

export function parsePackageReference(fullName: string): PackageReference {
    const match = fullName.match(VERSION_SUFFIX);
    if (!match || match.index === undefined) {
        return { packageName: fullName, version: null, fullName };
    }
    return {
        packageName: fullName.slice(0, match.index),
        version: match[1],
        fullName,
    };
}

export function packageIdentityKey(fullName: string): string {
    return parsePackageReference(fullName).packageName.toLowerCase();
}

export function satisfiesMinimumVersion(installedVersion: string, requiredVersion: string): boolean {
    return compareVersions(installedVersion, requiredVersion) >= 0;
}

export function findPinnedVersion(pkg: Package, requestedVersion: string, label = pkg.full_name): PackageVersion {
    const version = pkg.versions.find(candidate => candidate.version_number === requestedVersion);
    if (!version) {
        throw new Error(`${label}: pinned version ${requestedVersion} is unavailable; refusing to use latest`);
    }
    return version;
}

function compareIdentifier(left: string, right: string): number {
    const leftNumeric = /^\d+$/.test(left);
    const rightNumeric = /^\d+$/.test(right);
    if (leftNumeric && rightNumeric) return Number(left) - Number(right);
    if (leftNumeric !== rightNumeric) return leftNumeric ? -1 : 1;
    return left.localeCompare(right);
}

export function compareVersions(left: string, right: string): number {
    const parse = (value: string) => {
        const withoutBuild = value.split('+', 1)[0];
        const prereleaseStart = withoutBuild.indexOf('-');
        const core = prereleaseStart === -1 ? withoutBuild : withoutBuild.slice(0, prereleaseStart);
        const prerelease = prereleaseStart === -1 ? undefined : withoutBuild.slice(prereleaseStart + 1);
        return {
            core: core.split('.').map(part => Number.parseInt(part, 10) || 0),
            prerelease: prerelease?.split('.') ?? [],
        };
    };
    const a = parse(left);
    const b = parse(right);
    for (let index = 0; index < Math.max(a.core.length, b.core.length); index++) {
        const difference = (a.core[index] ?? 0) - (b.core[index] ?? 0);
        if (difference !== 0) return difference;
    }
    if (a.prerelease.length === 0 || b.prerelease.length === 0) {
        return a.prerelease.length === b.prerelease.length ? 0 : a.prerelease.length === 0 ? 1 : -1;
    }
    for (let index = 0; index < Math.max(a.prerelease.length, b.prerelease.length); index++) {
        if (a.prerelease[index] === undefined) return -1;
        if (b.prerelease[index] === undefined) return 1;
        const difference = compareIdentifier(a.prerelease[index], b.prerelease[index]);
        if (difference !== 0) return difference;
    }
    return 0;
}

export function hasNewerVersion(installed: string, available?: string): boolean {
    return !!available && compareVersions(available, installed) > 0;
}

export function latestVersionNumber(pkg: Package): string | undefined {
    // Thunderstore's lightweight listing currently marks many latest releases
    // as inactive, while the exact-version endpoint can mark historical pins as
    // active. `is_active` therefore cannot define "latest": doing so makes the
    // update count shrink while Apply caches the old pinned versions.
    return pkg.versions
        .reduce<string | undefined>((latest, version) => (
            !latest || compareVersions(version.version_number, latest) > 0
                ? version.version_number
                : latest
        ), undefined);
}
