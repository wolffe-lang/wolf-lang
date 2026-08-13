/* list_alloc (family B), EXPERT C: a hand-rolled bump arena with a
 * wholesale free — what wolf regions do by default. This is the "expert
 * on their own turf" comparison (secondary, report-only).
 * Protocol: argv[1]=ops; prints {"ns":..,"ops":..,"sink":..}. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

struct node { int64_t v; int64_t next; };

static int64_t build_walk(int64_t nodes) {
    struct node *arena = malloc((size_t)nodes * sizeof *arena);
    for (int64_t i = 0; i < nodes; i++) {
        arena[i].v = i & 1023;
        arena[i].next = i - 1;
    }
    int64_t sum = 0;
    for (int64_t idx = nodes - 1; idx >= 0;) {
        sum = (sum + arena[idx].v) & 1048575;
        idx = arena[idx].next;
    }
    free(arena);
    return sum;
}

int main(int argc, char **argv) {
    int64_t ops = argc > 1 ? (int64_t)strtoull(argv[1], 0, 10) : 200;
    const int64_t nodes = 10000;
    struct timespec t0, t1;
    clock_gettime(CLOCK_MONOTONIC, &t0);
    int64_t sink = 0;
    for (int64_t k = 0; k < ops; k++) sink = (sink + build_walk(nodes)) & 1048575;
    clock_gettime(CLOCK_MONOTONIC, &t1);
    uint64_t ns = (uint64_t)(t1.tv_sec - t0.tv_sec) * 1000000000ull
                + (uint64_t)(t1.tv_nsec - t0.tv_nsec);
    printf("{\"ns\":%llu,\"ops\":%lld,\"sink\":%lld}\n",
           (unsigned long long)ns, (long long)(ops * nodes), (long long)sink);
    return 0;
}
