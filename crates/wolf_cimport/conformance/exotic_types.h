/* Each of these has a wrong-but-plausible mapping that would compile
   and misbehave, so each is refused by its own name. */
int uses_complex(_Complex double z);
int uses_atomic(_Atomic int counter);
int fine(int ordinary);
