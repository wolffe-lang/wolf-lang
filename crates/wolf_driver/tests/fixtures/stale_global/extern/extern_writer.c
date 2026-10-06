/* s214 (wolf-lang#598): the C half of extern_writer.lu. Each function
   writes memory the wolf caller reads right after the call. */
#include <stdint.h>

int64_t s214_cell[1];

void s214_poke(int64_t v) { s214_cell[0] = v; }

void s214_bump(void);

void s214_call_back(void) { s214_bump(); }
