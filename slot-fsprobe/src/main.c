#include "path_policy.h"
#include "sha256.h"

#include <fcntl.h>
#include <linux/fscrypt.h>
#include <stdbool.h>
#include <stdint.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <unistd.h>

enum exit_code {
    EXIT_OK = 0,
    EXIT_CLI = 2,
    EXIT_PATH = 3,
    EXIT_PRIVILEGE = 4,
    EXIT_OPEN = 5,
    EXIT_METADATA = 6,
    EXIT_POLICY = 7,
    EXIT_OUTPUT = 8,
};

#define ANDROID_USER0_APP_UID_MIN ((uid_t)10000)
#define ANDROID_USER0_APP_UID_MAX ((uid_t)19999)

static bool metadata_allowed(const struct stat *metadata) {
    return S_ISDIR(metadata->st_mode) && metadata->st_uid >= ANDROID_USER0_APP_UID_MIN &&
           metadata->st_uid <= ANDROID_USER0_APP_UID_MAX &&
           metadata->st_gid == (gid_t)metadata->st_uid &&
           (metadata->st_mode & (S_IWGRP | S_IWOTH)) == 0;
}

static bool policy_shape_allowed(const struct fscrypt_get_policy_ex_arg *argument) {
    if (argument->policy.version == FSCRYPT_POLICY_V1) {
        return argument->policy_size == sizeof(argument->policy.v1);
    }
    if (argument->policy.version == FSCRYPT_POLICY_V2) {
        return argument->policy_size == sizeof(argument->policy.v2);
    }
    return false;
}

static void encode_little_endian_u64(uint64_t value, uint8_t output[8]) {
    for (size_t index = 0U; index < 8U; ++index) {
        output[index] = (uint8_t)(value >> (index * 8U));
    }
}

static void encode_hex(const uint8_t digest[SHA256_DIGEST_BYTES], char output[65]) {
    static const char HEX[] = "0123456789abcdef";
    for (size_t index = 0U; index < SHA256_DIGEST_BYTES; ++index) {
        output[index * 2U] = HEX[digest[index] >> 4U];
        output[(index * 2U) + 1U] = HEX[digest[index] & 0x0fU];
    }
    output[64] = '\n';
}

static bool write_all(int descriptor, const char *data, size_t length) {
    size_t offset = 0U;
    while (offset < length) {
        const ssize_t written = write(descriptor, data + offset, length - offset);
        if (written <= 0) {
            return false;
        }
        offset += (size_t)written;
    }
    return true;
}

int main(int argc, char *argv[]) {
    struct fscrypt_get_policy_ex_arg argument;
    struct sha256_context hash;
    struct stat metadata;
    uint8_t digest[SHA256_DIGEST_BYTES];
    uint8_t encoded_size[8];
    char output[65];

    if (argc != 3 || strcmp(argv[1], "policy") != 0) {
        return EXIT_CLI;
    }
    if (!fsprobe_path_allowed(argv[2])) {
        return EXIT_PATH;
    }
    if (geteuid() != 0) {
        return EXIT_PRIVILEGE;
    }
    const int descriptor = open(argv[2], O_RDONLY | O_DIRECTORY | O_CLOEXEC | O_NOFOLLOW);
    if (descriptor < 0) {
        return EXIT_OPEN;
    }
    if (fstat(descriptor, &metadata) != 0 || !metadata_allowed(&metadata)) {
        (void)close(descriptor);
        return EXIT_METADATA;
    }

    memset(&argument, 0, sizeof(argument));
    argument.policy_size = sizeof(argument.policy);
    if (ioctl(descriptor, FS_IOC_GET_ENCRYPTION_POLICY_EX, &argument) != 0 ||
        !policy_shape_allowed(&argument)) {
        (void)close(descriptor);
        return EXIT_POLICY;
    }
    if (close(descriptor) != 0) {
        return EXIT_POLICY;
    }

    encode_little_endian_u64(argument.policy_size, encoded_size);
    sha256_init(&hash);
    sha256_update(&hash, encoded_size, sizeof(encoded_size));
    sha256_update(&hash, &argument.policy.version, (size_t)argument.policy_size);
    sha256_final(&hash, digest);
    encode_hex(digest, output);
    return write_all(STDOUT_FILENO, output, sizeof(output)) ? EXIT_OK : EXIT_OUTPUT;
}
