/* s265 yewpaint: yew's two start-up rows, raw, over several binaries,
 * interleaved (A B A B ..., the order flipped each round), N each.
 * It links sagitta's own PTY observer (tests/support/live_pty.c at the
 * yew pin's sagitta 9ace98c2), so "first paint" is exactly the frame
 * end the yew#55 rows read (yew_live_pty_wait_frame, frame 0):
 *   cold  - yew_live_pty_spawn(bin, fixture.c): latency.c's
 *           measure_cold shape (cold_open_to_first_paint);
 *   clean - argv {bin, --clean, empty.c}: perf_startup.c's clean
 *           profile (startup.first_paint.clean).
 * The clock starts before the PTY is opened (cold) or before fork
 * (clean), as in those harnesses. Each sample's editor is then quit
 * with "\033[27u:q!\r" and reaped; a non-zero exit is a fault (exit 2).
 *   yewpaint MODE N FIXTURE STATE OUT.tsv LABEL=BIN ...           */
#define _POSIX_C_SOURCE 200809L
#include "support/live_pty.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int stop(YewLivePty *pty) {
    static const char quit[] = "\033[27u:q!\r";
    i64 deadline = yew_live_pty_now_ns() + INT64_C(20000000000);
    int code = -1;
    if (!yew_live_pty_write(pty, quit, sizeof quit - 1U, deadline)) return 0;
    if (!yew_live_pty_wait_exit(pty, deadline, &code)) return 0;
    return code == 0;
}

static i64 one(const char *mode, const char *bin, const char *fixture, const char *state) {
    YewLivePty pty = {.master = -1, .pid = -1};
    i64 start = yew_live_pty_now_ns(), done = -1;
    int ok;
    if (!strcmp(mode, "cold")) {
        ok = yew_live_pty_spawn(&pty, bin, fixture, state, 24, 80);
    } else {
        char *const argv[] = {(char *)bin, (char *)"--clean", (char *)fixture, NULL};
        ok = yew_live_pty_spawn_argv(&pty, bin, argv, state, 24, 80);
    }
    ok = ok && yew_live_pty_wait_frame(&pty, 0U, start + INT64_C(3000000000), &done);
    ok = ok && stop(&pty);
    yew_live_pty_close(&pty);
    if (!ok || done < start) {
        fprintf(stderr, "yewpaint: %s %s: no paint or bad exit\n", mode, bin);
        exit(2);
    }
    return done - start;
}

int main(int argc, char **argv) {
    if (argc < 7) return 2;
    const char *mode = argv[1], *fixture = argv[3], *state = argv[4];
    int n = atoi(argv[2]), nb = argc - 6;
    FILE *out = fopen(argv[5], "w");
    if (!out || n < 1) return 2;
    fprintf(out, "mode\tlabel\trun\tns\n");
    for (int r = -2; r < n; ++r) /* two unrecorded warm-up rounds */
        for (int jj = 0; jj < nb; ++jj) {
            int j = (r & 1) ? nb - 1 - jj : jj;
            char *eq = strchr(argv[6 + j], '=');
            if (!eq) return 2;
            i64 ns = one(mode, eq + 1, fixture, state);
            if (r >= 0) fprintf(out, "%s\t%.*s\t%d\t%lld\n", mode, (int)(eq - argv[6 + j]), argv[6 + j], r, (long long)ns);
        }
    return fclose(out) ? 2 : 0;
}
