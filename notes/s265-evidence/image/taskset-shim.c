/* s265: a disposition-neutral taskset shim for the lane's container on
 * kasumi CPUs 4-7 (wolf-lang#925; s264's notes/s264-evidence/taskset-shim.c
 * is the reference). A CPU set inside 0-3 is moved up by four (0-3 -> 4-7,
 * 0 -> 4); a set already inside 4-7 passes unchanged; anything else is
 * refused. No shell, interpreter, signal call or descriptor change: the
 * only work is argv, then execv of the real taskset. */
#define _POSIX_C_SOURCE 200809L
#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

static const char real[] = "/usr/bin/taskset.real";

static void fail(const char *m) {
    (void)!write(STDERR_FILENO, m, strlen(m));
    _exit(2);
}

static void pass(char **argv) {
    execv(real, argv);
    fail("s265 taskset: execv taskset.real failed\n");
}

static unsigned number(const char **at) {
    const char *p = *at;
    unsigned n = 0;
    if (*p < '0' || *p > '9') fail("s265 taskset: invalid CPU list\n");
    do {
        n = n * 10 + (unsigned)(*p++ - '0');
        if (n >= 64) fail("s265 taskset: CPU outside supported range\n");
    } while (*p >= '0' && *p <= '9');
    *at = p;
    return n;
}

static uint64_t list(const char *p) {
    uint64_t mask = 0;
    for (;;) {
        unsigned low = number(&p), high = low, step = 1;
        if (*p == '-') { ++p; high = number(&p); }
        if (*p == ':') { ++p; step = number(&p); }
        if (!step || high < low) fail("s265 taskset: invalid CPU range\n");
        for (unsigned i = low; i <= high; i += step) mask |= UINT64_C(1) << i;
        if (!*p) return mask;
        if (*p++ != ',') fail("s265 taskset: invalid CPU list\n");
    }
}

int main(int argc, char **argv) {
    int i = 1, cpu_list = 0, inline_list = 0;
    for (; i < argc && argv[i][0] == '-'; ++i) {
        const char *a = argv[i];
        if (!strcmp(a, "--pid") || !strcmp(a, "--help") || !strcmp(a, "--version")) pass(argv);
        if (!strcmp(a, "--")) { ++i; break; }
        if (!strncmp(a, "--cpu-list=", 11)) { cpu_list = inline_list = 1; break; }
        if (!strcmp(a, "--cpu-list")) { cpu_list = 1; continue; }
        if (!strcmp(a, "--all-tasks")) continue;
        if (a[1] == '-') pass(argv);
        if (strchr(a + 1, 'p')) pass(argv);
        for (const char *p = a + 1; *p; ++p) {
            if (*p == 'c') cpu_list = 1;
            else if (*p != 'a') pass(argv);
        }
    }
    if (i >= argc) pass(argv);
    const char *requested = argv[i] + (inline_list ? 11 : 0);
    uint64_t mask;
    if (cpu_list) mask = list(requested);
    else {
        char *end;
        errno = 0;
        mask = strtoull(requested, &end, 16);
        if (errno || !*requested || *end) pass(argv);
    }
    if (!mask) fail("s265 taskset: empty CPU set\n");
    if (!(mask & ~UINT64_C(0xf0))) pass(argv);              /* already 4-7 */
    if (mask & ~UINT64_C(0xf)) fail("s265 taskset: CPU set does not fit CPUs 4-7\n");
    uint64_t assigned = mask << 4;
    static char replacement[80];
    size_t used = 0;
    if (inline_list) used = (size_t)snprintf(replacement, sizeof replacement, "--cpu-list=");
    if (cpu_list) {
        int first = 1;
        for (unsigned cpu = 4; cpu < 8; ++cpu)
            if (assigned & (UINT64_C(1) << cpu)) {
                used += (size_t)snprintf(replacement + used, sizeof replacement - used,
                                         "%s%u", first ? "" : ",", cpu);
                first = 0;
            }
    } else {
        used += (size_t)snprintf(replacement, sizeof replacement, "%llx",
                                 (unsigned long long)assigned);
    }
    if (used >= sizeof replacement) fail("s265 taskset: replacement overflow\n");
    argv[i] = replacement;
    pass(argv);
    return 2;
}
