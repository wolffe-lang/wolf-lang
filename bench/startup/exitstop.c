/* The start-up floor rows' reader (bench-gates, `bench/startup.json`): runs a
 * program and reads it at its exit stop, when its address space is still whole.
 *
 *   exitstop mem PROG
 *       prints one line: "vmhwm_kb=N minflt=N text_kb=N exit=E", the program's
 *       peak resident set (VmHWM), its minor faults, the resident kB of its own
 *       executable mappings (the program file's r-x lines in smaps), and its exit
 *       status (or 128 plus the signal that ended it).
 *   exitstop altstack ADDR SIZE PROG
 *       ADDR is the link-time address (hex) of a SIZE-byte object in PROG, the
 *       main thread's alternate signal stack. At the exec stop the object is
 *       filled with 0xa5; at the exit stop the lowest byte that is no longer
 *       0xa5 gives how much of it the program used. Prints one line:
 *       "altstack used=N size=SIZE exit=E" (E is the exit status, or 128 plus
 *       the signal that ended it).
 *
 * The program runs in an empty environment with stdin and stdout on /dev/null
 * and stderr inherited, with address randomisation off
 * (personality(ADDR_NO_RANDOMIZE), read back), traced by this process
 * (PTRACE_TRACEME) and stopped at its exit (PTRACE_O_TRACEEXIT).
 *
 * Exit 0 with the line. Exit 3 with "exitstop: refused: CALL: ERROR" on stderr
 * when this host refuses address randomisation off, the trace, or the exit
 * stop: the rows cannot read without them, and bench-gates fails by name (a
 * container's default seccomp filter refuses the first). Exit 4 on any other
 * failure. */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/personality.h>
#include <sys/ptrace.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>

#define PATTERN 0xa5

static void die(int code, const char *what) {
    fprintf(stderr, "exitstop: %s: %s\n", what, strerror(errno));
    exit(code);
}

/* The child's report of a refused step, through a close-on-exec pipe. */
struct refusal {
    int step; /* 1 personality, 2 the read-back, 3 PTRACE_TRACEME, 4 execve */
    int err;
};

static const char *step_name(int step) {
    switch (step) {
    case 1: return "personality(ADDR_NO_RANDOMIZE)";
    case 2: return "personality(ADDR_NO_RANDOMIZE) read back without the flag";
    case 3: return "ptrace(PTRACE_TRACEME)";
    default: return "execve";
    }
}

static pid_t start(char *prog, int *pipe_rd) {
    int p[2];
    if (pipe2(p, O_CLOEXEC) != 0) die(4, "pipe2");
    pid_t pid = fork();
    if (pid < 0) die(4, "fork");
    if (pid == 0) {
        struct refusal r = {0, 0};
        int nul = open("/dev/null", O_RDWR);
        if (nul >= 0) {
            dup2(nul, 0);
            dup2(nul, 1);
        }
        if (personality(ADDR_NO_RANDOMIZE) == -1) {
            r.step = 1;
        } else {
            int now = personality(0xffffffff);
            if (now == -1 || !(now & ADDR_NO_RANDOMIZE)) {
                r.step = 2;
            } else if (ptrace(PTRACE_TRACEME, 0, NULL, NULL) != 0) {
                r.step = 3;
            } else {
                char *argv[] = {prog, NULL};
                char *envp[] = {NULL};
                execve(prog, argv, envp);
                r.step = 4;
            }
        }
        r.err = errno;
        ssize_t w = write(p[1], &r, sizeof r);
        (void)w;
        _exit(127);
    }
    close(p[1]);
    *pipe_rd = p[0];
    return pid;
}

/* After start(): EOF on the pipe means the exec happened. */
static void refused_or_execed(pid_t pid, int rd) {
    struct refusal r;
    ssize_t n = read(rd, &r, sizeof r);
    close(rd);
    if (n == (ssize_t)sizeof r) {
        int status;
        waitpid(pid, &status, 0);
        errno = r.err;
        if (r.step == 4) die(4, "execve");
        fprintf(stderr, "exitstop: refused: %s: %s\n", step_name(r.step),
                r.step == 2 ? "the persona does not carry it" : strerror(r.err));
        exit(3);
    }
}

/* Wait for the next stop or the end; deliver signals the program receives. */
static int wait_exit_stop(pid_t pid) {
    for (;;) {
        int status;
        if (waitpid(pid, &status, 0) < 0) die(4, "waitpid");
        if (WIFEXITED(status) || WIFSIGNALED(status)) return -1;
        if (!WIFSTOPPED(status)) continue;
        if (status >> 8 == (SIGTRAP | (PTRACE_EVENT_EXIT << 8))) return 0;
        int sig = WSTOPSIG(status);
        if (sig == SIGTRAP) sig = 0;
        if (ptrace(PTRACE_CONT, pid, NULL, (void *)(intptr_t)sig) != 0) die(4, "PTRACE_CONT");
    }
}

static int finish(pid_t pid) {
    int status;
    if (ptrace(PTRACE_CONT, pid, NULL, NULL) != 0) die(4, "PTRACE_CONT at the exit stop");
    if (waitpid(pid, &status, 0) < 0) die(4, "waitpid");
    if (WIFEXITED(status)) return WEXITSTATUS(status);
    if (WIFSIGNALED(status)) return 128 + WTERMSIG(status);
    return -1;
}

/* The exec stop, then the exit stop asked for. */
static void trace_from_exec(pid_t pid) {
    int status;
    if (waitpid(pid, &status, 0) < 0) die(4, "waitpid");
    if (!WIFSTOPPED(status) || WSTOPSIG(status) != SIGTRAP) {
        fprintf(stderr, "exitstop: the program did not stop at its exec\n");
        exit(4);
    }
    if (ptrace(PTRACE_SETOPTIONS, pid, NULL, (void *)(intptr_t)(PTRACE_O_TRACEEXIT | PTRACE_O_EXITKILL)) != 0) {
        fprintf(stderr, "exitstop: refused: ptrace(PTRACE_SETOPTIONS, PTRACE_O_TRACEEXIT): %s\n", strerror(errno));
        exit(3);
    }
}

static FILE *proc(pid_t pid, const char *what) {
    char path[64];
    snprintf(path, sizeof path, "/proc/%d/%s", (int)pid, what);
    FILE *f = fopen(path, "r");
    if (!f) die(4, path);
    return f;
}

/* The resident kB of PROG's own r-x mappings, from smaps. */
static long text_kb(pid_t pid, const char *exe) {
    FILE *f = proc(pid, "smaps");
    char line[4096];
    int ours = 0;
    long kb = 0;
    while (fgets(line, sizeof line, f)) {
        unsigned long lo, hi;
        char perms[8];
        int consumed = 0;
        if (sscanf(line, "%lx-%lx %7s %*s %*s %*s%n", &lo, &hi, perms, &consumed) == 3) {
            char *path = line + consumed;
            while (*path == ' ') path++;
            path[strcspn(path, "\n")] = 0;
            ours = strchr(perms, 'x') && strcmp(path, exe) == 0;
            continue;
        }
        long v;
        if (ours && sscanf(line, "Rss: %ld kB", &v) == 1) kb += v;
    }
    fclose(f);
    return kb;
}

static int mode_mem(char *prog) {
    char exe[4096];
    if (!realpath(prog, exe)) die(4, prog);
    int rd;
    pid_t pid = start(prog, &rd);
    refused_or_execed(pid, rd);
    trace_from_exec(pid);
    if (ptrace(PTRACE_CONT, pid, NULL, NULL) != 0) die(4, "PTRACE_CONT");
    if (wait_exit_stop(pid) != 0) {
        fprintf(stderr, "exitstop: the program ended without an exit stop\n");
        return 4;
    }
    long hwm = -1, minflt = -1;
    char line[4096];
    FILE *s = proc(pid, "status");
    while (fgets(line, sizeof line, s)) sscanf(line, "VmHWM: %ld kB", &hwm);
    fclose(s);
    FILE *st = proc(pid, "stat");
    if (fgets(line, sizeof line, st)) {
        /* field 10 (minflt) counts from after the ")" that ends field 2 */
        char *p = strrchr(line, ')');
        if (p) sscanf(p + 2, "%*c %*d %*d %*d %*d %*d %*u %ld", &minflt);
    }
    fclose(st);
    long text = text_kb(pid, exe);
    int code = finish(pid);
    if (hwm < 0 || minflt < 0) {
        fprintf(stderr, "exitstop: VmHWM or minflt unreadable\n");
        return 4;
    }
    printf("vmhwm_kb=%ld minflt=%ld text_kb=%ld exit=%d\n", hwm, minflt, text, code);
    return 0;
}

/* The load base of PROG (its first mapping at file offset 0) and whether it is
 * position-independent (ELF e_type ET_DYN), so a link-time address can be placed. */
static unsigned long runtime_address(pid_t pid, const char *exe, unsigned long addr) {
    unsigned char hdr[18];
    int fd = open(exe, O_RDONLY | O_CLOEXEC);
    if (fd < 0 || read(fd, hdr, sizeof hdr) != (ssize_t)sizeof hdr) die(4, exe);
    close(fd);
    int pie = (hdr[16] | (hdr[17] << 8)) == 3; /* ET_DYN */
    if (!pie) return addr;
    FILE *f = proc(pid, "maps");
    char line[4096];
    unsigned long base = 0;
    while (fgets(line, sizeof line, f)) {
        unsigned long lo, hi, off;
        int consumed = 0;
        if (sscanf(line, "%lx-%lx %*s %lx %*s %*s%n", &lo, &hi, &off, &consumed) != 3) continue;
        char *path = line + consumed;
        while (*path == ' ') path++;
        path[strcspn(path, "\n")] = 0;
        if (off == 0 && strcmp(path, exe) == 0) {
            base = lo;
            break;
        }
    }
    fclose(f);
    if (!base) {
        fprintf(stderr, "exitstop: no mapping of %s at offset 0\n", exe);
        exit(4);
    }
    return base + addr;
}

static int mode_altstack(const char *addr_hex, const char *size_s, char *prog) {
    char exe[4096];
    if (!realpath(prog, exe)) die(4, prog);
    unsigned long link = strtoul(addr_hex, NULL, 16);
    size_t size = strtoul(size_s, NULL, 10);
    if (!link || !size || size > (1u << 24)) {
        fprintf(stderr, "exitstop: a bad address or size\n");
        return 4;
    }
    int rd;
    pid_t pid = start(prog, &rd);
    refused_or_execed(pid, rd);
    trace_from_exec(pid);
    unsigned long at = runtime_address(pid, exe, link);
    unsigned char *buf = malloc(size);
    if (!buf) die(4, "malloc");
    memset(buf, PATTERN, size);
    char mem[64];
    snprintf(mem, sizeof mem, "/proc/%d/mem", (int)pid);
    int fd = open(mem, O_RDWR | O_CLOEXEC);
    if (fd < 0) die(4, mem);
    if (pwrite(fd, buf, size, (off_t)at) != (ssize_t)size) die(4, "writing the pattern");
    if (ptrace(PTRACE_CONT, pid, NULL, NULL) != 0) die(4, "PTRACE_CONT");
    if (wait_exit_stop(pid) != 0) {
        fprintf(stderr, "exitstop: the program ended without an exit stop\n");
        return 4;
    }
    if (pread(fd, buf, size, (off_t)at) != (ssize_t)size) die(4, "reading the pattern back");
    close(fd);
    size_t lowest = size;
    for (size_t i = 0; i < size; i++) {
        if (buf[i] != PATTERN) {
            lowest = i;
            break;
        }
    }
    int code = finish(pid);
    printf("altstack used=%zu size=%zu exit=%d\n", size - lowest, size, code);
    return 0;
}

int main(int argc, char **argv) {
    if (argc == 3 && strcmp(argv[1], "mem") == 0) return mode_mem(argv[2]);
    if (argc == 5 && strcmp(argv[1], "altstack") == 0) return mode_altstack(argv[2], argv[3], argv[4]);
    fprintf(stderr, "usage: exitstop mem PROG | exitstop altstack ADDR SIZE PROG\n");
    return 4;
}
