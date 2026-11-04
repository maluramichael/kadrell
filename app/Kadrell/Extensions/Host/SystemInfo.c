#include "SystemInfo.h"

#include <mach/mach.h>
#include <sys/sysctl.h>

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

int ks_mem_stats(uint64_t *total, uint64_t *used, uint64_t *free) {
    // Phys. Gesamtspeicher einmal aus sysctl holen; die VM-Zähler brauchen den Host-Port einmalig.
    static host_t host = MACH_PORT_NULL;
    if (host == MACH_PORT_NULL) host = mach_host_self();

    uint64_t t = 0;
    size_t len = sizeof(t);
    if (sysctlbyname("hw.memsize", &t, &len, NULL, 0) != 0 || t == 0) return -1;

    vm_statistics64_data_t stats;
    mach_msg_type_number_t count = sizeof(stats) / sizeof(natural_t);
    if (host_statistics64(host, HOST_VM_INFO64, (host_info64_t)&stats, &count) != KERN_SUCCESS) return -1;

    // "Memory Used" wie in der Aktivitätsanzeige: aktiv + wired + komprimiert. Gedrosselte Seiten zählen
    // schon in den komprimierten ein, sonst würden sie doppelt gezählt.
    uint64_t page = vm_page_size;
    int64_t compressed = (int64_t)stats.compressor_page_count - (int64_t)stats.throttled_count;
    if (compressed < 0) compressed = 0;
    uint64_t u = (uint64_t)stats.active_count * page + (uint64_t)stats.wire_count * page
                 + (uint64_t)compressed * page;
    if (u > t) u = t;
    *total = t;
    *used = u;
    *free = t - u;
    return 0;
}
