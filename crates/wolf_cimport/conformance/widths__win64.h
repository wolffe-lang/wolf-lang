/* The same declarations imported for windows: `long` is 32 bits under
   LLP64, and getting that wrong is the classic cross-compile bug. */
struct widths {
    long l;
    unsigned bits : 5;
};

long width_of(long x);
unsigned long unsigned_width(unsigned long x);
