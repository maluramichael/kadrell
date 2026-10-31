#ifndef SystemInfo_h
#define SystemInfo_h

#include <stdint.h>

// Lesende Systemwerte für `kadrell.system` im ext-host. In C, weil Swift 6 `mach_task_self_` als globalen,
// veränderlichen Zustand ablehnt.

// Zählerstände je Kern seit dem Start: `busy` (user + system + nice) und `total` (dazu idle), abwechselnd in `out`
// (2 * max Werte). Gibt die Zahl der Kerne zurück (höchstens `max`), -1 bei Fehler.
int ks_cpu_ticks(uint64_t *out, int max);

#endif
