/* Own black-box C17 caller; no compiler or libc implementation is copied. */
#include <stdarg.h>
#include <stdint.h>
#ifndef FREESTANDING
#include <stdio.h>
#include <string.h>
#endif

extern _Bool s263_layout(void);
extern int64_t s263_sum(int32_t n, ...);
extern int64_t s263_forward(int32_t n, ...);
extern int64_t s263_vfmt(unsigned char *fmt, va_list ap);

static int check_copy(unsigned char *fmt, ...) {
    va_list ap;
    va_start(ap, fmt);
    int64_t got = s263_vfmt(fmt, ap);
    /* vfmt consumes its copy, never C's cursor. */
    int first = va_arg(ap, int);
    va_end(ap);
    return got == -2 + 65535 + 65 + 'F' + 'Z' && first == -2;
}

static int integer_cases(void) {
    if (!s263_layout()) return 8;
    if (s263_sum(0) != 0) return 1;
    if (s263_sum(1, (int64_t)-42) != -42) return 2;
    if (s263_sum(8, (int64_t)1, (int64_t)2, (int64_t)3, (int64_t)4,
                 (int64_t)5, (int64_t)6, (int64_t)7, (int64_t)8) != 36) return 3;
    if (s263_forward(8, (int64_t)1, (int64_t)2, (int64_t)3, (int64_t)4,
                     (int64_t)5, (int64_t)6, (int64_t)7, (int64_t)8) != 36) return 4;
    if (!check_copy((unsigned char *)"F", -2, 65535, 65, (unsigned char *)"Z")) return 5;
    return 0;
}

#ifdef FREESTANDING
/* Program-supplied byte copy for the by-value C aggregate argument;
   no libc implementation or archive is used. */
void *memcpy(void *destination, const void *source, unsigned long count) {
    unsigned char *out = destination;
    const unsigned char *in = source;
    for (unsigned long i = 0; i < count; ++i) out[i] = in[i];
    return destination;
}
/* The real none-target object uses its documented trap hook. */
_Noreturn void wolf_trap(int kind, const unsigned char *file, int64_t len,
                        int64_t line, int64_t col) {
    (void)kind; (void)file; (void)len; (void)line; (void)col;
    __asm__ volatile("syscall" : : "a"(231L), "D"(134L) : "rcx", "r11", "memory");
    __builtin_unreachable();
}
int main(void) { return integer_cases(); }
#else
extern double s263_floats(double seed, int32_t n, ...);
extern int32_t s263_outgoing(unsigned char *, uint64_t, unsigned char *, unsigned char *);
int s263_harness(void) {
    int status = integer_cases();
    if (status) return status;
    if (s263_floats(0.5, 9, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0) != 45.5) return 6;
    const char *fmt = "%d %d %d %d %d %d %d %u %lld %llu %s | %.2f %.2f %.2f %.2f %.2f %.2f %.2f %.2f %.2f %.2f";
    char expected[512], actual[512];
    int n = snprintf(expected, sizeof expected, fmt, -7, -300, 250, 65000, 1, 65,
                     -1234, 4000000000U, -5000000000LL, 9000000000ULL, "wolf",
                     1.25, 2.5, 3.5, 4.5, 5.5, 6.5, 7.5, 8.5, 9.5, 10.5);
    int m = s263_outgoing((unsigned char *)actual, sizeof actual,
                          (unsigned char *)fmt, (unsigned char *)"wolf");
    if (m != n || strcmp(actual, expected)) { fprintf(stderr, "expected %s\nactual   %s\n", expected, actual); return 7; }
    puts("source variadic ABI ok");
    return 0;
}
#endif
