// Writes into the game's compiled code the way Apple Silicon allows: code pages are mapped MAP_JIT, writable only
// after pthread_jit_write_protect_np(0) on this thread, and while they're writable this thread can't run any of them.
// So the whole write happens here, in native code, and the managed caller only runs again once they're locked.
#include <libkern/OSCacheControl.h>
#include <pthread.h>
#include <string.h>

int jw_ready(void) { return pthread_jit_write_protect_supported_np(); }

void jw_write(void *dst, const void *src, size_t size) {
  pthread_jit_write_protect_np(0);
  memcpy(dst, src, size);
  pthread_jit_write_protect_np(1);
  sys_icache_invalidate(dst, size);
}

void jw_flush(void *start, size_t size) { sys_icache_invalidate(start, size); }
