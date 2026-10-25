/* s266: the test command's entry. Every signal back to its default and an
 * empty mask (ssh, setsid and podman may each hand down an ignored or
 * blocked signal), then exec. No interpreter, no descriptor touched. */
#define _DEFAULT_SOURCE /* NSIG */
#include <signal.h>
#include <stdio.h>
#include <unistd.h>

int main(int argc, char **argv) {
    sigset_t none;
    if (argc < 2) {
        fputs("usage: s266-exec-clean CMD [ARG...]\n", stderr);
        return 2;
    }
    sigemptyset(&none);
    sigprocmask(SIG_SETMASK, &none, NULL);
    for (int s = 1; s < NSIG; ++s) {
        if (s != SIGKILL && s != SIGSTOP) {
            signal(s, SIG_DFL);
        }
    }
    execvp(argv[1], argv + 1);
    perror("s266-exec-clean: execvp");
    return 127;
}
