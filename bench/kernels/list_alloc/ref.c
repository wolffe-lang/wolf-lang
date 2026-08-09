/* list_alloc: build, sum, and free a 10k-node linked list per op.
 * The allocation-discipline kernel: malloc-per-node is the C baseline the
 * wolf region allocator (arena bump, wholesale free) intends to beat (D12).
 * Protocol: argv[1]=ops; prints {"ns":<total>,"ops":<ops>}. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

#define NODES 10000

struct node { uint64_t v; struct node *next; };

volatile uint64_t sink;

int main(int argc, char **argv) {
    uint64_t ops = argc > 1 ? strtoull(argv[1], 0, 10) : 100;
    struct timespec t0, t1;
    clock_gettime(CLOCK_MONOTONIC, &t0);
    for (uint64_t k = 0; k < ops; k++) {
        struct node *head = 0;
        for (int i = 0; i < NODES; i++) {
            struct node *n = malloc(sizeof *n);
            n->v = (uint64_t)i;
            n->next = head;
            head = n;
        }
        uint64_t sum = 0;
        for (struct node *n = head; n; n = n->next) sum += n->v;
        sink = sum;
        while (head) {
            struct node *next = head->next;
            free(head);
            head = next;
        }
    }
    clock_gettime(CLOCK_MONOTONIC, &t1);
    uint64_t ns = (uint64_t)(t1.tv_sec - t0.tv_sec) * 1000000000ull
                + (uint64_t)(t1.tv_nsec - t0.tv_nsec);
    printf("{\"ns\":%llu,\"ops\":%llu}\n",
           (unsigned long long)ns, (unsigned long long)ops);
    return 0;
}
