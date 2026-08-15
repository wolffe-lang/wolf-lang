/* Everything an object-like macro can be that is not a value. */
#define STATEMENT do { side_effect(); } while (0)
#define FRAGMENT ) + 1
#define GUARD
#define REFERS_UNKNOWN (nowhere_declared + 1)
