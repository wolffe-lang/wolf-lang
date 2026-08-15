/* C keeps tags and ordinary identifiers in separate name spaces
   (c23-n3220 6.2.3). Wolf has one `c` namespace, so the tag is renamed
   visibly rather than one of them silently winning. */
struct stat {
    size_t st_size;
    unsigned st_mode;
};

int stat(const char *path, struct stat *out);
