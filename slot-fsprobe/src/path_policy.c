#include "path_policy.h"
#include "target_profile.h"

#include <ctype.h>
#include <string.h>

#define CANONICAL_CE_ROOT "/data/user/0/"
#define CANONICAL_DE_ROOT "/data/user_de/0/"
#define SLOT_CE_ROOT UCLONE_CE_SLOT_ROOT "/"
#define SLOT_DE_ROOT UCLONE_DE_SLOT_ROOT "/"
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

static bool package_segment(const char *value, size_t length) {
    if (length == 0U || !isalpha((unsigned char)value[0])) {
        return false;
    }
    for (size_t index = 1U; index < length; ++index) {
        const unsigned char byte = (unsigned char)value[index];
        if (!isalnum(byte) && byte != '_') {
            return false;
        }
    }
    return true;
}

static bool package_allowed(const char *value, size_t length) {
    if (length == 0U || length > 255U) {
        return false;
    }
    size_t start = 0U;
    size_t segments = 0U;
    for (size_t index = 0U; index <= length; ++index) {
        if (index != length && value[index] != '.') {
            continue;
        }
        if (!package_segment(value + start, index - start)) {
            return false;
        }
        ++segments;
        start = index + 1U;
    }
    return segments >= 2U;
}

static bool slot_id_allowed(const char *value, size_t length) {
    if (length == 0U || length > 64U || value[0] < 'a' || value[0] > 'z') {
        return false;
    }
    for (size_t index = 1U; index < length; ++index) {
        const char byte = value[index];
        if ((byte < 'a' || byte > 'z') && (byte < '0' || byte > '9') &&
            byte != '-' && byte != '_') {
            return false;
        }
    }
    return true;
}

static bool slot_component_allowed(const char *value, size_t length) {
    const size_t suffix = sizeof(STAGING_SUFFIX) - 1U;
    if (slot_id_allowed(value, length)) {
        return true;
    }
    return length > suffix + 1U && value[0] == '.' &&
           memcmp(value + length - suffix, STAGING_SUFFIX, suffix) == 0 &&
           slot_id_allowed(value + 1U, length - suffix - 1U);
}

static bool canonical_allowed(const char *path, size_t length, const char *root,
                              size_t root_length) {
    return length > root_length && memcmp(path, root, root_length) == 0 &&
           memchr(path + root_length, '/', length - root_length) == NULL &&
           package_allowed(path + root_length, length - root_length);
}

static bool slot_path_allowed(const char *path, size_t length, const char *root,
                              size_t root_length) {
    if (length <= root_length || memcmp(path, root, root_length) != 0) {
        return false;
    }
    const char *package = path + root_length;
    const char *separator = memchr(package, '/', length - root_length);
    if (separator == NULL || !package_allowed(package, (size_t)(separator - package))) {
        return false;
    }
    const char *slot = separator + 1;
    const size_t slot_length = length - (size_t)(slot - path);
    return memchr(slot, '/', slot_length) == NULL && slot_component_allowed(slot, slot_length);
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
    return canonical_allowed(path, length, CANONICAL_CE_ROOT,
                             sizeof(CANONICAL_CE_ROOT) - 1U) ||
           canonical_allowed(path, length, CANONICAL_DE_ROOT,
                             sizeof(CANONICAL_DE_ROOT) - 1U) ||
           slot_path_allowed(path, length, SLOT_CE_ROOT, sizeof(SLOT_CE_ROOT) - 1U) ||
           slot_path_allowed(path, length, SLOT_DE_ROOT, sizeof(SLOT_DE_ROOT) - 1U);
}
