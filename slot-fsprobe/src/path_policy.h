#ifndef SLOT_FSPROBE_PATH_POLICY_H
#define SLOT_FSPROBE_PATH_POLICY_H

#include <stdbool.h>
#include <stddef.h>

#define FSPROBE_MAX_PATH_BYTES ((size_t)4095)

bool fsprobe_path_allowed(const char *path);

#endif
