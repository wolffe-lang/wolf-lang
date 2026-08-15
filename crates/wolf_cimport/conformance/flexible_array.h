/* The tail's length lives in a sibling field, by convention, not in
   the type. */
struct packet {
    unsigned length;
    unsigned char payload[];
};

void send_packet(struct packet *p);
