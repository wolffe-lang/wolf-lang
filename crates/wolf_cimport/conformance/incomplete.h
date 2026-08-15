/* Declared, never defined. Pointers to it are fine; its members are
   not, because there are none to know about. */
struct opaque_handle *handle_open(const char *name);
void handle_close(struct opaque_handle *h);
