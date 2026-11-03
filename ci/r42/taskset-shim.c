/* r42's container-only shim: preserve dispositions and closed standard fds.
 * The test rig requests 0-3 (four constrained cores), or 0 (one core).
 * All other arguments go to the real taskset, which validates them.
 */
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

int main(int argc, char **argv) {
    if (argc > 2 && (!strcmp(argv[1], "-c") || !strcmp(argv[1], "--cpu-list"))) {
        const char *replacement = NULL;
        if (!strcmp(argv[2], "0-3")) replacement = getenv("R42_CPU_FOUR");
        if (!strcmp(argv[2], "0")) replacement = getenv("R42_CPU_ONE");
        if (replacement && strcmp(argv[2], replacement)) {
            const char *log = getenv("R42_TASKSET_LOG");
            if (!log) return 2;
            int fd = open(log, O_WRONLY | O_CREAT | O_APPEND, 0644);
            if (fd < 0) return 2;
            dprintf(fd, "taskset: %s -> %s\n", argv[2], replacement);
            close(fd);
            argv[2] = (char *)replacement;
        }
    }
    argv[0] = "/usr/bin/taskset.real";
    execv(argv[0], argv);
    perror("r42 taskset execv");
    return errno == ENOENT ? 127 : 126;
}
