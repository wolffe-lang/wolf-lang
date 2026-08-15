/* Variadic calls: recorded as variadic rather than refused here,
   because whether a backend can make the call is a separate question
   from whether wolf can say it. */
int printf(const char *fmt, ...);
int no_varargs(int x);
