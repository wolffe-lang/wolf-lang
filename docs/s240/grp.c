/* s240 probe: what kill(2) does with 0, -1 and a negative group id.
 *
 * Written from POSIX.1-2024 and the kill(2), setpgid(2), setsid(2) man
 * pages; no C library's or kernel's source was read. Every process this
 * probe signals is one it started: the probe first makes a session of
 * its own (setsid), and kill(-1) runs only when PROBE_ALLOW_ALL=1, which
 * the linux driver sets inside a fresh container (its own pid
 * namespace, where every process is the probe's).
 *
 * Each process installs a handler for SIGUSR1 and SIGCONT that writes a
 * record (its tag and the signal) to one shared pipe; after a send the
 * runner waits, drains the pipe, and prints who heard. Output lines are
 * `case: ret=<0|errno name> heard=<tags>`.
 *
 * Usage: grp [user-uid]   (run as root; user-uid defaults to 1000, or
 * SUDO_UID when set). Without root the permission cases are skipped by
 * name.
 */
#define _GNU_SOURCE
#include <errno.h>
#include <dirent.h>
#include <fcntl.h>
#include <limits.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

static int rep[2];            /* the report pipe */
static char tag[16] = "?";    /* this process's name in a record */

static void heard(int sig) {
    char b[24];
    int n = 0;
    for (const char *p = tag; *p && n < 15; p++) b[n++] = *p;
    b[n++] = sig == SIGCONT ? '+' : ':';
    b[n++] = '\n';
    (void)!write(rep[1], b, (size_t)n);
}

static void listen_both(void) {
    struct sigaction sa;
    memset(&sa, 0, sizeof sa);
    sa.sa_handler = heard;
    sa.sa_flags = SA_RESTART; /* a runner blocked in read() must not see EINTR */
    sigemptyset(&sa.sa_mask);
    sigaction(SIGUSR1, &sa, 0);
    sigaction(SIGCONT, &sa, 0);
}

static void nap(int ms) {
    struct timespec ts = {ms / 1000, (long)(ms % 1000) * 1000000L};
    while (nanosleep(&ts, &ts) < 0 && errno == EINTR) {}
}

static const char *ename(int e) {
    switch (e) {
    case 0: return "0";
    case EPERM: return "EPERM";
    case ESRCH: return "ESRCH";
    case EINVAL: return "EINVAL";
    case EACCES: return "EACCES";
    default: { static char b[32]; snprintf(b, sizeof b, "errno%d", e); return b; }
    }
}

/* Drain the report pipe after a pause; return the sorted, de-duplicated
 * list of records. */
static void drain(char *out, size_t cap) {
    nap(250);
    char buf[4096];
    size_t len = 0;
    for (;;) {
        ssize_t r = read(rep[0], buf + len, sizeof buf - 1 - len);
        if (r <= 0) break;
        len += (size_t)r;
        if (len >= sizeof buf - 1) break;
    }
    buf[len] = 0;
    /* split lines, sort (insertion sort, small) */
    char *lines[128];
    int n = 0;
    for (char *s = strtok(buf, "\n"); s && n < 128; s = strtok(0, "\n")) lines[n++] = s;
    for (int i = 1; i < n; i++)
        for (int j = i; j > 0 && strcmp(lines[j - 1], lines[j]) > 0; j--) {
            char *t = lines[j]; lines[j] = lines[j - 1]; lines[j - 1] = t;
        }
    out[0] = 0;
    for (int i = 0; i < n; i++) {
        if (i && !strcmp(lines[i], lines[i - 1])) continue;
        if (out[0]) strncat(out, ",", cap - strlen(out) - 1);
        strncat(out, lines[i], cap - strlen(out) - 1);
    }
    if (!out[0]) strncpy(out, "-", cap);
}

/* A target: a child that joins group `pg` (0: a new group; -1: stay;
 * -2: a new session), drops to `uid` (-1: keep), listens, and pauses. */
static pid_t target(const char *name, pid_t pg, int uid) {
    int sync[2];
    if (pipe(sync) < 0) { perror("pipe"); exit(2); }
    pid_t p = fork();
    if (p < 0) { perror("fork"); exit(2); }
    if (p == 0) {
        close(sync[0]);
        snprintf(tag, sizeof tag, "%s", name);
        listen_both();
        if (pg == -2) setsid();
        else if (pg >= 0) setpgid(0, pg);
        if (uid >= 0 && setuid((uid_t)uid) < 0) { perror("setuid"); _exit(3); }
        (void)!write(sync[1], "k", 1);
        close(sync[1]);
        for (;;) pause();
    }
    close(sync[1]);
    if (pg >= 0) setpgid(p, pg == 0 ? p : pg); /* the parent's half */
    char c;
    (void)!read(sync[0], &c, 1);
    close(sync[0]);
    return p;
}

/* Every process in sight with its real uid and state (linux /proc), so a
 * kill(-1) row can say who was there to reach. */
static void census(const char *why) {
    DIR *d = opendir("/proc");
    if (!d) { printf("census (%s): no /proc\n", why); return; }
    printf("census (%s):", why);
    struct dirent *e;
    while ((e = readdir(d))) {
        if (e->d_name[0] < '0' || e->d_name[0] > '9') continue;
        char path[300], l[256];
        snprintf(path, sizeof path, "/proc/%s/status", e->d_name);
        FILE *f = fopen(path, "r");
        if (!f) continue;
        int uid = -1; char st = '?';
        while (fgets(l, sizeof l, f)) {
            if (!strncmp(l, "Uid:", 4)) uid = atoi(l + 4);
            if (!strncmp(l, "State:", 6)) { char *q = l + 6; while (*q == ' ' || *q == '\t') q++; st = *q; }
        }
        fclose(f);
        printf(" %s/uid%d/%c", e->d_name, uid, st);
    }
    closedir(d);
    printf("\n");
}

static void reap(pid_t p) {
    kill(p, SIGKILL);
    waitpid(p, 0, 0);
}

/* A sender: forks, joins `pg` (as target()), drops to `uid`, calls
 * kill(who, sig), and reports the return over a pipe; it listens too, so
 * a send that reaches the sender shows as its tag. */
static int send_from(const char *name, pid_t pg, int uid, pid_t who, int sig) {
    int res[2];
    if (pipe(res) < 0) { perror("pipe"); exit(2); }
    pid_t p = fork();
    if (p == 0) {
        close(res[0]);
        snprintf(tag, sizeof tag, "%s", name);
        listen_both();
        if (pg == -2) setsid();
        else if (pg >= 0) setpgid(0, pg);
        if (uid >= 0 && setuid((uid_t)uid) < 0) { perror("setuid"); _exit(3); }
        int r = kill(who, sig);
        int e = r < 0 ? errno : 0;
        nap(150); /* stay alive long enough to hear our own send */
        (void)!write(res[1], &e, sizeof e);
        _exit(0);
    }
    close(res[1]);
    int e = -1;
    (void)!read(res[0], &e, sizeof e);
    close(res[0]);
    waitpid(p, 0, 0);
    return e;
}

static void line(const char *name, int e) {
    char h[2048];
    drain(h, sizeof h);
    printf("%s: ret=%s heard=%s\n", name, ename(e), h);
    fflush(stdout);
}

int main(int argc, char **argv) {
    setvbuf(stdout, 0, _IOLBF, 0);
    int uid = 1000;
    if (getenv("SUDO_UID")) uid = atoi(getenv("SUDO_UID"));
    if (argc > 1) uid = atoi(argv[1]);
    int root = geteuid() == 0;
    int allow_all = getenv("PROBE_ALLOW_ALL") && !strcmp(getenv("PROBE_ALLOW_ALL"), "1");

    if (pipe(rep) < 0) { perror("pipe"); return 2; }
    fcntl(rep[0], F_SETFL, O_NONBLOCK);
    snprintf(tag, sizeof tag, "pid1");
    listen_both(); /* the probe's top process: pid 1 in a container */

    /* The runner: a session of its own, so kill(0) can only reach the
     * probe's processes. */
    pid_t runner = fork();
    if (runner > 0) {
        int st;
        waitpid(runner, &st, 0);
        nap(300);
        char h[512];
        drain(h, sizeof h);
        printf("top: after the runner, the top process heard=%s\n", h);
        return WIFEXITED(st) ? WEXITSTATUS(st) : 1;
    }
    snprintf(tag, sizeof tag, "R");
    listen_both();
    if (setsid() < 0) { perror("setsid"); return 2; }
    printf("host: pid=%d pgid=%d sid=%d euid=%d user-uid=%d root=%d allow_all=%d\n",
           (int)getpid(), (int)getpgrp(), (int)getsid(0), (int)geteuid(), uid, root, allow_all);
    {
        FILE *f = fopen("/proc/self/status", "r");
        char l[256];
        while (f && fgets(l, sizeof l, f))
            if (!strncmp(l, "SigBlk", 6) || !strncmp(l, "SigIgn", 6) || !strncmp(l, "Cpus_allowed_list", 17))
                printf("status: %s", l);
        if (f) fclose(f);
    }
    char h[512];

    /* 1. kill(0): the caller's own group, caller included. */
    {
        pid_t a = target("A", 0, -1);          /* group A */
        pid_t b = target("B", a, -1);          /* joins A */
        pid_t c = target("C", 0, -1);          /* another group */
        int e = send_from("S", a, -1, 0, SIGUSR1);
        line("kill(0) from a member of A={A,B}, C apart", e);
        reap(a); reap(b); reap(c);
    }
    /* 2. kill(-G) from outside G. */
    {
        pid_t a = target("A", 0, -1);
        pid_t b = target("B", a, -1);
        pid_t c = target("C", 0, -1);
        int e = send_from("S", -1, -1, -a, SIGUSR1);
        line("kill(-A) from outside A={A,B}, C apart", e);
        e = send_from("S", -1, -1, -a, 0);
        line("kill(-A, 0) existence", e);
        reap(a); reap(b); reap(c);
        nap(50);
        e = send_from("S", -1, -1, -a, 0);
        line("kill(-A, 0) after every member is reaped", e);
        e = send_from("S", -1, -1, -a, SIGUSR1);
        line("kill(-A) after every member is reaped", e);
    }
    /* 3. A group whose leader has exited (and been reaped). */
    {
        pid_t a = target("A", 0, -1);
        pid_t b = target("B", a, -1);
        reap(a);
        int e = send_from("S", -1, -1, -a, SIGUSR1);
        line("kill(-A) with the leader reaped, B alive", e);
        reap(b);
    }
    /* 4. A group whose members are all zombies. */
    {
        pid_t a = target("A", 0, -1);
        pid_t b = target("B", a, -1);
        kill(a, SIGKILL); kill(b, SIGKILL);
        nap(100);
        int e = send_from("S", -1, -1, -a, 0);
        line("kill(-A, 0) with every member a zombie", e);
        e = send_from("S", -1, -1, -a, SIGUSR1);
        line("kill(-A) with every member a zombie", e);
        waitpid(a, 0, 0); waitpid(b, 0, 0);
    }
    /* 5. Edges of the id. */
    {
        int e = send_from("S", -1, -1, INT_MIN, 0);
        line("kill(INT_MIN, 0)", e);
        e = send_from("S", -1, -1, -INT_MAX, 0);
        line("kill(-INT_MAX, 0)", e);
        e = send_from("S", -1, -1, -1 * (pid_t)getpid(), 0);
        line("kill(-runner's group, 0) from a member of that group", e);
        drain(h, sizeof h);
    }
    /* 6. Permission over a group (needs root to make a second uid). */
    if (!root) {
        printf("perm: skipped by name (not root)\n");
    } else {
        pid_t a = target("Aroot", 0, -1);
        pid_t b = target("Buser", a, uid);
        int e = send_from("Suser", -1, uid, -a, SIGUSR1);
        line("as the user, kill(-G) G={root member, user member}", e);
        reap(a); reap(b);

        a = target("Aroot", 0, -1);
        e = send_from("Suser", -1, uid, -a, SIGUSR1);
        line("as the user, kill(-G) G={root member} only", e);
        e = send_from("Suser", -1, uid, -a, 0);
        line("as the user, kill(-G, 0) G={root member} only", e);
        e = send_from("Suser", -1, uid, -a, SIGCONT);
        line("as the user, kill(-G, SIGCONT) G={root member}, same session", e);
        reap(a);

        a = target("Xroot", -2, -1); /* a root process in another session */
        e = send_from("Suser", -1, uid, -a, SIGCONT);
        line("as the user, kill(-G, SIGCONT) G={root member}, another session", e);
        e = send_from("Suser", -1, uid, a, SIGCONT);
        line("as the user, kill(pid, SIGCONT) root, another session", e);
        reap(a);

        a = target("Auser", 0, uid);
        e = send_from("Sroot", -1, -1, -a, SIGUSR1);
        line("as root, kill(-G) G={user member}", e);
        reap(a);
    }
    /* 7. kill(-1): only where every process in sight is the probe's. */
    if (!allow_all) {
        printf("all: skipped by name (PROBE_ALLOW_ALL is not 1)\n");
    } else {
        pid_t a = target("A", 0, -1);
        pid_t b = target("B", -2, -1);
        pid_t u = root ? target("Uuser", -2, uid) : 0;
        int e = send_from(root ? "Sroot" : "S", 0, -1, -1, SIGUSR1);
        line(root ? "as root, kill(-1)" : "as the caller, kill(-1)", e);
        if (root) {
            e = send_from("Suser", 0, uid, -1, SIGUSR1);
            line("as the user, kill(-1); one other user process (another session)", e);
            reap(u);
            census("before the user's kill(-1) with no other user process; the sender is forked after this");
            e = send_from("Suser", 0, uid, -1, SIGUSR1);
            line("as the user, kill(-1); no other user process", e);
            e = send_from("Suser", 0, uid, -1, 0);
            line("as the user, kill(-1, 0); no other user process", e);
        }
        reap(a); reap(b);
    }
    printf("done\n");
    return 0;
}
