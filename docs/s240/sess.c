/* s240 probe: how setsid, setpgid and tcsetpgrp shape groups and
 * sessions, what a terminal's interrupt character reaches, what a parent
 * hears of a stop, and what an orphaned group is sent.
 *
 * Written from POSIX.1-2024 (XBD 11, XSH setsid, setpgid, tcsetpgrp,
 * waitpid, _exit) and the man pages; no C library's or kernel's source
 * was read. Every process signalled is one this probe started, inside a
 * session it made. Output lines are `case: <answer>`.
 */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <termios.h>
#include <time.h>
#include <unistd.h>

static int rep[2];
static char tag[16] = "?";

static void heard(int sig) {
    char b[32];
    int n = 0;
    for (const char *p = tag; *p && n < 15; p++) b[n++] = *p;
    b[n++] = ':';
    const char *s = sig == SIGINT ? "INT" : sig == SIGHUP ? "HUP" : sig == SIGCONT ? "CONT" : sig == SIGTTOU ? "TTOU" : "?";
    while (*s) b[n++] = *s++;
    b[n++] = '\n';
    (void)!write(rep[1], b, (size_t)n);
}

static void on(int sig) {
    struct sigaction sa;
    memset(&sa, 0, sizeof sa);
    sa.sa_handler = heard;
    sa.sa_flags = SA_RESTART;
    sigemptyset(&sa.sa_mask);
    sigaction(sig, &sa, 0);
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
    case ENXIO: return "ENXIO";
    case ENOTTY: return "ENOTTY";
    case EIO: return "EIO";
    default: { static char b[32]; snprintf(b, sizeof b, "errno%d", e); return b; }
    }
}

static void drain(char *out, size_t cap) {
    nap(250);
    char buf[2048];
    size_t len = 0;
    for (;;) {
        ssize_t r = read(rep[0], buf + len, sizeof buf - 1 - len);
        if (r <= 0) break;
        len += (size_t)r;
    }
    buf[len] = 0;
    for (size_t i = 0; i < len; i++) if (buf[i] == '\n') buf[i] = ',';
    if (len && buf[len - 1] == ',') buf[len - 1] = 0;
    snprintf(out, cap, "%s", len ? buf : "-");
}

#define ANS(name, expr) do { int r_ = (expr); printf("%s: %s\n", name, r_ < 0 ? ename(errno) : "0"); } while (0)

/* A child that pauses until killed; `pg` 0 new group, >0 join, -1 stay,
 * -2 new session. */
static pid_t idle(pid_t pg, const char *name, int sig1, int sig2) {
    int s[2];
    (void)!pipe(s);
    pid_t p = fork();
    if (p == 0) {
        close(s[0]);
        snprintf(tag, sizeof tag, "%s", name);
        if (sig1) on(sig1);
        if (sig2) on(sig2);
        if (pg == -2) setsid();
        else if (pg >= 0) setpgid(0, pg);
        (void)!write(s[1], "k", 1);
        for (;;) pause();
    }
    close(s[1]);
    if (pg >= 0) setpgid(p, pg ? pg : p);
    char c;
    (void)!read(s[0], &c, 1);
    close(s[0]);
    return p;
}

static void reap(pid_t p) { kill(p, SIGKILL); waitpid(p, 0, 0); }

static void groups_and_sessions(void) {
    /* The caller is a session leader here (main made it one). */
    ANS("setsid by a session leader", setsid());
    ANS("setpgid(0,0) by a session leader", setpgid(0, 0));
    {
        pid_t c = fork();
        if (c == 0) {
            setpgid(0, 0);
            int r = setsid();
            printf("setsid by a group leader (not session leader): %s\n", r < 0 ? ename(errno) : "0");
            _exit(0);
        }
        waitpid(c, 0, 0);
    }
    {
        pid_t c = fork();
        if (c == 0) {
            pid_t sid = setsid();
            int fd = open("/dev/tty", O_RDWR);
            int e = fd < 0 ? errno : 0;
            printf("setsid by a non-leader: sid==pid %s, pgid==pid %s, /dev/tty %s\n",
                   sid == getpid() ? "yes" : "no", getpgrp() == getpid() ? "yes" : "no",
                   fd < 0 ? ename(e) : "opened");
            _exit(0);
        }
        waitpid(c, 0, 0);
    }
    {
        pid_t other = idle(-2, "X", 0, 0);     /* leads a group in another session */
        pid_t c = idle(-1, "c", 0, 0);
        ANS("setpgid(child, group in another session)", setpgid(c, other));
        ANS("setpgid(child, a group that does not exist)", setpgid(c, 999999));
        ANS("setpgid(child, 0) before exec", setpgid(c, 0));
        reap(c); reap(other);
    }
    {
        int s[2];
        (void)!pipe(s);
        pid_t c = fork();
        if (c == 0) {
            close(s[0]);
            fcntl(s[1], F_SETFD, FD_CLOEXEC);
            execl("/bin/sleep", "sleep", "5", (char *)0);
            _exit(127);
        }
        close(s[1]);
        char b;
        (void)!read(s[0], &b, 1); /* EOF once the exec has happened */
        close(s[0]);
        ANS("setpgid(child, 0) after the child exec'd", setpgid(c, 0));
        reap(c);
    }
    {
        int s[2];
        (void)!pipe(s);
        pid_t c = fork();
        if (c == 0) {
            close(s[0]);
            pid_t g = fork();
            if (g == 0) { for (;;) pause(); }
            (void)!write(s[1], &g, sizeof g);
            for (;;) pause();
        }
        close(s[1]);
        pid_t g = 0;
        (void)!read(s[0], &g, sizeof g);
        ANS("setpgid(grandchild, 0)", setpgid(g, 0));
        kill(g, SIGKILL);
        reap(c);
    }
}

/* Stop and continue reports for a group. */
static void stops(void) {
    pid_t a = idle(0, "A", 0, 0);
    pid_t b = idle(a, "B", 0, 0);
    int st;
    ANS("kill(-G, SIGSTOP)", kill(-a, SIGSTOP));
    int n = 0;
    for (int i = 0; i < 2; i++) {
        pid_t w = waitpid(-a, &st, WUNTRACED);
        if (w > 0 && WIFSTOPPED(st) && WSTOPSIG(st) == SIGSTOP) n++;
    }
    printf("waitpid(-G, WUNTRACED) reported stopped: %d of 2\n", n);
    pid_t w = waitpid(-a, &st, WNOHANG | WUNTRACED);
    printf("a second waitpid(-G, WNOHANG|WUNTRACED) for the same stops: %s\n", w == 0 ? "0 (nothing new)" : w < 0 ? ename(errno) : "a pid");
    ANS("kill(-G, SIGCONT)", kill(-a, SIGCONT));
    n = 0;
#ifdef WCONTINUED
    for (int i = 0; i < 2; i++) {
        pid_t x = waitpid(-a, &st, WCONTINUED);
        if (x > 0 && WIFCONTINUED(st)) n++;
    }
    printf("waitpid(-G, WCONTINUED) reported continued: %d of 2\n", n);
#else
    printf("waitpid WCONTINUED: not defined on this host\n");
#endif
    w = waitpid(-a, &st, WNOHANG);
    printf("waitpid(-G, WNOHANG) with both running: %s\n", w == 0 ? "0" : w < 0 ? ename(errno) : "a pid");
    reap(a); reap(b);
}

/* An orphaned group with a stopped member. */
static void orphan(void) {
    int s[2];
    (void)!pipe(s);
    pid_t m = fork();
    if (m == 0) {
        setpgid(0, 0);
        pid_t m2 = fork();
        if (m2 == 0) {
            snprintf(tag, sizeof tag, "M2");
            on(SIGHUP);
            on(SIGCONT);
            (void)!write(s[1], "k", 1);
            raise(SIGSTOP);
            for (int i = 0; i < 20; i++) nap(50);
            _exit(0);
        }
        char c;
        (void)!read(s[0], &c, 1);
        nap(100); /* let M2 stop */
        _exit(0); /* M2's group loses its link to the session: orphaned */
    }
    close(s[1]);
    waitpid(m, 0, 0);
    char h[512];
    drain(h, sizeof h);
    nap(400);
    char h2[512];
    drain(h2, sizeof h2);
    printf("orphaned group with a stopped member, heard: %s %s\n", h, h2);
}

/* The terminal: a session of its own with a pty as its controlling
 * terminal. */
static void terminal(void) {
    int m = posix_openpt(O_RDWR | O_NOCTTY);
    if (m < 0 || grantpt(m) < 0 || unlockpt(m) < 0) { printf("pty: %s\n", ename(errno)); return; }
    char *name = ptsname(m);
    int s[2];
    (void)!pipe(s);
    pid_t L = fork();
    if (L == 0) {
        close(s[0]);
        snprintf(tag, sizeof tag, "L");
        on(SIGINT);
        setsid();
        int t = open(name, O_RDWR);
        pid_t fg = tcgetpgrp(t);
        int had = fg > 0;
        printf("open of the slave by a session leader: ctty %s (tcgetpgrp %s)\n",
               had ? "acquired" : "not acquired", fg < 0 ? ename(errno) : "a group");
        if (!had) {
#ifdef TIOCSCTTY
            int r = ioctl(t, TIOCSCTTY, 0);
            printf("TIOCSCTTY: %s; tcgetpgrp now %s\n", r < 0 ? ename(errno) : "0", tcgetpgrp(t) > 0 ? "a group" : "none");
#endif
        }
        printf("tcgetpgrp == the leader's group: %s\n", tcgetpgrp(t) == getpgrp() ? "yes" : "no");
        /* A job in its own group, handed the terminal. */
        pid_t j1 = idle(0, "J1", SIGINT, 0);
        pid_t j2 = idle(j1, "J2", SIGINT, 0);
        sigset_t ttou, old;
        sigemptyset(&ttou);
        sigaddset(&ttou, SIGTTOU);
        sigprocmask(SIG_BLOCK, &ttou, &old);
        ANS("tcsetpgrp(job) by the leader, SIGTTOU blocked", tcsetpgrp(t, j1));
        sigprocmask(SIG_SETMASK, &old, 0);
        /* A background process asks for the terminal with SIGTTOU at
         * its default: first from the leader's own group, which is
         * ORPHANED (the leader's parent is in another session), then
         * from a group of its own whose parent (the leader) is in this
         * session, which is not. */
        int st;
        for (int own = 0; own < 2; own++) {
            const char *which = own ? "a non-orphaned background group" : "the leader's (orphaned) group";
            pid_t b = fork();
            if (b == 0) {
                if (own) setpgid(0, 0);
                signal(SIGTTOU, SIG_DFL);
                int r = tcsetpgrp(t, getpgrp());
                printf("tcsetpgrp from %s, SIGTTOU default, returned: %s\n", which, r < 0 ? ename(errno) : "0");
                _exit(0);
            }
            if (own) setpgid(b, b);
            pid_t w = waitpid(b, &st, WUNTRACED);
            if (w == b && WIFSTOPPED(st)) {
                printf("tcsetpgrp from %s, SIGTTOU default: stopped by %s\n", which, WSTOPSIG(st) == SIGTTOU ? "SIGTTOU" : "another signal");
                kill(b, SIGKILL);
                waitpid(b, 0, 0);
            } else {
                printf("tcsetpgrp from %s, SIGTTOU default: not stopped\n", which);
            }
        }
        pid_t b;
        b = fork();
        if (b == 0) {
            signal(SIGTTOU, SIG_IGN);
            int r = tcsetpgrp(t, j1);
            printf("tcsetpgrp from the background, SIGTTOU ignored: %s\n", r < 0 ? ename(errno) : "0");
            _exit(0);
        }
        waitpid(b, 0, 0);
        pid_t other = idle(-2, "X", 0, 0);
        sigprocmask(SIG_BLOCK, &ttou, &old);
        ANS("tcsetpgrp(group of another session)", tcsetpgrp(t, other));
        ANS("tcsetpgrp(a group that does not exist)", tcsetpgrp(t, 999999));
        ANS("tcsetpgrp(0)", tcsetpgrp(t, 0));
        ANS("tcsetpgrp(-1)", tcsetpgrp(t, -1));
        sigprocmask(SIG_SETMASK, &old, 0);
        reap(other);
        /* The interrupt character, with J the foreground group: */
        (void)!write(s[1], "k", 1);
        nap(500);
        char h[512];
        drain(h, sizeof h);
        printf("interrupt character with the job in the foreground, heard: %s\n", h);
        reap(j1); reap(j2);
        _exit(0);
    }
    close(s[1]);
    char c;
    (void)!read(s[0], &c, 1);
    struct termios tio;
    if (tcgetattr(m, &tio) == 0) {
        char intr = (char)tio.c_cc[VINTR];
        (void)!write(m, &intr, 1);
    }
    waitpid(L, 0, 0);
    /* drain anything the pty echoed */
    fcntl(m, F_SETFL, O_NONBLOCK);
    char sink[256];
    while (read(m, sink, sizeof sink) > 0) {}
    close(m);
}

int main(void) {
    setvbuf(stdout, 0, _IOLBF, 0);
    if (pipe(rep) < 0) return 2;
    fcntl(rep[0], F_SETFL, O_NONBLOCK);
    pid_t r = fork();
    if (r > 0) { int st; waitpid(r, &st, 0); printf("done\n"); return WIFEXITED(st) ? WEXITSTATUS(st) : 1; }
    setsid();
    printf("host: pid=%d pgid=%d sid=%d euid=%d\n", (int)getpid(), (int)getpgrp(), (int)getsid(0), (int)geteuid());
    groups_and_sessions();
    stops();
    orphan();
    terminal();
    return 0;
}
