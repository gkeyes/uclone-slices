#!/system/bin/sh

# KernelSU runs this when the module is removed. Only the system_server discovery
# manifest is deleted, so former Apps stop being gated; account data under
# /data/adb/uclone-slices-v2 and /data/misc_*/0/uclone-slices-v2 is kept.
rm -rf /data/system/uclone-slices-v2
