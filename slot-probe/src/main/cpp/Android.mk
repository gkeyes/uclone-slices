LOCAL_PATH := $(call my-dir)

include $(CLEAR_VARS)
LOCAL_MODULE := slotprobe_native
LOCAL_SRC_FILES := native_probe.cpp
LOCAL_CPPFLAGS := -std=c++20 -Wall -Wextra -Werror
LOCAL_LDLIBS := -llog
include $(BUILD_SHARED_LIBRARY)
