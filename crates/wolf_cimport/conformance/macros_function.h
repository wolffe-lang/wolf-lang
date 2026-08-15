/* Function-like macros stay ALIVE: the tokens are recorded so the
   worker can re-expand them at a wolf call site. */
#define SAMPLE_SET(d, s) ((s)->bits |= (1 << (d)))
#define MAX(a, b) ((a) > (b) ? (a) : (b))
#define LOW_BYTE(x) ((x) & 0xff)
int has_bits(void);
