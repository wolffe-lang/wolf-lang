/* Bitfield placement is per-target and must be exact (report 06 #8). */
struct flags {
    unsigned first : 1;
    unsigned second : 3;
    unsigned rest : 28;
};

struct mixed {
    unsigned char tag;
    unsigned lo : 4;
    unsigned hi : 4;
};

void take_flags(struct flags *f);
