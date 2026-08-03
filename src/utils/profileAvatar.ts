/**
 * Generates a lightweight, repeatable avatar gradient from the beginning and
 * end of a profile name. The seed drives a small set of color-harmony rules,
 * so it has Minecraft-seed character without persisting state or using I/O.
 */
const gradientCache = new Map<string, string>();

const hashName = (value: string): number => {
    let hash = 2166136261;
    for (let index = 0; index < value.length; index += 1) {
        hash = Math.imul(hash ^ value.charCodeAt(index), 16777619);
    }
    return hash >>> 0;
};

const readableCharacters = (value: string) => Array.from(value.trim().toLocaleLowerCase())
    .filter(character => /[\p{L}\p{N}]/u.test(character));

const readableLightness = (hue: number, defaultLightness: number) => (
    hue >= 38 && hue <= 74 ? Math.min(defaultLightness, 37) : defaultLightness
);

export const getProfileAvatarGradient = (profileName: string, fallbackId = ''): string => {
    const key = profileName.trim() || fallbackId || 'profile';
    const cached = gradientCache.get(key);
    if (cached) return cached;

    const characters = readableCharacters(key);
    const first = characters[0]?.codePointAt(0) || 0;
    const last = characters.at(-1)?.codePointAt(0) || 0;
    let seed = (hashName(key) ^ Math.imul(first, 73856093) ^ Math.imul(last, 19349663)) >>> 0;
    const random = () => {
        seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
        return seed / 4294967296;
    };

    const firstHue = Math.floor(random() * 360);
    const harmonyOffsets = [28, 52, 120, 150] as const; // analogous, wider analogous, triadic, split complementary
    const secondHue = (firstHue + harmonyOffsets[Math.floor(random() * harmonyOffsets.length)]) % 360;
    const angle = 118 + Math.floor(random() * 36);
    const start = `hsl(${firstHue} 72% ${readableLightness(firstHue, 47)}%)`;
    const end = `hsl(${secondHue} 68% ${readableLightness(secondHue, 39)}%)`;
    const gradient = `linear-gradient(${angle}deg, ${start} 0%, ${end} 100%)`;
    gradientCache.set(key, gradient);
    return gradient;
};
