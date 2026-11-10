/* s290 phase-1 probe: what setsid(2) answers, black box. */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <pthread.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/wait.h>
#include <termios.h>
#include <unistd.h>

static const char *en(int e) {
    switch (e) {
    case 0: return "ok";
    case EPERM: return "EPERM";
    case ENXIO: return "ENXIO";
    case ENOTTY: return "ENOTTY";
    case EINVAL: return "EINVAL";
    case EIO: return "EIO";
    default: { static char b[32]; snprintf(b, sizeof b, "errno %d", e); return b; }
    }
}

static int tty_open(void) {
    int fd = open("/dev/tty", O_RDWR);
    int e = fd < 0 ? errno : 0;
    if (fd >= 0) close(fd);
    return e;
}

static void *thr_setsid(void *arg) {
    pid_t *out = arg;
    *out = setsid();
    return NULL;
}

/* A child that forks itself off, so it is not a group leader. */
static int child(int which) {
    pid_t me = getpid();
    switch (which) {
    case 1: { /* plain setsid by a non-leader */
        pid_t r = setsid();
        int e = r < 0 ? errno : 0;
        printf("C1 setsid by a non-leader: %s, sid==pid %s, pgid==pid %s, /dev/tty %s",
               en(e), getsid(0) == me ? "yes" : "no", getpgrp() == me ? "yes" : "no",
               en(tty_open()));
        errno = 0;
        pid_t fg = tcgetpgrp(0);
        printf(", tcgetpgrp(0) %s\n", fg < 0 ? en(errno) : "a group");
        /* again: now a session leader */
        r = setsid();
        printf("C1 setsid again by the session leader: %s\n", en(r < 0 ? errno : 0));
        return 0;
    }
    case 2: { /* setsid from a second thread */
        pid_t r = -2;
        pthread_t t;
        pthread_create(&t, NULL, thr_setsid, &r);
        pthread_join(t, NULL);
        printf("C2 setsid from another thread: %s, main thread's getsid(0)==pid %s, getpgrp()==pid %s, /dev/tty %s\n",
               r == me ? "ok (pid)" : "other", getsid(0) == me ? "yes" : "no",
               getpgrp() == me ? "yes" : "no", en(tty_open()));
        return 0;
    }
    case 3: { /* session leader without a terminal opens a fresh pty slave without O_NOCTTY */
        if (setsid() < 0) { printf("C3 setsid %s\n", en(errno)); return 0; }
        int m = posix_openpt(O_RDWR | O_NOCTTY);
        if (m < 0 || grantpt(m) < 0 || unlockpt(m) < 0) { printf("C3 pty %s\n", en(errno)); return 0; }
        char *name = ptsname(m);
        int s = open(name, O_RDWR); /* no O_NOCTTY */
        printf("C3 open of a free pty slave by a session leader with no terminal: /dev/tty after %s\n",
               s < 0 ? "open failed" : en(tty_open()));
        return 0;
    }
    case 4: { /* TIOCNOTTY by a non-leader on its controlling terminal */
        int fd = open("/dev/tty", O_RDWR);
        if (fd < 0) { printf("C4 no terminal: %s\n", en(errno)); return 0; }
        pid_t sid0 = getsid(0);
        int r = ioctl(fd, TIOCNOTTY);
        printf("C4 TIOCNOTTY by a non-leader: %s, same session %s, /dev/tty after %s\n",
               en(r < 0 ? errno : 0), getsid(0) == sid0 ? "yes" : "no", en(tty_open()));
        close(fd);
        return 0;
    }
    }
    return 1;
}

int main(void) {
    setvbuf(stdout, NULL, _IONBF, 0);
    pid_t me = getpid();
    printf("P start: pid==pgid %s, pid==sid %s, /dev/tty %s\n",
           getpgrp() == me ? "yes" : "no", getsid(0) == me ? "yes" : "no", en(tty_open()));
    /* lead a group, then setsid: EPERM */
    if (getpgrp() != me) setpgid(0, 0);
    pid_t r = setsid();
    printf("P setsid by a group leader: %s\n", r < 0 ? en(errno) : "ok");
    for (int w = 1; w <= 4; w++) {
        pid_t p = fork();
        if (p == 0) _exit(child(w));
        int st;
        waitpid(p, &st, 0);
    }
    return 0;
}
