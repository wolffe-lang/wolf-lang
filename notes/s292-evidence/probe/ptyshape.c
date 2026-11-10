/* s292 phase 1: what a host's pseudo-terminals answer, black box, before
 * any wolf call exists. One program for linux and macOS; each row is one
 * line, a pty's number written N. Built with the host's cc, run from a
 * scratch directory (the A4 row writes ./ptyshape.tmp there).
 *
 * A: the open and the name. B: which end takes the size and the settings,
 * and when. C: SIGWINCH to a child that leads a session on the slave.
 * D: every RFC 4254 section 8 opcode set on the slave and read back.
 * E: the interrupt character's effect. J: a master opened without
 * O_NOCTTY by a session leader with no terminal.
 */
#define _GNU_SOURCE
#define _DARWIN_C_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/wait.h>
#include <termios.h>
#include <time.h>
#include <unistd.h>

static const char *en(int e) {
    switch (e) {
    case 0: return "ok";
    case EINVAL: return "EINVAL";
    case ENOTTY: return "ENOTTY";
    case EIO: return "EIO";
    case ENXIO: return "ENXIO";
    case EPERM: return "EPERM";
    case EACCES: return "EACCES";
    case EBADF: return "EBADF";
    case ENOENT: return "ENOENT";
    case EAGAIN: return "EAGAIN";
    case ENODEV: return "ENODEV";
    case EBUSY: return "EBUSY";
    case ENOSPC: return "ENOSPC";
    default: {
        static char b[32];
        snprintf(b, sizeof b, "errno %d", e);
        return b;
    }
    }
}

/* The name with every run of digits written N. */
static const char *shape(const char *s) {
    static char b[256];
    size_t o = 0;
    int last = 0;
    for (; *s && o + 2 < sizeof b; s++) {
        int d = *s >= '0' && *s <= '9';
        if (d) {
            if (!last) b[o++] = 'N';
        } else {
            b[o++] = *s;
        }
        last = d;
    }
    b[o] = 0;
    return b;
}

static int name_of(int m, char *buf, size_t n) {
    errno = 0;
    int r = ptsname_r(m, buf, n);
    if (r == 0) return 0;
    return r > 0 ? r : errno;
}

static const char *cloexec(int fd) {
    int f = fcntl(fd, F_GETFD);
    if (f < 0) return "F_GETFD failed";
    return (f & FD_CLOEXEC) ? "close-on-exec" : "inherited";
}

/* A master, granted and unlocked, by the route A2 measures. */
static int open_pair(char *name, size_t n) {
    int m = open("/dev/ptmx", O_RDWR | O_NOCTTY | O_CLOEXEC);
    if (m < 0) return -1;
    if (grantpt(m) != 0 || unlockpt(m) != 0 || name_of(m, name, n) != 0) {
        close(m);
        return -1;
    }
    return m;
}

static void msleep(int ms) {
    struct timespec t = {ms / 1000, (long)(ms % 1000) * 1000000L};
    while (nanosleep(&t, &t) != 0 && errno == EINTR) {
    }
}

/* Four calls on one descriptor: the size read and set, the settings read
 * and set (the settings set are the ones read, or a fresh struct when the
 * read failed). */
static void four(const char *row, int fd) {
    struct winsize w;
    memset(&w, 0, sizeof w);
    errno = 0;
    int g = ioctl(fd, TIOCGWINSZ, &w);
    int ge = g < 0 ? errno : 0;
    struct winsize s = w;
    if (g < 0) {
        memset(&s, 0, sizeof s);
    }
    s.ws_row = 24;
    s.ws_col = 80;
    errno = 0;
    int sr = ioctl(fd, TIOCSWINSZ, &s);
    int se = sr < 0 ? errno : 0;
    struct termios t;
    memset(&t, 0, sizeof t);
    errno = 0;
    int a = tcgetattr(fd, &t);
    int ae = a < 0 ? errno : 0;
    if (a < 0) {
        memset(&t, 0, sizeof t);
        t.c_cflag = CS8 | CREAD;
    }
    errno = 0;
    int b = tcsetattr(fd, TCSANOW, &t);
    int be = b < 0 ? errno : 0;
    printf("%s TIOCGWINSZ %s, TIOCSWINSZ %s, tcgetattr %s, tcsetattr %s\n", row, en(ge), en(se),
           en(ae), en(be));
}

static void size_of(const char *row, const char *end, int fd) {
    struct winsize w;
    memset(&w, 0, sizeof w);
    if (ioctl(fd, TIOCGWINSZ, &w) < 0) {
        printf("%s %s's size: %s\n", row, end, en(errno));
    } else {
        printf("%s %s's size: %u %u %u %u\n", row, end, w.ws_row, w.ws_col, w.ws_xpixel,
               w.ws_ypixel);
    }
}

static int set_size(int fd, int r, int c, int xp, int yp) {
    struct winsize w;
    memset(&w, 0, sizeof w);
    w.ws_row = (unsigned short)r;
    w.ws_col = (unsigned short)c;
    w.ws_xpixel = (unsigned short)xp;
    w.ws_ypixel = (unsigned short)yp;
    return ioctl(fd, TIOCSWINSZ, &w) < 0 ? errno : 0;
}

static int set_cc(int fd, int idx, unsigned char v) {
    struct termios t;
    if (tcgetattr(fd, &t) < 0) return errno;
    t.c_cc[idx] = v;
    return tcsetattr(fd, TCSANOW, &t) < 0 ? errno : 0;
}

static int get_cc(int fd, int idx) {
    struct termios t;
    if (tcgetattr(fd, &t) < 0) return -1000 - errno;
    return t.c_cc[idx];
}

static void part_a(void) {
    char name[256];
    int m = posix_openpt(O_RDWR | O_NOCTTY | O_CLOEXEC);
    if (m < 0) {
        printf("A1 posix_openpt(O_RDWR|O_NOCTTY|O_CLOEXEC): %s\n", en(errno));
    } else {
        printf("A1 posix_openpt(O_RDWR|O_NOCTTY|O_CLOEXEC): a master, %s\n", cloexec(m));
        close(m);
    }
    m = posix_openpt(O_RDWR | O_NOCTTY);
    if (m < 0) {
        printf("A1b posix_openpt(O_RDWR|O_NOCTTY): %s\n", en(errno));
    } else {
        printf("A1b posix_openpt(O_RDWR|O_NOCTTY): a master, %s\n", cloexec(m));
        close(m);
    }
    m = open("/dev/ptmx", O_RDWR | O_NOCTTY | O_CLOEXEC);
    if (m < 0) {
        printf("A2 open /dev/ptmx O_RDWR|O_NOCTTY|O_CLOEXEC: %s\n", en(errno));
        return;
    }
    int g = grantpt(m) == 0 ? 0 : errno;
    int u = unlockpt(m) == 0 ? 0 : errno;
    int n = name_of(m, name, sizeof name);
    printf("A2 open /dev/ptmx O_RDWR|O_NOCTTY|O_CLOEXEC: a master, %s; grantpt %s, unlockpt %s, "
           "ptsname_r %s, name %s\n",
           cloexec(m), en(g), en(u), en(n), n == 0 ? shape(name) : "-");
    int s = open(name, O_RDWR | O_NOCTTY);
    printf("A2b its slave opens: %s; isatty %d\n", s < 0 ? en(errno) : "ok", s >= 0 && isatty(s));
    if (s >= 0) {
        char sn[256];
        int r = name_of(s, sn, sizeof sn);
        printf("A4 ptsname_r on the slave: %s\n", en(r));
        close(s);
    }
    close(m);
    /* A3: the slave opened before unlockpt. */
    m = open("/dev/ptmx", O_RDWR | O_NOCTTY | O_CLOEXEC);
    if (m >= 0) {
        g = grantpt(m) == 0 ? 0 : errno;
        n = name_of(m, name, sizeof name);
        printf("A3 before unlockpt: grantpt %s, ptsname_r %s\n", en(g), en(n));
        if (n == 0) {
            s = open(name, O_RDWR | O_NOCTTY);
            printf("A3 the locked slave's open: %s\n", s < 0 ? en(errno) : "ok (opened)");
            if (s >= 0) close(s);
        }
        close(m);
    }
    int f = open("ptyshape.tmp", O_RDWR | O_CREAT | O_TRUNC, 0600);
    if (f >= 0) {
        int r = name_of(f, name, sizeof name);
        printf("A4 ptsname_r on a file: %s\n", en(r));
        errno = 0;
        r = unlockpt(f) == 0 ? 0 : errno;
        printf("A4 unlockpt on a file: %s\n", en(r));
        close(f);
        unlink("ptyshape.tmp");
    }
}

static void dump(const char *row, int fd) {
    struct termios t;
    if (tcgetattr(fd, &t) < 0) {
        printf("%s tcgetattr: %s\n", row, en(errno));
        return;
    }
    printf("%s iflag %#lx oflag %#lx cflag %#lx lflag %#lx ispeed %lu ospeed %lu cc", row,
           (unsigned long)t.c_iflag, (unsigned long)t.c_oflag, (unsigned long)t.c_cflag,
           (unsigned long)t.c_lflag, (unsigned long)cfgetispeed(&t),
           (unsigned long)cfgetospeed(&t));
    for (int i = 0; i < NCCS; i++) printf(" %d", t.c_cc[i]);
    printf("\n");
}

static void part_b(void) {
    char name[256];
    int m = open_pair(name, sizeof name);
    if (m < 0) {
        printf("B0 a pair: %s\n", en(errno));
        return;
    }
    /* B6 first (linux): settings set on the master before any slave open. */
    int e6 = set_cc(m, VINTR, 7);
    int s6 = set_size(m, 33, 77, 0, 0);
    printf("B6 before any slave open, on the master: VINTR=7 %s, size 33x77 %s\n", en(e6), en(s6));
    four("B1 the master, no slave ever opened:", m);
    int s = open(name, O_RDWR | O_NOCTTY);
    if (s < 0) {
        printf("B2 the slave's open: %s\n", en(errno));
        close(m);
        return;
    }
    printf("B6 at the slave's first open: VINTR %d", get_cc(s, VINTR));
    size_of(";", "the slave", s);
    dump("D2 a pty's settings, the slave:", s);
    four("B2 the master, the slave open:", m);
    four("B2s the slave:", s);
    int r = set_size(m, 40, 132, 0, 0);
    printf("B3 TIOCSWINSZ 40x132 on the master: %s;", en(r));
    size_of("", "the slave", s);
    r = set_size(s, 50, 100, 0, 0);
    printf("B3 TIOCSWINSZ 50x100 on the slave: %s;", en(r));
    size_of("", "the master", m);
    r = set_cc(m, VERASE, 8);
    printf("B3 VERASE=8 through the master: %s; the slave reads %d\n", en(r), get_cc(s, VERASE));
    r = set_cc(s, VKILL, 0x15 + 1);
    printf("B3 VKILL=22 through the slave: %s; the master reads %d\n", en(r), get_cc(m, VKILL));
    close(s);
    four("B4 the master after the slave's last close:", m);
    s = open(name, O_RDWR | O_NOCTTY);
    if (s < 0) {
        printf("B5 the slave opened again: %s\n", en(errno));
        close(m);
        return;
    }
    printf("B5 the slave opened again: VINTR %d VERASE %d VKILL %d", get_cc(s, VINTR),
           get_cc(s, VERASE), get_cc(s, VKILL));
    size_of(";", "the slave", s);
    int e1 = set_cc(s, VINTR, 9);
    int e2 = set_size(s, 41, 133, 0, 0);
    printf("B5 set on the slave: VINTR=9 %s, 41x133 %s\n", en(e1), en(e2));
    close(s);
    s = open(name, O_RDWR | O_NOCTTY);
    if (s < 0) {
        printf("B5 the slave opened a third time: %s\n", en(errno));
    } else {
        printf("B5 after its close and a new open: VINTR %d", get_cc(s, VINTR));
        size_of(";", "the slave", s);
        close(s);
    }
    close(m);
}

/* ---- C and E: a child that leads a session on the slave. ---- */

static volatile sig_atomic_t n_winch, n_int, n_quit;
static void on_winch(int x) { (void)x; n_winch++; }
static void on_int(int x) { (void)x; n_int++; }
static void on_quit(int x) { (void)x; n_quit++; }

static void child(const char *name, int cmd, int rep) {
    struct sigaction sa;
    memset(&sa, 0, sizeof sa);
    sa.sa_flags = SA_RESTART;
    sa.sa_handler = on_winch;
    sigaction(SIGWINCH, &sa, NULL);
    sa.sa_handler = on_int;
    sigaction(SIGINT, &sa, NULL);
    sa.sa_handler = on_quit;
    sigaction(SIGQUIT, &sa, NULL);
    char line[256];
    pid_t sid = setsid();
    int s = open(name, O_RDWR);
    int tty = open("/dev/tty", O_RDWR);
    int tty_e = tty < 0 ? errno : 0;
    if (tty >= 0) close(tty);
    pid_t fg = s >= 0 ? tcgetpgrp(s) : -1;
    int n = snprintf(line, sizeof line, "setsid %s; slave %s; /dev/tty %s; foreground %s\n",
                     sid == getpid() ? "pid" : "failed", s >= 0 ? "open" : en(errno),
                     en(tty_e), fg == getpgrp() ? "its group" : "another");
    if (write(rep, line, (size_t)n) < 0) _exit(3);
    for (;;) {
        char c;
        ssize_t k = read(cmd, &c, 1);
        if (k <= 0) _exit(2);
        if (c == 'x') _exit(0);
        n = snprintf(line, sizeof line, "winch %d int %d quit %d\n", (int)n_winch, (int)n_int,
                     (int)n_quit);
        n_winch = n_int = n_quit = 0;
        if (write(rep, line, (size_t)n) < 0) _exit(3);
    }
}

static void ask(const char *row, int cmd, int rep) {
    msleep(250);
    char c = 'q', b[256];
    if (write(cmd, &c, 1) != 1) {
        printf("%s the ask failed\n", row);
        return;
    }
    ssize_t k = read(rep, b, sizeof b - 1);
    b[k > 0 ? k : 0] = 0;
    printf("%s %s", row, k > 0 ? b : "no answer\n");
}

static void drain(int m) {
    char b[512];
    int fl = fcntl(m, F_GETFL);
    fcntl(m, F_SETFL, fl | O_NONBLOCK);
    while (read(m, b, sizeof b) > 0) {
    }
    fcntl(m, F_SETFL, fl);
}

static void part_c(void) {
    char name[256];
    int m = open_pair(name, sizeof name);
    if (m < 0) {
        printf("C0 a pair: %s\n", en(errno));
        return;
    }
    /* E2: this process (no session leader) holds a slave first. */
    int s = open(name, O_RDWR | O_NOCTTY);
    int cp[2], rp[2];
    if (s < 0 || pipe(cp) != 0 || pipe(rp) != 0) {
        printf("C0 setup: %s\n", en(errno));
        return;
    }
    fflush(stdout);
    pid_t p = fork();
    if (p == 0) {
        close(m);
        close(s);
        close(cp[1]);
        close(rp[0]);
        child(name, cp[0], rp[1]);
    }
    close(cp[0]);
    close(rp[1]);
    char b[256];
    ssize_t k = read(rp[0], b, sizeof b - 1);
    b[k > 0 ? k : 0] = 0;
    printf("E2 the child, a slave already held open elsewhere: %s", k > 0 ? b : "no answer\n");
    int r = set_size(m, 60, 120, 0, 0);
    printf("C1 TIOCSWINSZ 60x120 on the master: %s\n", en(r));
    ask("C1 the child:", cp[1], rp[0]);
    r = set_size(s, 61, 121, 0, 0);
    printf("C2 TIOCSWINSZ 61x121 on a slave that is not the setter's terminal: %s\n", en(r));
    ask("C2 the child:", cp[1], rp[0]);
    r = set_size(m, 61, 121, 0, 0);
    printf("C3 the same size again on the master: %s\n", en(r));
    ask("C3 the child:", cp[1], rp[0]);
    r = set_size(m, 61, 121, 640, 480);
    printf("C4 the pixel fields alone on the master: %s\n", en(r));
    ask("C4 the child:", cp[1], rp[0]);
    r = set_cc(s, VINTR, 7);
    char g = 7;
    ssize_t w = write(m, &g, 1);
    printf("E1 VINTR=7 on the slave: %s; 0x07 written to the master: %zd\n", en(r), w);
    ask("E1 the child:", cp[1], rp[0]);
    g = 3;
    w = write(m, &g, 1);
    printf("E1 0x03 written to the master: %zd\n", w);
    ask("E1 the child:", cp[1], rp[0]);
    g = 0x1c;
    w = write(m, &g, 1);
    printf("E1 0x1c (VQUIT) written to the master: %zd\n", w);
    ask("E1 the child:", cp[1], rp[0]);
    drain(m);
    char x = 'x';
    if (write(cp[1], &x, 1) != 1) printf("C5 the end could not be asked\n");
    int st = 0;
    waitpid(p, &st, 0);
    printf("C5 the child's status: %d\n", WIFEXITED(st) ? WEXITSTATUS(st) : -WTERMSIG(st));
    close(cp[1]);
    close(rp[0]);
    close(s);
    close(m);
}

/* ---- D: every opcode, set on the slave and read back. ---- */

enum kind { CHR, IFL, OFL, LFL, CFL, SIZE7, SIZE8, SPEED_I, SPEED_O, NONE };
struct op {
    int code;
    const char *name;
    enum kind k;
    long v; /* the c_cc index, or the flag's mask */
};

#ifndef VDSUSP
#define VDSUSP_I -1
#else
#define VDSUSP_I VDSUSP
#endif
#ifndef VSTATUS
#define VSTATUS_I -1
#else
#define VSTATUS_I VSTATUS
#endif
#if defined(VSWTCH)
#define VSWTCH_I VSWTCH
#elif defined(VSWTC)
#define VSWTCH_I VSWTC
#else
#define VSWTCH_I -1
#endif
#ifdef VFLUSH
#define VFLUSH_I VFLUSH
#else
#define VFLUSH_I -1
#endif
#ifdef IUCLC
#define IUCLC_M IUCLC
#else
#define IUCLC_M 0
#endif
#ifdef XCASE
#define XCASE_M XCASE
#else
#define XCASE_M 0
#endif
#ifdef OLCUC
#define OLCUC_M OLCUC
#else
#define OLCUC_M 0
#endif
#ifdef IUTF8
#define IUTF8_M IUTF8
#else
#define IUTF8_M 0
#endif

static const struct op OPS[] = {
    {1, "VINTR", CHR, VINTR},       {2, "VQUIT", CHR, VQUIT},
    {3, "VERASE", CHR, VERASE},     {4, "VKILL", CHR, VKILL},
    {5, "VEOF", CHR, VEOF},         {6, "VEOL", CHR, VEOL},
    {7, "VEOL2", CHR, VEOL2},       {8, "VSTART", CHR, VSTART},
    {9, "VSTOP", CHR, VSTOP},       {10, "VSUSP", CHR, VSUSP},
    {11, "VDSUSP", CHR, VDSUSP_I},  {12, "VREPRINT", CHR, VREPRINT},
    {13, "VWERASE", CHR, VWERASE},  {14, "VLNEXT", CHR, VLNEXT},
    {15, "VFLUSH", CHR, VFLUSH_I},  {16, "VSWTCH", CHR, VSWTCH_I},
    {17, "VSTATUS", CHR, VSTATUS_I}, {18, "VDISCARD", CHR, VDISCARD},
    {30, "IGNPAR", IFL, IGNPAR},    {31, "PARMRK", IFL, PARMRK},
    {32, "INPCK", IFL, INPCK},      {33, "ISTRIP", IFL, ISTRIP},
    {34, "INLCR", IFL, INLCR},      {35, "IGNCR", IFL, IGNCR},
    {36, "ICRNL", IFL, ICRNL},      {37, "IUCLC", IFL, IUCLC_M},
    {38, "IXON", IFL, IXON},        {39, "IXANY", IFL, IXANY},
    {40, "IXOFF", IFL, IXOFF},      {41, "IMAXBEL", IFL, IMAXBEL},
    {42, "IUTF8", IFL, IUTF8_M},    {50, "ISIG", LFL, ISIG},
    {51, "ICANON", LFL, ICANON},    {52, "XCASE", LFL, XCASE_M},
    {53, "ECHO", LFL, ECHO},        {54, "ECHOE", LFL, ECHOE},
    {55, "ECHOK", LFL, ECHOK},      {56, "ECHONL", LFL, ECHONL},
    {57, "NOFLSH", LFL, NOFLSH},    {58, "TOSTOP", LFL, TOSTOP},
    {59, "IEXTEN", LFL, IEXTEN},    {60, "ECHOCTL", LFL, ECHOCTL},
    {61, "ECHOKE", LFL, ECHOKE},    {62, "PENDIN", LFL, PENDIN},
    {70, "OPOST", OFL, OPOST},      {71, "OLCUC", OFL, OLCUC_M},
    {72, "ONLCR", OFL, ONLCR},      {73, "OCRNL", OFL, OCRNL},
    {74, "ONOCR", OFL, ONOCR},      {75, "ONLRET", OFL, ONLRET},
    {90, "CS7", SIZE7, CS7},        {91, "CS8", SIZE8, CS8},
    {92, "PARENB", CFL, PARENB},    {93, "PARODD", CFL, PARODD},
    {128, "TTY_OP_ISPEED", SPEED_I, 0}, {129, "TTY_OP_OSPEED", SPEED_O, 0},
};

static tcflag_t *field(struct termios *t, enum kind k) {
    switch (k) {
    case IFL: return &t->c_iflag;
    case OFL: return &t->c_oflag;
    case LFL: return &t->c_lflag;
    default: return &t->c_cflag;
    }
}

static void part_d(void) {
    char name[256];
    int m = open_pair(name, sizeof name);
    if (m < 0) {
        printf("D0 a pair: %s\n", en(errno));
        return;
    }
    int s = open(name, O_RDWR | O_NOCTTY);
    if (s < 0) {
        printf("D0 the slave: %s\n", en(errno));
        return;
    }
    int kept = 0, lost = 0, lacks = 0;
    for (size_t i = 0; i < sizeof OPS / sizeof OPS[0]; i++) {
        const struct op *o = &OPS[i];
        struct termios t0, t, back;
        if ((o->k == CHR && o->v < 0) || (o->k != CHR && o->k < SIZE7 && o->v == 0)) {
            printf("D %d %s: the host lacks it\n", o->code, o->name);
            lacks++;
            continue;
        }
        if (tcgetattr(s, &t0) < 0) {
            printf("D %d %s: tcgetattr %s\n", o->code, o->name, en(errno));
            continue;
        }
        t = t0;
        long want = 0, before = 0, got = 0;
        switch (o->k) {
        case CHR:
            before = t.c_cc[o->v];
            want = before == 7 ? 8 : 7;
            t.c_cc[o->v] = (cc_t)want;
            break;
        case IFL:
        case OFL:
        case LFL:
        case CFL:
            before = (*field(&t, o->k) & (tcflag_t)o->v) != 0;
            want = !before;
            if (want) *field(&t, o->k) |= (tcflag_t)o->v;
            else *field(&t, o->k) &= ~(tcflag_t)o->v;
            break;
        case SIZE7:
            before = (t.c_cflag & CSIZE) == CS7;
            want = 1;
            t.c_cflag = (t.c_cflag & ~(tcflag_t)CSIZE) | CS7;
            break;
        case SIZE8:
            before = (t.c_cflag & CSIZE) == CS8;
            want = 1;
            {
                struct termios t7 = t;
                t7.c_cflag = (t7.c_cflag & ~(tcflag_t)CSIZE) | CS6;
                tcsetattr(s, TCSANOW, &t7);
            }
            t.c_cflag = (t.c_cflag & ~(tcflag_t)CSIZE) | CS8;
            break;
        case SPEED_I:
            before = (long)cfgetispeed(&t);
            want = (long)B9600;
            cfsetispeed(&t, B9600);
            break;
        case SPEED_O:
            before = (long)cfgetospeed(&t);
            want = (long)B9600;
            cfsetospeed(&t, B9600);
            break;
        default:
            break;
        }
        errno = 0;
        int r = tcsetattr(s, TCSANOW, &t);
        int re = r < 0 ? errno : 0;
        if (tcgetattr(s, &back) < 0) {
            printf("D %d %s: the read back %s\n", o->code, o->name, en(errno));
            continue;
        }
        switch (o->k) {
        case CHR: got = back.c_cc[o->v]; break;
        case IFL:
        case OFL:
        case LFL:
        case CFL: got = (*field(&back, o->k) & (tcflag_t)o->v) != 0; break;
        case SIZE7: got = (back.c_cflag & CSIZE) == CS7; break;
        case SIZE8: got = (back.c_cflag & CSIZE) == CS8; break;
        case SPEED_I: got = (long)cfgetispeed(&back); break;
        case SPEED_O: got = (long)cfgetospeed(&back); break;
        default: break;
        }
        int ok = got == want;
        if (ok) kept++;
        else lost++;
        printf("D %d %s: was %ld, set %ld (%s), reads %ld: %s\n", o->code, o->name, before, want,
               en(re), got, ok ? "kept" : "NOT KEPT");
        tcsetattr(s, TCSANOW, &t0);
    }
    printf("D kept %d, not kept %d, lacking %d of %zu\n", kept, lost, lacks,
           sizeof OPS / sizeof OPS[0]);
    /* The disabled value, and three more speeds. */
#ifdef _POSIX_VDISABLE
    int dis = (unsigned char)_POSIX_VDISABLE;
#else
    int dis = 0;
#endif
    int r = set_cc(s, VINTR, (unsigned char)dis);
    printf("D3 VINTR set to the host's disabled value %d: %s; reads %d\n", dis, en(r),
           get_cc(s, VINTR));
    static const speed_t sp[] = {B115200, B230400, B50};
    static const char *spn[] = {"B115200", "B230400", "B50"};
    for (int i = 0; i < 3; i++) {
        struct termios t;
        tcgetattr(s, &t);
        int a = cfsetospeed(&t, sp[i]) == 0 ? 0 : errno;
        int b = tcsetattr(s, TCSANOW, &t) == 0 ? 0 : errno;
        struct termios back;
        tcgetattr(s, &back);
        printf("D4 ospeed %s (%lu): cfsetospeed %s, tcsetattr %s, reads %lu\n", spn[i],
               (unsigned long)sp[i], en(a), en(b), (unsigned long)cfgetospeed(&back));
    }
    {
        struct termios t;
        tcgetattr(s, &t);
        errno = 0;
        int a = cfsetospeed(&t, (speed_t)12345) == 0 ? 0 : errno;
        int b = tcsetattr(s, TCSANOW, &t) == 0 ? 0 : errno;
        struct termios back;
        tcgetattr(s, &back);
        printf("D4 ospeed 12345, no constant: cfsetospeed %s, tcsetattr %s, reads %lu\n", en(a),
               en(b), (unsigned long)cfgetospeed(&back));
    }
    {
        struct termios t;
        tcgetattr(s, &t);
        t.c_cflag |= PARENB | PARODD;
        t.c_cflag = (t.c_cflag & ~(tcflag_t)CSIZE) | CS7;
        int b = tcsetattr(s, TCSANOW, &t) == 0 ? 0 : errno;
        struct termios back;
        tcgetattr(s, &back);
        printf("D5 CS7 PARENB PARODD in one set: %s; cflag before %#lx after %#lx\n", en(b),
               (unsigned long)t.c_cflag, (unsigned long)back.c_cflag);
    }
    close(s);
    close(m);
}

static void part_j(void) {
    int p[2];
    if (pipe(p) != 0) return;
    fflush(stdout);
    pid_t c = fork();
    if (c == 0) {
        close(p[0]);
        char line[200];
        pid_t sid = setsid();
        int m = open("/dev/ptmx", O_RDWR);
        int me = m < 0 ? errno : 0;
        int t = open("/dev/tty", O_RDWR);
        int te = t < 0 ? errno : 0;
        int n = snprintf(line, sizeof line,
                         "J1 a session leader with no terminal opens /dev/ptmx without "
                         "O_NOCTTY: setsid %s, open %s, then /dev/tty %s\n",
                         sid == getpid() ? "pid" : "failed", en(me), en(te));
        if (write(p[1], line, (size_t)n) < 0) _exit(3);
        _exit(0);
    }
    close(p[1]);
    char b[256];
    ssize_t k = read(p[0], b, sizeof b - 1);
    b[k > 0 ? k : 0] = 0;
    printf("%s", k > 0 ? b : "J1 no answer\n");
    waitpid(c, NULL, 0);
    close(p[0]);
}

int main(void) {
    setvbuf(stdout, NULL, _IOLBF, 0);
    part_a();
    part_b();
    part_c();
    part_d();
    part_j();
    printf("END\n");
    return 0;
}
