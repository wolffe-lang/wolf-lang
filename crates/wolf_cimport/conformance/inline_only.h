/* A header-only inline has no symbol either. bindgen's fatal gap is
   discovering this at link time after a long build. */
static inline int twice(int x);
static inline unsigned clamp(unsigned v, unsigned hi);
int ordinary(int x);
