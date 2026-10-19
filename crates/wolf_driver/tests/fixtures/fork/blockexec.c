/* s247 (`[os.proc.fork]`, the hub row with an inherited mask) — start a
   program with signal SIG blocked, as a job a shell started with that
   signal blocked would be: block it on this thread, then exec. usage:
   blockexec SIG PROG [ARGS...] */
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

int main(int argc, char **argv) {
  if (argc < 3) {
    fprintf(stderr, "usage: blockexec SIG PROG [ARGS...]\n");
    return 2;
  }
  sigset_t s;
  sigemptyset(&s);
  sigaddset(&s, atoi(argv[1]));
  sigprocmask(SIG_BLOCK, &s, 0);
  execv(argv[2], argv + 2);
  perror("blockexec: execv");
  return 127;
}
