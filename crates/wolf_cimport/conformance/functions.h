/* Ordinary prototypes: the happy path the whole thing rests on. */
void *malloc(size_t n);
void *calloc(size_t n, size_t size);
void free(void *p);
void *memset(void *s, int c, size_t n);
void *memcpy(void *dst, const void *src, size_t n);
int puts(const char *s);
