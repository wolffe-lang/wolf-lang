/* s290 phase-1 probe: getsid of a child that left the session. */
#include <errno.h>
#include <signal.h>
#include <stdio.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>
int main(void) {
    int p[2]; pipe(p);
    pid_t c = fork();
    if (c == 0) { setsid(); write(p[1], "x", 1); pause(); _exit(0); }
    char b; read(p[0], &b, 1);
    errno = 0;
    pid_t s = getsid(c);
    printf("getsid(child in another session): %s (child pid %s)\n",
           s < 0 ? strerror(errno) : (s == c ? "its pid" : "other"), "-");
    errno = 0;
    s = getsid(1);
    printf("getsid(1): %s\n", s < 0 ? strerror(errno) : "a value");
    kill(c, SIGKILL); waitpid(c, NULL, 0);
    errno = 0;
    s = getsid(c);
    printf("getsid(reaped child): %s\n", s < 0 ? strerror(errno) : "a value");
    return 0;
}
