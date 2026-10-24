using System;
using System.IO;
using System.Reflection;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using MonoMod.RuntimeDetour;
using MonoMod.RuntimeDetour.Platforms;

// Doorstop runs Doorstop.Entrypoint.Start() in its target assembly. This one gives MonoMod a way to write code on
// Apple Silicon, then starts BepInEx's own entry point (DOORSTOP_INVOKE_DLL_PATH) as if Doorstop had called it.
namespace Doorstop
{
    public static class Entrypoint
    {
        public static void Start()
        {
            AppleSiliconMods.Loader.Run();
        }
    }
}

namespace AppleSiliconMods
{
    internal static class Loader
    {
        private static readonly string Here = Path.GetDirectoryName(typeof(Loader).Assembly.Location);

        internal static void Log(string line)
        {
            try { File.AppendAllText(Path.Combine(Here, "apple_silicon.log"), $"{DateTime.Now:yyyy-MM-dd HH:mm:ss} {line}\n"); }
            catch { }
        }

        private static bool _started;

        internal static void Run()
        {
            if (_started) throw new InvalidOperationException("Started twice: Doorstop was pointed back at this shim");
            _started = true;
            try
            {
                Setup();
            }
            catch (Exception e)
            {
                Log($"Setting up failed, BepInEx starts without it: {e}");
            }
            try
            {
                LoadGameMarkers();
            }
            catch (Exception e)
            {
                Log($"Loading the STRAFTAT marker failed: {e}");
            }

            // Doorstop sets DOORSTOP_INVOKE_DLL_PATH to the assembly it started (this one); BepInEx reads it to find
            // itself, so it gets BepInEx's preloader back before BepInEx starts.
            string preloader = PreloaderPath();
            Environment.SetEnvironmentVariable("DOORSTOP_INVOKE_DLL_PATH", preloader);
            Assembly.LoadFrom(preloader).GetType("Doorstop.Entrypoint", true).GetMethod("Start", BindingFlags.Public | BindingFlags.Static).Invoke(null, null);
        }

        // Separate from Run, so MonoMod failing to load is caught there and BepInEx still starts.
        [MethodImpl(MethodImplOptions.NoInlining)]
        private static void Setup()
        {
            // An x86_64 game (Rosetta) can't load the arm64-only helper: it then runs exactly as without this.
            if (JitWrite.TryLoad(Path.Combine(Here, "libjitwrite.dylib")))
            {
                DetourHelper.Native = new JitWritePlatform();
                Log("Native Apple Silicon: MonoMod writes code through libjitwrite");
            }
            else
            {
                Log("Not native arm64 (or no helper): MonoMod left as it is");
            }
        }

        // STRAFTAT treats every assembly loaded from outside its Managed folder as a mod that is incompatible with
        // vanilla matchmaking unless it is marked otherwise. In a game that ships that check, load the marker that
        // says the shim changes no gameplay (see StraftatMarker/).
        private static void LoadGameMarkers()
        {
            string managed = Environment.GetEnvironmentVariable("DOORSTOP_MANAGED_FOLDER_DIR");
            if (string.IsNullOrEmpty(managed)) managed = Path.GetDirectoryName(typeof(object).Assembly.Location);
            string marker = Path.Combine(Here, "AppleSiliconMods.Straftat.dll");
            if (File.Exists(Path.Combine(managed, "ComputerysModdingUtilities.dll")) && File.Exists(marker))
            {
                Assembly.LoadFrom(marker);
                Log("STRAFTAT: the shim is marked as compatible with vanilla matchmaking");
            }
        }

        // BepInEx's preloader: in the folder r2modmac hands Mono as its DLL search path (the game's BepInEx/core, or a
        // profile's), else beside the game.
        internal static string PreloaderPath()
        {
            string search = Environment.GetEnvironmentVariable("DOORSTOP_MONO_DLL_SEARCH_PATH_OVERRIDE") ?? "";
            foreach (string dir in search.Split(new[] { ':' }, StringSplitOptions.RemoveEmptyEntries))
            {
                string candidate = Path.Combine(dir, "BepInEx.Preloader.dll");
                if (File.Exists(candidate)) return candidate;
            }
            string beside = Path.GetFullPath(Path.Combine(Here, "..", "BepInEx", "core", "BepInEx.Preloader.dll"));
            if (File.Exists(beside)) return beside;
            throw new FileNotFoundException("BepInEx.Preloader.dll not found in the DLL search path or beside the game");
        }
    }

    internal static class JitWrite
    {
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)] private delegate int ReadyFn();
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)] private delegate void WriteFn(IntPtr dst, IntPtr src, UIntPtr size);
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)] private delegate void FlushFn(IntPtr start, UIntPtr size);

        private static WriteFn _write;
        private static FlushFn _flush;

        [DllImport("libSystem.B.dylib")] private static extern IntPtr dlopen(string path, int mode);
        [DllImport("libSystem.B.dylib")] private static extern IntPtr dlsym(IntPtr handle, string symbol);

        internal static bool TryLoad(string path)
        {
            if (IntPtr.Size != 8 || !File.Exists(path)) return false;
            IntPtr lib = dlopen(path, 2 /* RTLD_NOW */);
            if (lib == IntPtr.Zero) return false;
            T Fn<T>(string name) where T : Delegate => (T)Marshal.GetDelegateForFunctionPointer(dlsym(lib, name), typeof(T));
            if (Fn<ReadyFn>("jw_ready")() == 0) return false;
            _write = Fn<WriteFn>("jw_write");
            _flush = Fn<FlushFn>("jw_flush");
            return true;
        }

        internal static void Write(IntPtr dst, IntPtr src, uint size) => _write(dst, src, (UIntPtr)size);
        internal static void Flush(IntPtr start, uint size) => _flush(start, (UIntPtr)size);
    }

    // MonoMod's own ARM platform, with every write to code done by libjitwrite. The jump is MonoMod's AArch64 one,
    // but through x17 (ldr x17, #8; br x17; then the 8-byte target address): x16/x17 are the AArch64 ABI's
    // intra-procedure-call scratch registers, free at a call's entry (Mono's own prologues overwrite them first),
    // where x15 isn't guaranteed to be.
    internal sealed class JitWritePlatform : IDetourNativePlatform
    {
        private const byte AArch64 = 4; // DetourNativeARMPlatform.DetourType.AArch64
        private const uint JumpSize = 16;
        private readonly DetourNativeARMPlatform _arm = new DetourNativeARMPlatform();

        public NativeDetourData Create(IntPtr from, IntPtr to, byte? type = null) => _arm.Create(from, to, type ?? AArch64);

        public void Free(NativeDetourData detour) { }

        public unsafe void Apply(NativeDetourData detour)
        {
            if (detour.Type != AArch64) throw new NotSupportedException($"Only AArch64 jumps are written natively, not type {detour.Type}");
            byte* jump = stackalloc byte[(int)JumpSize];
            *(uint*)jump = 0x58000051; // ldr x17, #8
            *(uint*)(jump + 4) = 0xD61F0220; // br x17
            *(ulong*)(jump + 8) = (ulong)(long)detour.Target;
            JitWrite.Write(detour.Method, (IntPtr)jump, JumpSize);
        }

        public void Copy(IntPtr src, IntPtr dst, byte type)
        {
            if (type != AArch64) throw new NotSupportedException($"Only AArch64 jumps are copied natively, not type {type}");
            JitWrite.Write(dst, src, JumpSize);
        }

        public void MakeWritable(IntPtr src, uint size) { }
        public void MakeExecutable(IntPtr src, uint size) { }
        public void MakeReadWriteExecutable(IntPtr src, uint size) { }
        public void FlushICache(IntPtr src, uint size) => JitWrite.Flush(src, size);
        public IntPtr MemAlloc(uint size) => Marshal.AllocHGlobal((int)size);
        public void MemFree(IntPtr ptr) => Marshal.FreeHGlobal(ptr);
    }
}
