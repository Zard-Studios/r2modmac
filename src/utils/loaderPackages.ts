import { LOADER_PACKAGES } from '../data/loaderPackages.ts';

/**
 * Recognising the loader a game needs.
 *
 * The backend reports which runtime a profile runs on; the package that ships
 * that runtime is community-specific. ReturnOfModding games are the clearest
 * case: Risk of Rain Returns installs `ReturnOfModding-ReturnOfModding` while
 * Hades II installs `Hell2Modding-Hell2Modding`, so matching on the literal
 * string "returnofmodding" made the Hades II repair fail with "no compatible
 * loader was found" (issue #38). The package lists come from the Thunderstore
 * ecosystem schema via scripts/generate-loader-map.mjs.
 */
const SCHEMA_SLUGS: Record<string, string[]> = {
    bepinex: ['bepinex', 'bepisloader'],
    returnofmodding: ['return-of-modding'],
    lovely: ['lovely'],
    owml: ['owml'],
    shimloader: ['shimloader'],
};

const packageKey = (fullName: string) => {
    const parts = fullName.split('-');
    return (parts.length >= 2 ? `${parts[0]}-${parts[1]}` : fullName).toLowerCase();
};

/** The `Author-Package` ids known to ship this runtime. */
export const loaderPackageIds = (runtime: string | undefined): string[] => {
    if (!runtime) return [];
    return (SCHEMA_SLUGS[runtime] ?? [runtime]).flatMap(slug => LOADER_PACKAGES[slug] ?? []);
};

/**
 * ReturnOfModding communities must not enter the generic BepInEx bootstrap.
 * The runtime answer normally comes from the backend ecosystem map; the Hades
 * II spelling fallback keeps the guard active when the map is offline.
 */
export const isReturnOfModdingCommunity = (
    community: string,
    runtime?: string,
): boolean => {
    const normalized = community.toLowerCase().replace(/[\s_]+/g, '-');
    return runtime === 'returnofmodding'
        || normalized === 'hades-ii'
        || normalized === 'hadesii';
};

/** The loader packages valid for a particular community, in lookup order. */
export const loaderPackageIdsForCommunity = (
    runtime: string | undefined,
    community: string,
): string[] => {
    const normalizedCommunity = community.toLowerCase().replace(/[\s_]+/g, '-');
    if (runtime === 'returnofmodding' && isReturnOfModdingCommunity(community)) {
        // Hades II is hooked by Hell2Modding's d3d12.dll. The generic
        // ReturnOfModding pack ships version.dll and is not a valid fallback.
        return ['Hell2Modding-Hell2Modding'];
    }
    if (runtime === 'bepinex' && normalizedCommunity === 'hollow-knight-silksong') {
        // The original BepInEx package is now a deprecated migration shim
        // which depends on this package. Installing the shim therefore adds
        // two apparent loaders and lets the old archive overwrite the new one.
        return ['silksong_modding-BepInExPack_Silksong'];
    }
    return loaderPackageIds(runtime);
};

/**
 * Whether `fullName` (with or without a version suffix) is the loader itself.
 *
 * BepInEx keeps a name check alongside the schema list: communities publish
 * their own `BepInExPack_<Game>` forks, and a fork too new for the bundled
 * snapshot is still the runtime the profile is installing.
 */
export const isLoaderPackage = (
    runtime: string | undefined,
    fullName: string,
): boolean => {
    if (!runtime) return false;
    const key = packageKey(fullName);
    if (loaderPackageIds(runtime).some(id => id.toLowerCase() === key)) return true;
    // The package part only, and matched whole: mods built around the runtime
    // carry its name — RoR2BepInExPack and BepInEx_GUI among them — and reading
    // them as the loader made Apply skip installing them entirely.
    if (runtime === 'bepinex') {
        const pkg = key.slice(key.indexOf('-') + 1);
        return pkg === 'bepinexpack' || pkg.startsWith('bepinexpack_');
    }
    if (runtime === 'owml') return key === 'owml' || key.endsWith('-owml');
    if (runtime === 'lovely') return key === 'lovely' || key.endsWith('-lovely');
    return false;
};

/** A loader candidate Repair may install rather than merely recognise. */
export const isRepairableLoaderPackage = (
    runtime: string | undefined,
    candidate: { full_name: string; is_deprecated?: boolean },
): boolean => !candidate.is_deprecated && isLoaderPackage(runtime, candidate.full_name);

/** Human-readable loader name for messages. */
export const loaderDisplayName = (runtime: string): string => {
    switch (runtime) {
        case 'bepinex': return 'BepInEx';
        case 'owml': return 'OWML';
        case 'returnofmodding': return 'ReturnOfModding';
        case 'lovely': return 'Lovely';
        case 'shimloader': return 'Shimloader';
        default:
            // An unsupported loader is reported by its ecosystem slug
            // (`melonloader`, `gdweave`, ...); show it as written rather than
            // inventing a name for it.
            return runtime;
    }
};
