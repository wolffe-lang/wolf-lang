/* b3-request-churn (family B), NAIVE C: malloc a 16-entry scratch buffer
 * per request, use it, free it.
 * Protocol: argv[1]=ops; prints {"ns":..,"ops":..,"sink":..}. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

struct req { int64_t id; int64_t size; };

static int64_t handle(int64_t id) {
    struct req *buf = malloc(16 * sizeof *buf);
    for (int64_t j = 0; j < 16; j++) {
        buf[j].id = id + j;
        buf[j].size = (id + j) & 31;
    }
    int64_t out = 0;
    for (int64_t t = 0; t < 16; t++) out = (out + buf[t].size) & 65535;
    free(buf);
    return out;
}

int main(int argc, char **argv) {
    int64_t ops = argc > 1 ? (int64_t)strtoull(argv[1], 0, 10) : 20000;
    struct timespec t0, t1;
    clock_gettime(CLOCK_MONOTONIC, &t0);
    int64_t sink = 0;
    for (int64_t i = 0; i < ops; i++) sink = (sink + handle(i)) & 1048575;
    clock_gettime(CLOCK_MONOTONIC, &t1);
    uint64_t ns = (uint64_t)(t1.tv_sec - t0.tv_sec) * 1000000000ull
                + (uint64_t)(t1.tv_nsec - t0.tv_nsec);
    printf("{\"ns\":%llu,\"ops\":%lld,\"sink\":%lld}\n",
           (unsigned long long)ns, (long long)ops, (long long)sink);
    return 0;
}
