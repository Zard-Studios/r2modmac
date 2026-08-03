/**
 * Curated color harmonies for generated profile avatars.  The profile id acts
 * as a stable seed, so profiles vary without a saved color, extra I/O, or
 * changing appearance between renders.
 */
const avatarGradients = [
    'linear-gradient(135deg, #2563eb 0%, #7c3aed 100%)', // analogous: blue → violet
    'linear-gradient(135deg, #0891b2 0%, #2563eb 100%)', // analogous: cyan → blue
    'linear-gradient(135deg, #0f766e 0%, #0369a1 100%)', // analogous: teal → azure
    'linear-gradient(135deg, #4338ca 0%, #be185d 100%)', // split-complementary: indigo → rose
    'linear-gradient(135deg, #b45309 0%, #c2410c 100%)', // analogous: amber → vermilion
    'linear-gradient(135deg, #047857 0%, #4d7c0f 100%)', // analogous: emerald → olive
    'linear-gradient(135deg, #9d174d 0%, #6d28d9 100%)', // analogous: magenta → violet
    'linear-gradient(135deg, #1d4ed8 0%, #0f766e 100%)', // cool triad: blue → teal
] as const;

const stableHash = (value: string): number => {
    let hash = 0;
    for (let index = 0; index < value.length; index += 1) {
        hash = ((hash << 5) - hash + value.charCodeAt(index)) | 0;
    }
    return Math.abs(hash);
};

export const getProfileAvatarGradient = (profileId: string, profileName = ''): string => (
    avatarGradients[stableHash(profileId || profileName || 'profile') % avatarGradients.length]
);
