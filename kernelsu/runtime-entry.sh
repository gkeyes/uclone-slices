#!/system/bin/sh

umask 077
MODDIR=$1
INIT_NS=$(readlink /proc/1/ns/mnt) || exit 1
SELF_NS=$(readlink /proc/self/ns/mnt) || exit 1
[ "$SELF_NS" != "$INIT_NS" ] || exit 1
# Incoming Android storage changes remain visible; account mounts cannot propagate out.
/system/bin/mount -o rslave none / || exit 1
"$MODDIR/bin/uclone-namespace-anchor" --start || exit 1
exec "$MODDIR/bin/ucloned"
