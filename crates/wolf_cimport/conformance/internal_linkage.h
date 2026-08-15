/* `static` at file scope has no link-time symbol: importing it as
   callable would compile and fail to link. */
static int helper(int x);
extern int shared_counter;
int public_entry(int x);
