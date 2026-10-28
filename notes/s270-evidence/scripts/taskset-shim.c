#define _POSIX_C_SOURCE 200809L
#include <errno.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

/* No shell, interpreter, signal calls, or descriptor repair. The only
 * descriptor opened here is the append log; close it before exec even
 * when open() selected a deliberately closed standard descriptor. */
static const char real[] = "/usr/bin/taskset.real";
static const char logpath[] = "/lane/records/taskset-translations.log";

static void fail(const char *message) {
    (void)!write(STDERR_FILENO, message, strlen(message));
    _exit(2);
}

static void pass(char **argv) {
    execv(real, argv);
    fail("r41 taskset: execv taskset.real failed\n");
}

static unsigned number(const char **at) {
    const char *p = *at;
    unsigned n = 0;
    if (*p < '0' || *p > '9') fail("r41 taskset: invalid CPU list\n");
    do {
        n = n * 10 + (unsigned)(*p++ - '0');
        if (n >= 64) fail("r41 taskset: CPU outside supported range\n");
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
        if (!step || high < low || (high - low) / step + 1 > 4)
            fail("r41 taskset: CPU set does not fit CPUs 0-5\n");
        for (unsigned i = low; i <= high; i += step) mask |= UINT64_C(1) << i;
        if (!*p) return mask;
        if (*p++ != ',') fail("r41 taskset: invalid CPU list\n");
    }
}

static void record(const char *requested, const char *assigned) {
    size_t size = strlen(requested) + strlen(assigned) + 100;
    char *line = malloc(size);
    if (!line) fail("r41 taskset: allocation failed\n");
    int length = snprintf(line, size,
        "{\"requested\":\"%s\",\"assigned\":\"%s\",\"pid\":%ld}\n",
        requested, assigned, (long)getpid());
    if (length < 0 || (size_t)length >= size) fail("r41 taskset: log formatting failed\n");
    int fd = open(logpath, O_WRONLY | O_CREAT | O_APPEND | O_CLOEXEC, 0644);
    if (fd < 0) fail("r41 taskset: cannot append translation log\n");
    size_t done = 0;
    while (done < (size_t)length) {
        ssize_t n = write(fd, line + done, (size_t)length - done);
        if (n < 0 && errno == EINTR) continue;
        if (n <= 0) { (void)close(fd); fail("r41 taskset: log write failed\n"); }
        done += (size_t)n;
    }
    if (close(fd)) fail("r41 taskset: log close failed\n");
    free(line);
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
    if (!mask) fail("r41 taskset: empty CPU set\n");
    if (!(mask & ~UINT64_C(0x3f))) { record(requested, requested); pass(argv); }
    fail("r41 taskset: CPU set does not fit CPUs 0-5\n");
    return 2;
}
