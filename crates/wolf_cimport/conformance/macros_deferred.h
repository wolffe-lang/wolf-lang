/* The v0-deferred macro classes, each refused by its own name with
   the inline-C escape. */
#define CONTAINER_OF(ptr, member) ((typeof(member) *)(ptr))
#define GLUE(a, b) a ## b
#define PICK(x) _Generic((x), int: 1, default: 0)
#define BLOCK(x) { use(x); }
