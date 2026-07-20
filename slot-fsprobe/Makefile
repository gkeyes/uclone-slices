HOST_CC ?= cc
TARGET_PROFILE ?= slotprobe
ifneq ($(filter $(TARGET_PROFILE),slotprobe fitness generic),$(TARGET_PROFILE))
$(error TARGET_PROFILE must be slotprobe, fitness, or generic)
endif
REPO_ROOT := $(abspath $(CURDIR)/..)
PROFILE_OUTPUT := $(CURDIR)/build/generated/$(TARGET_PROFILE)
PROFILE_DIR := $(PROFILE_OUTPUT)/target-profile-$(TARGET_PROFILE)
PROFILE_HEADER := $(PROFILE_DIR)/target_profile.h
PROFILE_FILE := $(REPO_ROOT)/slot-targets/$(TARGET_PROFILE).toml
PROFILE_RENDERER := $(REPO_ROOT)/tools/render-target-profile.sh
HOST_CFLAGS := -std=c11 -O2 -Wall -Wextra -Werror -Wpedantic -Wshadow \
	-Wformat=2 -Wstrict-prototypes -Wmissing-prototypes -fstack-protector-strong \
	-I$(PROFILE_DIR)
HOST_TEST := build/host/$(TARGET_PROFILE)/slot-fsprobe-tests
HOST_SOURCES := tests/test_main.c src/path_policy.c src/sha256.c
ANDROID_NDK_HOME ?= $(ANDROID_SDK_ROOT)/ndk/29.0.14206865
ANDROID_OUT := $(CURDIR)/build/android/$(TARGET_PROFILE)

.PHONY: all test android-check clean

all: test

$(PROFILE_HEADER): $(PROFILE_RENDERER) $(PROFILE_FILE)
	@mkdir -p $(PROFILE_OUTPUT)
	$(PROFILE_RENDERER) $(TARGET_PROFILE) $(PROFILE_OUTPUT)

$(HOST_TEST): $(HOST_SOURCES) src/path_policy.h src/sha256.h $(PROFILE_HEADER)
	@mkdir -p $(dir $@)
	$(HOST_CC) $(HOST_CFLAGS) -Isrc $(HOST_SOURCES) -o $@

test: $(HOST_TEST)
	./$(HOST_TEST)

android-check: $(PROFILE_HEADER)
	@test -x "$(ANDROID_NDK_HOME)/ndk-build" || \
		{ echo "Android NDK not found; set ANDROID_NDK_HOME" >&2; exit 1; }
	@grep -Eq '^Pkg.Revision = 29\.' "$(ANDROID_NDK_HOME)/source.properties" || \
		{ echo "Android NDK 29 is required" >&2; exit 1; }
	"$(ANDROID_NDK_HOME)/ndk-build" \
		NDK_PROJECT_PATH="$(CURDIR)" \
		APP_BUILD_SCRIPT="$(CURDIR)/Android.mk" \
		NDK_APPLICATION_MK="$(CURDIR)/Application.mk" \
		NDK_OUT="$(ANDROID_OUT)/obj" \
		NDK_LIBS_OUT="$(ANDROID_OUT)/bin" \
		UCLONE_TARGET_INCLUDE="$(PROFILE_DIR)" \
		NDK_DEBUG=0

clean:
	rm -rf build
