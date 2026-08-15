/* `long double` has no single width or format; rounding it to f64
   would compile and lose precision at the seam. */
long double precise(long double x);
double ordinary_double(double x);
float ordinary_float(float x);
