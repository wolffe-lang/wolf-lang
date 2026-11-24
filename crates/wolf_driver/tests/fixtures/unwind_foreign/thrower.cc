// unwind_foreign_lanes.rs: a C++ function with C linkage that throws an int.
extern "C" int foreign_thrower(int x) {
    if (x != 0) {
        throw x;
    }
    return 0;
}
