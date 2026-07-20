LOCAL_PATH := $(call my-dir)

include $(CLEAR_VARS)
LOCAL_MODULE := slot-fsprobe
LOCAL_SRC_FILES := \
    src/main.c \
    src/path_policy.c \
    src/sha256.c
LOCAL_C_INCLUDES := $(LOCAL_PATH)/src $(UCLONE_TARGET_INCLUDE)
LOCAL_CFLAGS := \
    -std=c17 \
    -O2 \
    -Wall \
    -Wextra \
    -Werror \
    -Wpedantic \
    -Wshadow \
    -Wformat=2 \
    -Wstrict-prototypes \
    -Wmissing-prototypes \
    -fstack-protector-strong \
    -D_FORTIFY_SOURCE=2 \
    -fPIE
LOCAL_LDFLAGS := -fPIE -pie -Wl,-z,relro -Wl,-z,now
include $(BUILD_EXECUTABLE)
