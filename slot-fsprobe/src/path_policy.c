#include "path_policy.h"
#include "target_profile.h"

#include <string.h>

#define CANONICAL_CE UCLONE_TARGET_CE
#define CANONICAL_DE UCLONE_TARGET_DE
#define SLOT_CE UCLONE_CE_SLOT_ROOT "/" UCLONE_TARGET_PACKAGE "/"
#define SLOT_DE UCLONE_DE_SLOT_ROOT "/" UCLONE_TARGET_PACKAGE "/"
#define STAGING_PREFIX '.'
#define STAGING_SUFFIX ".staging"

static bool normalized_absolute(const char *path, size_t length) {
    size_t segment_start = 1U;

    if (length < 2U || path[0] != '/' || path[length - 1U] == '/') {
        return false;
    }
    for (size_t index = 1U; index <= length; ++index) {
        if (index != length && path[index] != '/') {
            continue;
        }
        const size_t segment_length = index - segment_start;
        if (segment_length == 0U ||
            (segment_length == 1U && path[segment_start] == '.') ||
            (segment_length == 2U && path[segment_start] == '.' &&
             path[segment_start + 1U] == '.')) {
            return false;
        }
        segment_start = index + 1U;
    }
    return true;
}

static bool slot_component_allowed(const char *component, size_t length) {
    const size_t preview_length = sizeof(UCLONE_PREVIEW_SLOT) - 1U;
    const size_t suffix_length = sizeof(STAGING_SUFFIX) - 1U;

    if (length == preview_length &&
        memcmp(component, UCLONE_PREVIEW_SLOT, preview_length) == 0) {
        return true;
    }
    return length == 1U + preview_length + suffix_length &&
           component[0] == STAGING_PREFIX &&
           memcmp(component + 1U, UCLONE_PREVIEW_SLOT, preview_length) == 0 &&
           memcmp(component + 1U + preview_length, STAGING_SUFFIX, suffix_length) == 0;
}

static bool below_slot_root(const char *path, size_t length, const char *prefix,
                            size_t prefix_length) {
    if (length <= prefix_length || memcmp(path, prefix, prefix_length) != 0) {
        return false;
    }
    const char *component = path + prefix_length;
    const size_t component_length = length - prefix_length;
    return memchr(component, '/', component_length) == NULL &&
           slot_component_allowed(component, component_length);
}

bool fsprobe_path_allowed(const char *path) {
    if (path == NULL) {
        return false;
    }
    const size_t length = strnlen(path, FSPROBE_MAX_PATH_BYTES + 1U);
    if (length == 0U || length > FSPROBE_MAX_PATH_BYTES ||
        !normalized_absolute(path, length)) {
        return false;
    }
    if ((length == sizeof(CANONICAL_CE) - 1U && strcmp(path, CANONICAL_CE) == 0) ||
        (length == sizeof(CANONICAL_DE) - 1U && strcmp(path, CANONICAL_DE) == 0)) {
        return true;
    }
    return below_slot_root(path, length, SLOT_CE, sizeof(SLOT_CE) - 1U) ||
           below_slot_root(path, length, SLOT_DE, sizeof(SLOT_DE) - 1U);
}
