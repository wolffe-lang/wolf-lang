/* word_count: count whitespace-separated words in a ~1MB synthetic buffer
 * per op. The string kernel: byte-scan baseline for wolf's zero-copy
 * `words()` views (D25). Protocol: argv[1]=ops; prints {"ns":..,"ops":..}. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

#define LEN (1 << 20)

volatile uint64_t sink;

int main(int argc, char **argv) {
    uint64_t ops = argc > 1 ? strtoull(argv[1], 0, 10) : 50;
    char *buf = malloc(LEN);
    uint32_t seed = 0x9e3779b9u;
    for (int i = 0; i < LEN; i++) {
        seed = seed * 1664525u + 1013904223u;
        buf[i] = (seed >> 28) == 0 ? ' ' : (char)('a' + (seed % 26));
    }
    struct timespec t0, t1;
    clock_gettime(CLOCK_MONOTONIC, &t0);
    for (uint64_t k = 0; k < ops; k++) {
        uint64_t words = 0;
        int in_word = 0;
        for (int i = 0; i < LEN; i++) {
            int ws = buf[i] == ' ' || buf[i] == '\n' || buf[i] == '\t';
            words += (uint64_t)(!ws && !in_word);
            in_word = !ws;
        }
        sink = words;
    }
    clock_gettime(CLOCK_MONOTONIC, &t1);
    free(buf);
    uint64_t ns = (uint64_t)(t1.tv_sec - t0.tv_sec) * 1000000000ull
                + (uint64_t)(t1.tv_nsec - t0.tv_nsec);
    printf("{\"ns\":%llu,\"ops\":%llu}\n",
           (unsigned long long)ns, (unsigned long long)ops);
    return 0;
}
