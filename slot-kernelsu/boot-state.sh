#!/system/bin/sh

# Shared, side-effect-free classification for the fixed slotctl protocol frame.
# This file is sourced only after its root ownership and mode are verified.

classify_reconcile_frame() {
    frame="$1"
    package_field="\"package\":\"$UCLONE_TARGET_PACKAGE\""
    case "$frame" in
        *'"status":"ok"'*'"kind":"reconcile_report"'*"$package_field"*) ;;
        *) printf '%s\n' invalid; return 0 ;;
    esac
    case "$frame" in
        *'"outcome":{"kind":"locked"}'*|*'"outcome":{"kind":"held"}'*)
            printf '%s\n' locked
            ;;
        *'"outcome":{"kind":"restored_base"}'*|*'"outcome":{"kind":"restored_slot"'*|\
        *'"outcome":{"kind":"rolled_back"}'*|*'"outcome":{"kind":"rolled_forward"}'*|\
        *'"outcome":{"kind":"recovery_required"}'*|*'"outcome":{"kind":"quarantined"}'*)
            printf '%s\n' terminal
            ;;
        *)
            printf '%s\n' invalid
            ;;
    esac
}

reconcile_marker_action() {
    case "$1" in
        terminal) printf '%s\n' retire ;;
        locked|invalid) printf '%s\n' retain ;;
        *) printf '%s\n' retain ;;
    esac
}
