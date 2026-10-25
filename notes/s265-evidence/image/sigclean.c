/* s265: exec argv[1..] with an empty signal mask and SIGINT, SIGQUIT,
 * SIGPIPE and SIGXFSZ at their defaults (an ssh session can hand them
 * blocked or ignored). The C twin of s264's exec-clean.py. */
#define _POSIX_C_SOURCE 200809L
#include <signal.h>
#include <string.h>
#include <unistd.h>

int main(int argc, char **argv) {
    if (argc < 2) return 2;
    sigset_t none;
    sigemptyset(&none);
    sigprocmask(SIG_SETMASK, &none, 0);
    int sigs[] = {SIGINT, SIGQUIT, SIGPIPE, SIGXFSZ};
    for (unsigned i = 0; i < sizeof sigs / sizeof *sigs; ++i) signal(sigs[i], SIG_DFL);
    execvp(argv[1], argv + 1);
    static const char m[] = "s265 sigclean: exec failed\n";
    (void)!write(2, m, sizeof m - 1);
    return 127;
}
