/* const/volatile/restrict at the seam: volatile is honored,
   restrict is an assertion, const is recorded. */
int read_reg(volatile unsigned *reg);
int compare(const char *a, const char *b);
void fast_copy(char *restrict dst, const char *restrict src, size_t n);
