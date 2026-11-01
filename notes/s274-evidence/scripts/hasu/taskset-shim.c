/* s274: the disposition-neutral taskset shim (wolf-lang#925, hasu-env README),
 * copied from s271's (itself r41's) and given a block base: a CPU set that fits
 * the block's six CPUs passes unchanged; one that fits 0-5 (a test's own
 * `taskset -c 0-3`) is translated onto the block (cpu + BLOCK_BASE). Every
 * translation is logged. No shell, interpreter, signal calls or descriptor
 * repair; the only descriptor opened is the append log, closed before exec.
 * Build: cc -O2 -static -DBLOCK_BASE=<0|6> taskset-shim.c -o /usr/bin/taskset */
#define _POSIX_C_SOURCE 200809L
#include <errno.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#ifndef BLOCK_BASE
#error "BLOCK_BASE (0 for block A, 6 for block B)"
#endif

static const char real[] = "/usr/bin/taskset.real";
static const char logpath[] = "/lane/records/taskset-translations.log";
static const uint64_t block = UINT64_C(0x3f) << BLOCK_BASE;

static void fail(const char *message) {
    (void)!write(STDERR_FILENO, message, strlen(message));
    _exit(2);
}

static void pass(char **argv) {
    execv(real, argv);
    fail("s274 taskset: execv taskset.real failed\n");
}

static unsigned number(const char **at) {
    const char *p = *at;
    unsigned n = 0;
    if (*p < '0' || *p > '9') fail("s274 taskset: invalid CPU list\n");
    do {
        n = n * 10 + (unsigned)(*p++ - '0');
        if (n >= 64) fail("s274 taskset: CPU outside supported range\n");
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
        if (!step || high < low) fail("s274 taskset: invalid CPU range\n");
        for (unsigned i = low; i <= high; i += step) mask |= UINT64_C(1) << i;
        if (!*p) return mask;
        if (*p++ != ',') fail("s274 taskset: invalid CPU list\n");
    }
}

static void record(const char *requested, const char *assigned) {
    size_t size = strlen(requested) + strlen(assigned) + 100;
    char *line = malloc(size);
    if (!line) fail("s274 taskset: allocation failed\n");
    int length = snprintf(line, size,
        "{\"requested\":\"%s\",\"assigned\":\"%s\",\"pid\":%ld}\n",
        requested, assigned, (long)getpid());
    if (length < 0 || (size_t)length >= size) fail("s274 taskset: log formatting failed\n");
    int fd = open(logpath, O_WRONLY | O_CREAT | O_APPEND | O_CLOEXEC, 0644);
    if (fd < 0) fail("s274 taskset: cannot append translation log\n");
    size_t done = 0;
    while (done < (size_t)length) {
        ssize_t n = write(fd, line + done, (size_t)length - done);
        if (n < 0 && errno == EINTR) continue;
        if (n <= 0) { (void)close(fd); fail("s274 taskset: log write failed\n"); }
        done += (size_t)n;
    }
    if (close(fd)) fail("s274 taskset: log close failed\n");
    free(line);
}

/* The mask as a CPU list, for the rewritten argument. */
static char *render(uint64_t mask) {
    static char buf[256];
    size_t at = 0;
    for (unsigned i = 0; i < 64; ++i) {
        if (!(mask & (UINT64_C(1) << i))) continue;
        int n = snprintf(buf + at, sizeof buf - at, at ? ",%u" : "%u", i);
        if (n < 0 || (size_t)n >= sizeof buf - at) fail("s274 taskset: list too long\n");
        at += (size_t)n;
    }
    return buf;
}

int main(int argc, char **argv) {
    int i = 1, cpu_list = 0, inline_list = 0;
    /* Only inspect taskset's option prefix, never the target's options. */
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
    if (!mask) fail("s274 taskset: empty CPU set\n");
    if (!(mask & ~block)) { record(requested, requested); pass(argv); }
    if (!(mask & ~UINT64_C(0x3f))) {
        uint64_t moved = mask << BLOCK_BASE;
        char *assigned = render(moved);
        record(requested, assigned);
        if (inline_list) {
            static char arg[300];
            snprintf(arg, sizeof arg, "--cpu-list=%s", assigned);
            argv[i] = arg;
        } else if (cpu_list) {
            argv[i] = assigned;
        } else {
            static char hex[32];
            snprintf(hex, sizeof hex, "%llx", (unsigned long long)moved);
            argv[i] = hex;
        }
        pass(argv);
    }
    fail("s274 taskset: CPU set fits neither the block nor CPUs 0-5\n");
    return 2;
}
