#ifndef SLOT_FSPROBE_SHA256_H
#define SLOT_FSPROBE_SHA256_H

#include <stddef.h>
#include <stdint.h>

#define SHA256_DIGEST_BYTES ((size_t)32)

struct sha256_context {
    uint32_t state[8];
    uint64_t total_bytes;
    uint8_t block[64];
    size_t block_used;
};

void sha256_init(struct sha256_context *context);
void sha256_update(struct sha256_context *context, const uint8_t *data, size_t length);
void sha256_final(struct sha256_context *context, uint8_t digest[SHA256_DIGEST_BYTES]);

#endif
