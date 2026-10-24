using System;
using ComputerysModdingUtilities;

// STRAFTAT counts every assembly loaded from outside its Managed folder as an incompatible mod unless the assembly, or
// a vanilla-compatible assembly that references it, says otherwise. The shim changes no gameplay (it only lets
// Harmony write its jumps under Apple Silicon's W^X), so this marker says so and references the shim.
[assembly: StraftatMod(isVanillaCompatible: true)]

namespace AppleSiliconMods.Straftat
{
    internal static class Marker
    {
        // Keeps AppleSiliconMods among this assembly's references, which is what STRAFTAT follows.
        internal static readonly Type Shim = typeof(Doorstop.Entrypoint);
    }
}
