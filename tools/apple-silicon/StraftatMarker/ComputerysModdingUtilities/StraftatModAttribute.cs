using System;

// The shape of STRAFTAT's attribute, for compiling the marker only.
namespace ComputerysModdingUtilities
{
    [AttributeUsage(AttributeTargets.Assembly, AllowMultiple = false, Inherited = false)]
    public sealed class StraftatModAttribute : Attribute
    {
        public bool IsVanillaCompatible { get; }

        public StraftatModAttribute(bool isVanillaCompatible = false)
        {
            IsVanillaCompatible = isVanillaCompatible;
        }
    }
}
