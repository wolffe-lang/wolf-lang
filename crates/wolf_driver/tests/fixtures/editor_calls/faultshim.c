/*
 * s248 (wolf-lang#796, `[os.fs.sync]`): the save witness's fault shim,
 * written for this gate (yew's torture campaign is the oracle's shape:
 * end the program at each storage boundary in turn and read what is
 * left under the name). Loaded into ONE program the test started —
 * LD_PRELOAD on linux, DYLD_INSERT_LIBRARIES with dyld's interposing on
 * macOS — it watches the calls on the save's own files and nothing else:
 *
 *   doc.txt.s248tmp  the temporary file   ("tmp")
 *   doc.txt          the target           ("target")
 *   "." or "<d>/."   the directory        ("dir")
 *
 * Each watched call is one BOUNDARY: an open of a watched path, a write,
 * a duplicate, a sync (fsync, fdatasync, F_FULLFSYNC), a close of a
 * watched descriptor, a rename naming a watched path. With S248_FAULT_AT
 * = N the shim ends the process with _exit(77) BEFORE the N-th boundary
 * runs (a process ending, not a signal). S248_FAULT_LOG names the file
 * each boundary is logged to, one line each:
 *
 *   <n> open <label> excl=<0|1>
 *   <n> write <label> <bytes>
 *   <n> dup <label>
 *   <n> sync <label> <fsync|fdatasync|fullfsync>
 *   <n> close <label>
 *   <n> rename <from-label> <to-label>
 *   <n> fault
 */
#define _GNU_SOURCE 1
#if defined(__APPLE__)
#define _DARWIN_C_SOURCE 1
#endif

#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <unistd.h>

#define MAXFD 4096

static char labels[MAXFD]; /* 0 unwatched, 't' tmp, 'g' target, 'd' dir */
static long boundary;
static long fault_at = -1;
static int log_fd = -1;
static int ready;

static const char *name_of(char l) {
    switch (l) {
    case 't': return "tmp";
    case 'g': return "target";
    case 'd': return "dir";
    default: return "other";
    }
}

static char label_of_path(const char *p) {
    if (p == NULL) return 0;
    size_t n = strlen(p);
    const char *tmp = "doc.txt.s248tmp";
    const char *tgt = "doc.txt";
    size_t nt = strlen(tmp), ng = strlen(tgt);
    if (n >= nt && strcmp(p + n - nt, tmp) == 0 && (n == nt || p[n - nt - 1] == '/')) return 't';
    if (n >= ng && strcmp(p + n - ng, tgt) == 0 && (n == ng || p[n - ng - 1] == '/')) return 'g';
    if (strcmp(p, ".") == 0 || (n >= 2 && strcmp(p + n - 2, "/.") == 0)) return 'd';
    return 0;
}

#if defined(__APPLE__)
/* dyld interposing: calls from this library reach the real functions. */
#define REAL(f) f
#else
static void *real(const char *name) {
    void *f = dlsym(RTLD_NEXT, name);
    if (f == NULL) _exit(98);
    return f;
}
#define REAL(f) ((__typeof__(&f))real(#f))
#endif

static void init(void) {
    if (ready) return;
    ready = 1;
    const char *at = getenv("S248_FAULT_AT");
    if (at != NULL && *at) fault_at = strtol(at, NULL, 10);
    const char *log = getenv("S248_FAULT_LOG");
    if (log != NULL && *log) {
#if defined(__APPLE__)
        log_fd = open(log, O_WRONLY | O_CREAT | O_APPEND | O_CLOEXEC, 0644);
#else
        log_fd = ((int (*)(const char *, int, ...))real("open"))(
            log, O_WRONLY | O_CREAT | O_APPEND | O_CLOEXEC, 0644);
#endif
    }
}

static void say(const char *line) {
    if (log_fd < 0) return;
#if defined(__APPLE__)
    (void)write(log_fd, line, strlen(line));
#else
    ((ssize_t (*)(int, const void *, size_t))real("write"))(log_fd, line, strlen(line));
#endif
}

/* One boundary: log it, and end the process before it runs if it is the
 * one asked for. */
static void at_boundary(const char *what) {
    init();
    char buf[256];
    boundary++;
    if (fault_at > 0 && boundary == fault_at) {
        snprintf(buf, sizeof buf, "%ld fault %s\n", boundary, what);
        say(buf);
        _exit(77);
    }
    snprintf(buf, sizeof buf, "%ld %s\n", boundary, what);
    say(buf);
}

static int watched(int fd) { return fd >= 0 && fd < MAXFD && labels[fd] != 0; }

static void opened(int fd, char l) {
    if (fd >= 0 && fd < MAXFD) labels[fd] = l;
}

static int s248_open_common(int fd_or_err, char l) {
    if (fd_or_err >= 0) opened(fd_or_err, l);
    return fd_or_err;
}

static void open_boundary(const char *path, int flags, char *l) {
    *l = label_of_path(path);
    if (*l) {
        char what[96];
        snprintf(what, sizeof what, "open %s excl=%d", name_of(*l), (flags & O_EXCL) ? 1 : 0);
        at_boundary(what);
    }
}

#if defined(__APPLE__)
#define DECL(ret, name, ...) static ret s248_##name(__VA_ARGS__)
#else
#define DECL(ret, name, ...) ret name(__VA_ARGS__)
#endif

DECL(int, open, const char *path, int flags, ...) {
    mode_t mode = 0;
    if (flags & O_CREAT) {
        va_list ap;
        va_start(ap, flags);
        mode = (mode_t)va_arg(ap, int);
        va_end(ap);
    }
    char l;
    open_boundary(path, flags, &l);
    return s248_open_common(REAL(open)(path, flags, mode), l);
}

DECL(int, openat, int dirfd, const char *path, int flags, ...) {
    mode_t mode = 0;
    if (flags & O_CREAT) {
        va_list ap;
        va_start(ap, flags);
        mode = (mode_t)va_arg(ap, int);
        va_end(ap);
    }
    char l;
    open_boundary(path, flags, &l);
    return s248_open_common(REAL(openat)(dirfd, path, flags, mode), l);
}

#if !defined(__APPLE__)
int open64(const char *path, int flags, ...) {
    mode_t mode = 0;
    if (flags & O_CREAT) {
        va_list ap;
        va_start(ap, flags);
        mode = (mode_t)va_arg(ap, int);
        va_end(ap);
    }
    char l;
    open_boundary(path, flags, &l);
    return s248_open_common(REAL(open64)(path, flags, mode), l);
}

int openat64(int dirfd, const char *path, int flags, ...) {
    mode_t mode = 0;
    if (flags & O_CREAT) {
        va_list ap;
        va_start(ap, flags);
        mode = (mode_t)va_arg(ap, int);
        va_end(ap);
    }
    char l;
    open_boundary(path, flags, &l);
    return s248_open_common(REAL(openat64)(dirfd, path, flags, mode), l);
}
#endif

DECL(ssize_t, write, int fd, const void *buf, size_t n) {
    if (watched(fd)) {
        char what[96];
        snprintf(what, sizeof what, "write %s %zu", name_of(labels[fd]), n);
        at_boundary(what);
    }
    return REAL(write)(fd, buf, n);
}

DECL(int, fsync, int fd) {
    if (watched(fd)) {
        char what[96];
        snprintf(what, sizeof what, "sync %s fsync", name_of(labels[fd]));
        at_boundary(what);
    }
    return REAL(fsync)(fd);
}

#if !defined(__APPLE__)
int fdatasync(int fd) {
    if (watched(fd)) {
        char what[96];
        snprintf(what, sizeof what, "sync %s fdatasync", name_of(labels[fd]));
        at_boundary(what);
    }
    return REAL(fdatasync)(fd);
}
#endif

static int fcntl_common(int fd, int cmd, void *arg, int (*f)(int, int, ...)) {
    char l = watched(fd) ? labels[fd] : 0;
    if (l && (cmd == F_DUPFD || cmd == F_DUPFD_CLOEXEC)) {
        char what[96];
        snprintf(what, sizeof what, "dup %s", name_of(l));
        at_boundary(what);
        int r = f(fd, cmd, arg);
        if (r >= 0) opened(r, l);
        return r;
    }
#if defined(__APPLE__)
    if (l && cmd == F_FULLFSYNC) {
        char what[96];
        snprintf(what, sizeof what, "sync %s fullfsync", name_of(l));
        at_boundary(what);
    }
#endif
    return f(fd, cmd, arg);
}

#if defined(__APPLE__)
static int s248_fcntl(int fd, int cmd, ...) {
    va_list ap;
    va_start(ap, cmd);
    void *arg = va_arg(ap, void *);
    va_end(ap);
    return fcntl_common(fd, cmd, arg, fcntl);
}
#else
int fcntl(int fd, int cmd, ...) {
    va_list ap;
    va_start(ap, cmd);
    void *arg = va_arg(ap, void *);
    va_end(ap);
    return fcntl_common(fd, cmd, arg, (int (*)(int, int, ...))real("fcntl"));
}

int fcntl64(int fd, int cmd, ...) {
    va_list ap;
    va_start(ap, cmd);
    void *arg = va_arg(ap, void *);
    va_end(ap);
    return fcntl_common(fd, cmd, arg, (int (*)(int, int, ...))real("fcntl64"));
}
#endif

DECL(int, dup, int fd) {
    char l = watched(fd) ? labels[fd] : 0;
    if (l) {
        char what[96];
        snprintf(what, sizeof what, "dup %s", name_of(l));
        at_boundary(what);
    }
    int r = REAL(dup)(fd);
    if (l && r >= 0) opened(r, l);
    return r;
}

DECL(int, close, int fd) {
    char l = watched(fd) ? labels[fd] : 0;
    if (l) {
        char what[96];
        snprintf(what, sizeof what, "close %s", name_of(l));
        at_boundary(what);
        labels[fd] = 0;
    }
    return REAL(close)(fd);
}

DECL(int, rename, const char *from, const char *to) {
    char a = label_of_path(from), b = label_of_path(to);
    if (a || b) {
        char what[96];
        snprintf(what, sizeof what, "rename %s %s", name_of(a), name_of(b));
        at_boundary(what);
    }
    return REAL(rename)(from, to);
}

#if !defined(__APPLE__)
int renameat(int fd1, const char *from, int fd2, const char *to) {
    char a = label_of_path(from), b = label_of_path(to);
    if (a || b) {
        char what[96];
        snprintf(what, sizeof what, "rename %s %s", name_of(a), name_of(b));
        at_boundary(what);
    }
    return REAL(renameat)(fd1, from, fd2, to);
}
#endif

#if defined(__APPLE__)
#define INTERPOSE(r, f)                                                          \
    __attribute__((used)) static const struct {                                \
        const void *replacement;                                               \
        const void *replacee;                                                  \
    } s248_interpose_##f __attribute__((section("__DATA,__interpose"))) = {     \
        (const void *)(unsigned long)&r, (const void *)(unsigned long)&f};
INTERPOSE(s248_open, open)
INTERPOSE(s248_openat, openat)
INTERPOSE(s248_write, write)
INTERPOSE(s248_fsync, fsync)
INTERPOSE(s248_fcntl, fcntl)
INTERPOSE(s248_dup, dup)
INTERPOSE(s248_close, close)
INTERPOSE(s248_rename, rename)
#endif
