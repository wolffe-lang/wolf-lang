/* s272: black-box probe of pseudo-terminal pairs opened by 8 threads at once.
 * usage: ptyprobe <mode> <iters>
 *   mode 0: no lock
 *   mode 1: lock around ptsname + copy
 *   mode 2: lock around posix_openpt .. open(slave)
 *   mode 3: mode 2 + the closes under the lock too
 * Prints the failures of each call by errno, and crossings (a size set on the
 * master that the slave does not read back). */
#define _XOPEN_SOURCE 600
#define _DARWIN_C_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <termios.h>
#include <unistd.h>

static pthread_mutex_t mu = PTHREAD_MUTEX_INITIALIZER;
static int mode, iters;
static int fail_openpt[256], fail_grant[256], fail_unlock[256], fail_name, fail_open[256], crossed;
static pthread_mutex_t count = PTHREAD_MUTEX_INITIALIZER;

static void bump(int *a, int e) { pthread_mutex_lock(&count); a[e & 255]++; pthread_mutex_unlock(&count); }

static void *worker(void *arg) {
  int t = (int)(long)arg;
  for (int i = 0; i < iters; i++) {
    char name[128];
    if (mode >= 2) pthread_mutex_lock(&mu);
    int m = posix_openpt(O_RDWR | O_NOCTTY);
    if (m < 0) { int e = errno; fprintf(stderr, "openpt errno=%d (%s) t=%d i=%d\n", e, strerror(e), t, i); if (mode >= 2) pthread_mutex_unlock(&mu); bump(fail_openpt, e); continue; }
    if (grantpt(m) != 0) { int e = errno; if (mode >= 2) pthread_mutex_unlock(&mu); bump(fail_grant, e); close(m); continue; }
    if (unlockpt(m) != 0) { int e = errno; if (mode >= 2) pthread_mutex_unlock(&mu); bump(fail_unlock, e); close(m); continue; }
    if (mode == 1) pthread_mutex_lock(&mu);
    char *p = ptsname(m);
    if (p) { strncpy(name, p, sizeof name - 1); name[sizeof name - 1] = 0; }
    if (mode == 1) pthread_mutex_unlock(&mu);
    if (!p) { if (mode >= 2) pthread_mutex_unlock(&mu); pthread_mutex_lock(&count); fail_name++; pthread_mutex_unlock(&count); close(m); continue; }
    int s = open(name, O_RDWR | O_NOCTTY);
    if (mode == 2) pthread_mutex_unlock(&mu);
    if (s < 0) { int e = errno; if (mode == 3) pthread_mutex_unlock(&mu); bump(fail_open, e); close(m); continue; }
    if (mode == 3) pthread_mutex_unlock(&mu);
    struct winsize w = {0};
    w.ws_row = (unsigned short)(t * 1000 + i + 1);
    w.ws_col = 7;
    ioctl(m, TIOCSWINSZ, &w);
    struct winsize r = {0};
    if (ioctl(s, TIOCGWINSZ, &r) != 0 || r.ws_row != w.ws_row || r.ws_col != 7) {
      pthread_mutex_lock(&count); crossed++; pthread_mutex_unlock(&count);
    }
    if (mode == 3) pthread_mutex_lock(&mu);
    close(m);
    close(s);
    if (mode == 3) pthread_mutex_unlock(&mu);
  }
  return 0;
}

static void show(const char *what, int *a) {
  for (int e = 0; e < 256; e++)
    if (a[e]) printf(" %s:%s(%d)x%d", what, strerror(e), e, a[e]);
}

int main(int argc, char **argv) {
  mode = atoi(argv[1]);
  iters = atoi(argv[2]);
  pthread_t th[8];
  for (long t = 0; t < 8; t++) pthread_create(&th[t], 0, worker, (void *)t);
  for (int t = 0; t < 8; t++) pthread_join(th[t], 0);
  printf("mode=%d iters=%d crossed=%d name_fail=%d", mode, iters, crossed, fail_name);
  show("openpt", fail_openpt);
  show("grantpt", fail_grant);
  show("unlockpt", fail_unlock);
  show("open", fail_open);
  printf("\n");
  return 0;
}
