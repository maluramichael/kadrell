#ifndef SystemInfo_h
#define SystemInfo_h

#include <stdint.h>

// Lesende Systemwerte für `kadrell.system` im ext-host. In C, weil Swift 6 `mach_task_self_` als globalen,
// veränderlichen Zustand ablehnt.

// Zählerstände je Kern seit dem Start: `busy` (user + system + nice) und `total` (dazu idle), abwechselnd in `out`
// (2 * max Werte). Gibt die Zahl der Kerne zurück (höchstens `max`), -1 bei Fehler.
int ks_cpu_ticks(uint64_t *out, int max);

// Hauptspeicher in Bytes: `total` physik, `used` aktiv + wired + komprimiert (wie „Memory Used" in der
// Aktivitätsanzeige), `free` = total - used. 0 bei Erfolg, -1 wenn der Kernel nichts liefert.
int ks_mem_stats(uint64_t *total, uint64_t *used, uint64_t *free);

#endif
