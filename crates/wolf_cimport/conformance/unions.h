/* A union's live member is a rule the programmer knows and the header
   does not state, so it always demotes to opaque. */
union value {
    int as_int;
    double as_double;
    void *as_ptr;
};

void consume(union value v);
void consume_ptr(union value *v);
