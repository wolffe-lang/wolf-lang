/* On aarch64 Linux `char` is unsigned; a program that assumed
   otherwise compiles on both and behaves on one. */
char first_char(const char *s);
signed char explicit_signed(signed char c);
unsigned char explicit_unsigned(unsigned char c);
