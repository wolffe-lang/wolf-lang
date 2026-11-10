/* s292 phase 1, the second probe: what a slave descriptor held open does to
 * a pty's settings across another open (macOS resets them at an open when
 * no descriptor of the slave is open: ptyshape.c's B5), and whether an
 * O_NOCTTY open by a session leader keeps the slave from becoming its
 * terminal. Rows K1..K4 of notes/s292-prediction.md section 8.
 */
#define _GNU_SOURCE
#define _DARWIN_C_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/wait.h>
#include <termios.h>
#include <time.h>
#include <unistd.h>

static int open_pair(char *name, size_t n) {
    int m = open("/dev/ptmx", O_RDWR | O_NOCTTY | O_CLOEXEC);
    if (m < 0) return -1;
    if (grantpt(m) != 0 || unlockpt(m) != 0 || ptsname_r(m, name, n) != 0) {
        close(m);
        return -1;
    }
    return m;
}

static void set_both(int fd, int intr, int rows, int cols) {
    struct termios t;
    tcgetattr(fd, &t);
    t.c_cc[VINTR] = (cc_t)intr;
    tcsetattr(fd, TCSANOW, &t);
    struct winsize w;
    memset(&w, 0, sizeof w);
    w.ws_row = (unsigned short)rows;
    w.ws_col = (unsigned short)cols;
    ioctl(fd, TIOCSWINSZ, &w);
}

static int read_both(int fd, char *out, size_t n) {
    struct termios t;
    struct winsize w;
    memset(&w, 0, sizeof w);
    if (tcgetattr(fd, &t) < 0 || ioctl(fd, TIOCGWINSZ, &w) < 0) {
        return snprintf(out, n, "read failed: errno %d", errno);
    }
    return snprintf(out, n, "VINTR %d, size %u %u", t.c_cc[VINTR], w.ws_row, w.ws_col);
}

static void msleep(int ms) {
    struct timespec t = {0, (long)ms * 1000000L};
    nanosleep(&t, NULL);
}

static void k1(void) {
    char name[128], b[128];
    int m = open_pair(name, sizeof name);
    int s1 = open(name, O_RDWR | O_NOCTTY);
    set_both(s1, 9, 42, 134);
    int s2 = open(name, O_RDWR | O_NOCTTY);
    read_both(s2, b, sizeof b);
    printf("K1 set on a held slave, read on a second open: %s\n", b);
    close(s2);
    close(s1);
    close(m);
}

/* K2 (keep = 1) and K3 (keep = 0). */
static void k23(const char *row, int keep) {
    char name[128], b[200];
    int m = open_pair(name, sizeof name);
    int s1 = open(name, O_RDWR | O_NOCTTY);
    set_both(s1, 9, 42, 134);
    int p[2];
    if (pipe(p) != 0) return;
    fflush(stdout);
    pid_t c = fork();
    if (c == 0) {
        close(p[0]);
        close(m);
        if (!keep) close(s1);
        msleep(150); /* the parent has closed its copy by now */
        setsid();
        int s = open(name, O_RDWR);
        int t = open("/dev/tty", O_RDWR);
        int k = snprintf(b, sizeof b, "/dev/tty %s; ", t >= 0 ? "ok" : "failed");
        read_both(s, b + k, sizeof b - (size_t)k);
        if (keep) close(s1);
        size_t len = strlen(b);
        b[len++] = '\n';
        if (write(p[1], b, len) < 0) _exit(3);
        _exit(0);
    }
    close(p[1]);
    close(s1);
    ssize_t k = read(p[0], b, sizeof b - 1);
    b[k > 0 ? k : 0] = 0;
    printf("%s %s", row, k > 0 ? b : "no answer\n");
    waitpid(c, NULL, 0);
    close(p[0]);
    close(m);
}

static void k4(void) {
    int p[2];
    if (pipe(p) != 0) return;
    fflush(stdout);
    pid_t c = fork();
    if (c == 0) {
        char name[128], b[200];
        close(p[0]);
        setsid();
        int m = open_pair(name, sizeof name);
        int s1 = open(name, O_RDWR | O_NOCTTY);
        int t1 = open("/dev/tty", O_RDWR);
        int e1 = t1 < 0 ? errno : 0;
        if (t1 >= 0) close(t1);
        int s2 = open(name, O_RDWR);
        int t2 = open("/dev/tty", O_RDWR);
        int e2 = t2 < 0 ? errno : 0;
        int n = snprintf(b, sizeof b,
                         "K4 a session leader's O_NOCTTY open: slave %s, /dev/tty %s; then an "
                         "open without it: slave %s, /dev/tty %s\n",
                         s1 >= 0 ? "ok" : "failed", e1 == 0 ? "ok" : e1 == ENXIO ? "ENXIO" : "other",
                         s2 >= 0 ? "ok" : "failed", e2 == 0 ? "ok" : e2 == ENXIO ? "ENXIO" : "other");
        if (write(p[1], b, (size_t)n) < 0) _exit(3);
        (void)m;
        _exit(0);
    }
    close(p[1]);
    char b[256];
    ssize_t k = read(p[0], b, sizeof b - 1);
    b[k > 0 ? k : 0] = 0;
    printf("%s", k > 0 ? b : "K4 no answer\n");
    waitpid(c, NULL, 0);
    close(p[0]);
}

int main(void) {
    setvbuf(stdout, NULL, _IOLBF, 0);
    k1();
    k23("K2 the child kept the inherited slave until after its own open:", 1);
    k23("K3 the child closed the inherited slave first:", 0);
    k4();
    printf("END\n");
    return 0;
}
