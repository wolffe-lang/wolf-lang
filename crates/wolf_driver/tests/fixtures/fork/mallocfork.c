/* s247 (`[os.proc.fork.note]`, the inventory's allocator row) — a control,
   black box: does the HOST C library's allocator survive a fork made while
   other threads are inside it? The runtime does not lean on the answer (it
   forks only with every other thread of its own at rest, none in the
   allocator), so this measures what a protocol without the hold would have
   stood on. Three threads malloc and free without pause; the main thread
   forks N times; each child mallocs and frees 1000 blocks and _exits 0; the
   parent waits for each up to 2 s and kills by its recorded pid one that
   has not ended (a child stuck on a lock a vanished thread held), then
   reaps it. Prints the counts. usage: mallocfork N */
#include <pthread.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

static void *churn(void *arg) {
  unsigned s = (unsigned)(size_t)arg;
  for (;;) {
    s = s * 1103515245u + 12345u;
    void *p = malloc(16 + (s >> 20) % 4096);
    free(p);
  }
  return 0;
}

int main(int argc, char **argv) {
  int n = argc > 1 ? atoi(argv[1]) : 200;
  for (int i = 0; i < 3; i++) {
    pthread_t t;
    pthread_create(&t, 0, churn, (void *)(size_t)(i + 1));
  }
  int ok = 0, stuck = 0, other = 0;
  for (int i = 0; i < n; i++) {
    pid_t p = fork();
    if (p == 0) {
      for (int k = 0; k < 1000; k++) {
        void *q = malloc(32 + k);
        free(q);
      }
      _exit(0);
    }
    if (p < 0) { other++; continue; }
    int st = 0, done = 0;
    for (int w = 0; w < 200; w++) {
      if (waitpid(p, &st, WNOHANG) == p) { done = 1; break; }
      struct timespec ts = {0, 10 * 1000 * 1000};
      nanosleep(&ts, 0);
    }
    if (!done) {
      kill(p, SIGKILL);
      waitpid(p, &st, 0);
      stuck++;
    } else if (WIFEXITED(st) && WEXITSTATUS(st) == 0) {
      ok++;
    } else {
      other++;
    }
  }
  printf("mallocfork: %d forks while 3 threads malloc: %d children ran their own malloc and ended 0, %d stuck (killed at 2 s), %d other\n", n, ok, stuck, other);
  return 0;
}
