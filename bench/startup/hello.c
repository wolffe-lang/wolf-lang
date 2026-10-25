/* s265 (wolf-lang#918): corpus/hello.lu's C twin for bench-gates' start-up
 * row. Built by the host's clang -O2; its system calls are the C library's
 * and the loader's, which the wolf hello pays too, so the difference is
 * the wolf runtime's own start-up and exit. */
#include <stdio.h>

int main(void) {
    puts("hello, wolf");
    return 0;
}
