/* kw02 (wolf-lang#521): the C callees wolf reaches through bodyless `extern "c" fn`. */
#include <stdbool.h>
#include <stdint.h>
#include <string.h>

struct Big { int8_t a; int64_t b; int16_t c; double d; uint8_t e; };
struct Mix { int32_t i; float f; };
struct Two { double x; double y; };
struct C3 { uint8_t a; uint32_t b; uint8_t c; };

int64_t kc_ints(int8_t a, uint8_t b, int16_t c, uint16_t d, int32_t e, uint32_t f, int64_t g, uint64_t h) {
    return a + 2 * (int64_t)b + 3 * (int64_t)c + 4 * (int64_t)d + 5 * (int64_t)e + 6 * (int64_t)f
        + 7 * g + 8 * (int64_t)h;
}
double kc_floats(float a, double b, float c, double d, float e, double f, float g, double h, double i) {
    return (double)a + 2.0 * b + 3.0 * (double)c + 4.0 * d + 5.0 * (double)e + 6.0 * f + 7.0 * (double)g
        + 8.0 * h + 9.0 * i;
}
int8_t kc_neg8(int8_t x) { return (int8_t)-x; }
bool kc_pos(int64_t x) { return x > 0; }
struct Big kc_big_make(int64_t x) { struct Big r = { -7, x, -300, 4.5, 200 }; return r; }
int64_t kc_big_sum(struct Big r) { return r.a + 2 * r.b + 3 * (int64_t)r.c + 5 * (int64_t)r.e; }
struct Mix kc_mix_make(int32_t i, float f) { struct Mix m = { i + 1, f * 2.0f }; return m; }
struct Two kc_two_make(double x) { struct Two t = { x, x + 0.25 }; return t; }
double kc_two_dot(struct Two t, struct Two u) { return t.x * u.x + t.y * u.y; }
struct C3 kc_c3_make(uint8_t a) { struct C3 c = { a, 3735928559u, (uint8_t)(a + 1) }; return c; }
void kc_fill(uint8_t *p, int64_t n) { for (int64_t i = 0; i < n; i++) p[i] = (uint8_t)(3 * i + 1); }
void kc_big_write(struct Big *p, int64_t x) { struct Big r = { 5, x, -2, 0.5, 250 }; *p = r; }
int64_t kc_strlen(const uint8_t *s) { return (int64_t)strlen((const char *)s); }
void kc_nothing(void) {}
