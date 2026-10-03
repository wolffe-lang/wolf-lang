/* kw02 (wolf-lang#513): the C side of export.lu. Calls every wolf export by its C prototype and
   checks each value against C's own computation of the same expression.
   Prints one line per check; the last line counts. */
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>

struct Big { int8_t a; int64_t b; int16_t c; double d; uint8_t e; };
struct Mix { int32_t i; float f; };
struct Two { double x; double y; };
struct C3 { uint8_t a; uint32_t b; uint8_t c; };

int64_t kx_ints(int8_t, uint8_t, int16_t, uint16_t, int32_t, uint32_t, int64_t, uint64_t);
double kx_floats(float, double, float, double, float, double, float, double, double);
float kx_floats32(float, double, float, double, float, double, float, double, double);
double kx_mixed(int64_t, double, int32_t, float, uint8_t, double);
float kx_mixed32(int64_t, double, int32_t, float, uint8_t, double);
int8_t kx_neg8(int8_t);
uint16_t kx_inc16(uint16_t);
float kx_half(float);
bool kx_pos(int64_t);
uint64_t kx_u64(uint64_t);
int64_t kx_big_sum(struct Big);
double kx_big_d(struct Big);
struct Big kx_big_make(int64_t);
struct Mix kx_mix_swap(struct Mix);
double kx_two_dot(struct Two, struct Two);
struct Two kx_two_make(double);
struct C3 kx_c3_make(uint8_t);
int64_t kx_c3_sum(struct C3);
int64_t kx_big_ptr(struct Big *);
int64_t kx_strlen(const uint8_t *);

static int ok, bad;
static void chk(const char *name, int good, const char *detail) {
    printf("%s %s%s%s\n", name, good ? "ok" : "BAD", good ? "" : " ", good ? "" : detail);
    if (good) ok++; else bad++;
}
#define CHKI(name, got, want) do { long long g_ = (long long)(got), w_ = (long long)(want); \
    char b_[96]; snprintf(b_, sizeof b_, "got=%lld want=%lld", g_, w_); chk(name, g_ == w_, b_); } while (0)
#define CHKU(name, got, want) do { unsigned long long g_ = (got), w_ = (want); \
    char b_[96]; snprintf(b_, sizeof b_, "got=%llu want=%llu", g_, w_); chk(name, g_ == w_, b_); } while (0)
#define CHKF(name, got, want) do { double g_ = (got), w_ = (want); \
    char b_[96]; snprintf(b_, sizeof b_, "got=%.17g want=%.17g", g_, w_); chk(name, g_ == w_, b_); } while (0)

int32_t kw_c_main(void) {
    CHKI("ints", kx_ints(-5, 250, -300, 60000, -70000, 4000000000u, -5000000000LL, 600000000000ULL),
         -5 + 2 * 250LL + 3 * -300LL + 4 * 60000LL + 5 * -70000LL + 6 * 4000000000LL
             + 7 * -5000000000LL + 8 * 600000000000LL);
    CHKF("floats", kx_floats(0.5f, 1.25, -2.0f, 3.5, 0.75f, -1.5, 8.0f, 0.125, 1024.0),
         1.25 + 2.0 * 3.5 + 3.0 * -1.5 + 4.0 * 0.125 + 5.0 * 1024.0);
    CHKF("floats32", kx_floats32(0.5f, 1.25, -2.0f, 3.5, 0.75f, -1.5, 8.0f, 0.125, 1024.0),
         0.5f + 2.0f * -2.0f + 3.0f * 0.75f + 4.0f * 8.0f);
    CHKF("mixed", kx_mixed(-3, 0.5, 7, 2.5f, 200, 0.25), (double)-3 * 0.5 + (double)7 + (double)200 * 0.25);
    CHKF("mixed32", kx_mixed32(-3, 0.5, 7, 2.5f, 200, 0.25), 5.0f);
    CHKI("neg8", kx_neg8(-100), 100);
    CHKI("neg8b", kx_neg8(5), -5);
    CHKU("inc16", kx_inc16(65000), 65001);
    CHKF("half", kx_half(5.0f), 2.5f);
    CHKI("pos", kx_pos(7), 1);
    CHKI("posn", kx_pos(-7), 0);
    CHKU("u64", kx_u64(4000000000ULL), 12000000000ULL);
    struct Big r = { -7, 1234567890123LL, -300, 4.5, 200 };
    CHKI("big_sum", kx_big_sum(r), -7 + 2 * 1234567890123LL + 3 * -300LL + 5 * 200LL);
    CHKF("big_d", kx_big_d(r), 9.0);
    struct Big m = kx_big_make(99);
    CHKI("big_make.a", m.a, -7);
    CHKI("big_make.b", m.b, 99);
    CHKI("big_make.c", m.c, -300);
    CHKF("big_make.d", m.d, 4.5);
    CHKI("big_make.e", m.e, 200);
    struct Mix x = kx_mix_swap((struct Mix){ 41, 1.5f });
    CHKI("mix.i", x.i, 42);
    CHKF("mix.f", x.f, 3.0f);
    CHKF("two_dot", kx_two_dot((struct Two){ 1.5, -2.0 }, (struct Two){ 4.0, 0.25 }), 1.5 * 4.0 + -2.0 * 0.25);
    struct Two t = kx_two_make(3.0);
    CHKF("two.x", t.x, 3.0);
    CHKF("two.y", t.y, 3.25);
    struct C3 c = kx_c3_make(17);
    CHKI("c3.a", c.a, 17);
    CHKU("c3.b", c.b, 3735928559u);
    CHKI("c3.c", c.c, 18);
    CHKI("c3_sum", kx_c3_sum((struct C3){ 1, 3735928559u, 250 }), 1 + 2 * 3735928559LL + 3 * 250);
    struct Big w = { 10, 1000, 20, 0.5, 30 };
    CHKI("big_ptr.ret", kx_big_ptr(&w), 1000);
    CHKI("big_ptr.a", w.a, 11);
    CHKI("big_ptr.b", w.b, 2000);
    CHKI("big_ptr.c", w.c, 19);
    CHKF("big_ptr.d", w.d, 1.5);
    CHKI("big_ptr.e", w.e, 32);
    CHKI("strlen", kx_strlen((const uint8_t *)"membrane"), 8);
    CHKI("strlen0", kx_strlen((const uint8_t *)""), 0);
    printf("x1 checks ok=%d bad=%d\n", ok, bad);
    return bad == 0 ? 0 : 1;
}
