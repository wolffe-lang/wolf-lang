/* `int f()` promises nothing about its parameters. Believing it has
   none is how a call goes wrong. */
int ancient();
int modern(void);
int prototyped(int a, int b);
