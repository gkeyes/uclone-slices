#!/system/bin/sh

uclone_default_profile() {
    UCLONE_TARGET_PROFILE=generic
    UCLONE_TARGET_PACKAGE=com.uclone.slots.preview
    UCLONE_TARGET_USER=0
    UCLONE_BASE_SLOT=base
    UCLONE_PREVIEW_SLOT=preview
    UCLONE_MODULE=uclone-slices-preview
    UCLONE_MODULE_ROOT=/data/adb/modules/uclone-slices-preview
    UCLONE_RUNTIME_ROOT=/data/adb/uclone-slices-preview
}

uclone_load_profile() {
    profile_file="$1"
    uclone_default_profile
    [ -f "$profile_file" ] && [ ! -L "$profile_file" ] || return 1
    owner_mode="$(/system/bin/toybox stat -c '%u:%a' "$profile_file" 2>/dev/null)" || return 1
    [ "$owner_mode" = 0:444 ] || return 1
    profile="$(/system/bin/toybox sed -n "s/^readonly UCLONE_TARGET_PROFILE='\([a-z]*\)'$/\1/p" "$profile_file")"
    package="$(/system/bin/toybox sed -n "s/^readonly UCLONE_TARGET_PACKAGE='\([A-Za-z0-9._]*\)'$/\1/p" "$profile_file")"
    user="$(/system/bin/toybox sed -n "s/^readonly UCLONE_TARGET_USER='\([0-9]*\)'$/\1/p" "$profile_file")"
    case "$profile:$package:$user" in
        generic:com.uclone.slots.preview:0|\
        fitness:com.asksky.fitness:0|\
        slotprobe:com.uclone.slotprobe:0) ;;
        *) uclone_default_profile; return 1 ;;
    esac
    UCLONE_TARGET_PROFILE=$profile
    UCLONE_TARGET_PACKAGE=$package
    UCLONE_TARGET_USER=$user
    return 0
}
