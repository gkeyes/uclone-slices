#include "../src/path_policy.h"
#include "../src/sha256.h"
#include "target_profile.h"

#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

static int failures = 0;

static void check(bool condition, const char *name) {
    if (!condition) {
        (void)fprintf(stderr, "FAIL: %s\n", name);
        ++failures;
    }
}

static void digest_hex(const uint8_t *data, size_t length, char output[65]) {
    static const char HEX[] = "0123456789abcdef";
    struct sha256_context context;
    uint8_t digest[SHA256_DIGEST_BYTES];

    sha256_init(&context);
    sha256_update(&context, data, length);
    sha256_final(&context, digest);
    for (size_t index = 0U; index < SHA256_DIGEST_BYTES; ++index) {
        output[index * 2U] = HEX[digest[index] >> 4U];
        output[(index * 2U) + 1U] = HEX[digest[index] & 0x0fU];
    }
    output[64] = '\0';
}

static void check_sha256(const char *input, const char *expected, const char *name) {
    char actual[65];
    digest_hex((const uint8_t *)input, strlen(input), actual);
    check(strcmp(actual, expected) == 0, name);
}

static void test_sha256_vectors(void) {
    check_sha256("", "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                 "sha256 empty");
    check_sha256("abc", "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
                 "sha256 abc");
    check_sha256("abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
                 "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
                 "sha256 multi-block");

    struct sha256_context context;
    uint8_t digest[SHA256_DIGEST_BYTES];
    char actual[65];
    uint8_t a_block[1000];
    memset(a_block, 'a', sizeof(a_block));
    sha256_init(&context);
    for (size_t index = 0U; index < 1000U; ++index) {
        sha256_update(&context, a_block, sizeof(a_block));
    }
    sha256_final(&context, digest);
    static const char HEX[] = "0123456789abcdef";
    for (size_t index = 0U; index < SHA256_DIGEST_BYTES; ++index) {
        actual[index * 2U] = HEX[digest[index] >> 4U];
        actual[(index * 2U) + 1U] = HEX[digest[index] & 0x0fU];
    }
    actual[64] = '\0';
    check(strcmp(actual, "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0") ==
              0,
          "sha256 million a");
}

static void test_allowed_paths(void) {
    static const char *const ALLOWED[] = {
        UCLONE_TARGET_CE,
        UCLONE_TARGET_DE,
        UCLONE_CE_SLOT_ROOT "/" UCLONE_TARGET_PACKAGE "/" UCLONE_PREVIEW_SLOT,
        UCLONE_DE_SLOT_ROOT "/" UCLONE_TARGET_PACKAGE "/" UCLONE_PREVIEW_SLOT,
        UCLONE_CE_SLOT_ROOT "/" UCLONE_TARGET_PACKAGE "/." UCLONE_PREVIEW_SLOT
                             ".staging",
        UCLONE_DE_SLOT_ROOT "/" UCLONE_TARGET_PACKAGE "/." UCLONE_PREVIEW_SLOT
                             ".staging",
    };
    for (size_t index = 0U; index < sizeof(ALLOWED) / sizeof(ALLOWED[0]); ++index) {
        check(fsprobe_path_allowed(ALLOWED[index]), ALLOWED[index]);
    }
}

static void test_rejected_paths(void) {
    static const char *const REJECTED[] = {
        "",
        "data/user/0/" UCLONE_TARGET_PACKAGE,
        "/",
        UCLONE_TARGET_CE "/",
        "/data/user//0/" UCLONE_TARGET_PACKAGE,
        "/data/user/./0/" UCLONE_TARGET_PACKAGE,
        "/data/user/0/../0/" UCLONE_TARGET_PACKAGE,
        "/data/data/" UCLONE_TARGET_PACKAGE,
        "/data/user/1/" UCLONE_TARGET_PACKAGE,
        UCLONE_TARGET_CE ".other",
        UCLONE_TARGET_CE "/files",
        "/data/misc_ce/0/uclone-slot-lab/slots/" UCLONE_TARGET_PACKAGE "/preview",
        "/data/misc_ce/0/uclone-slices-preview/slots/com.other/preview",
        UCLONE_CE_SLOT_ROOT "/" UCLONE_TARGET_PACKAGE,
        UCLONE_CE_SLOT_ROOT "/" UCLONE_TARGET_PACKAGE "/",
        UCLONE_CE_SLOT_ROOT "/" UCLONE_TARGET_PACKAGE "/Preview",
        UCLONE_CE_SLOT_ROOT "/" UCLONE_TARGET_PACKAGE "/0preview",
        UCLONE_CE_SLOT_ROOT "/" UCLONE_TARGET_PACKAGE "/pre.view",
        UCLONE_CE_SLOT_ROOT "/" UCLONE_TARGET_PACKAGE "/foreign",
        UCLONE_CE_SLOT_ROOT "/" UCLONE_TARGET_PACKAGE "/.foreign.staging",
        UCLONE_CE_SLOT_ROOT "/" UCLONE_TARGET_PACKAGE "/.staging",
        UCLONE_CE_SLOT_ROOT "/" UCLONE_TARGET_PACKAGE "/.Preview.staging",
        UCLONE_CE_SLOT_ROOT "/" UCLONE_TARGET_PACKAGE "/.preview",
        UCLONE_CE_SLOT_ROOT "/" UCLONE_TARGET_PACKAGE "/preview/files",
        UCLONE_CE_SLOT_ROOT "/" UCLONE_TARGET_PACKAGE "/../preview",
        (UCLONE_DE_SLOT_ROOT "/" UCLONE_TARGET_PACKAGE "/"
         "abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklm"),
    };
    check(!fsprobe_path_allowed(NULL), "null path");
    for (size_t index = 0U; index < sizeof(REJECTED) / sizeof(REJECTED[0]); ++index) {
        check(!fsprobe_path_allowed(REJECTED[index]), REJECTED[index]);
    }

    char overlong[FSPROBE_MAX_PATH_BYTES + 2U];
    memset(overlong, 'a', sizeof(overlong));
    overlong[0] = '/';
    overlong[sizeof(overlong) - 1U] = '\0';
    check(!fsprobe_path_allowed(overlong), "overlong path");
}

int main(void) {
    test_sha256_vectors();
    test_allowed_paths();
    test_rejected_paths();
    if (failures != 0) {
        return 1;
    }
    (void)puts("PASS: SHA-256 vectors and fixed-path validation");
    return 0;
}
