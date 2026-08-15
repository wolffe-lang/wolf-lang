/* Enum constants carry the values the target resolved them to. */
enum color {
    RED,
    GREEN,
    BLUE = 7,
    NEXT
};

enum flags_e { NONE = 0, ONE = 1, TWO = 2, BOTH = 3 };

int paint(enum color c);
