#include "SystemInfo.h"

#include <mach/mach.h>

int ks_cpu_ticks(uint64_t *out, int max) {
    // Das Host-Port-Recht einmal holen: jeder Aufruf von mach_host_self() erhöht den Referenzzähler.
    static host_t host = MACH_PORT_NULL;
    if (host == MACH_PORT_NULL) host = mach_host_self();
    natural_t count = 0;
    processor_info_array_t info = NULL;
    mach_msg_type_number_t infoCount = 0;
    if (host_processor_info(host, PROCESSOR_CPU_LOAD_INFO, &count, &info, &infoCount) != KERN_SUCCESS) return -1;
    int cores = (int)count < max ? (int)count : max;
    for (int i = 0; i < cores; i++) {
        const integer_t *t = info + CPU_STATE_MAX * i;
        uint64_t busy = (uint64_t)(uint32_t)t[CPU_STATE_USER] + (uint32_t)t[CPU_STATE_SYSTEM] + (uint32_t)t[CPU_STATE_NICE];
        out[2 * i] = busy;
        out[2 * i + 1] = busy + (uint32_t)t[CPU_STATE_IDLE];
    }
    vm_deallocate(mach_task_self(), (vm_address_t)info, infoCount * sizeof(integer_t));
    return cores;
}
