/* Plain records, with offsets resolved for the target. */
struct point {
    int x;
    int y;
};

struct nested {
    struct point origin;
    unsigned char tag;
    size_t count;
};

int distance(struct point *a, struct point *b);
