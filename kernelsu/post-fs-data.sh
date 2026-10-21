#!/system/bin/sh

umask 077
MODDIR=${0%/*}
# Enrollment discovery only: no CE access, mount changes, app stops, or app starts.
# This lets system_server reject an uninstrumented configured process during early boot.
"$MODDIR/bin/ucloned" --export-enrollment
