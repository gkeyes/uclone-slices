package com.uclone.slices.v2.gate;

import java.util.HashSet;
import java.util.Set;

/** Visit state belongs to system task events, never an elapsed-time guess. */
final class LaunchSessions {
    private String owner;
    private int ownerUid = -1;
    private String foreground;
    private final Set<Integer> temporaryUids = new HashSet<>();
    private final Set<Integer> proposedTemporaryUids = new HashSet<>();

    synchronized void manual(String packageName, int uid) {
        owner = packageName;
        ownerUid = uid;
        foreground = packageName;
        temporaryUids.clear();
        proposedTemporaryUids.clear();
    }

    synchronized void requested(String target, int targetUid, int callerUid, boolean resultToOwner) {
        if (owner == null || owner.equals(target)) return;
        if (relatedCaller(callerUid) || resultToOwner) {
            proposedTemporaryUids.add(targetUid);
        } else {
            // A user opening a previously used payment/camera App independently ends
            // the old visit once that App actually resumes.
            proposedTemporaryUids.remove(targetUid);
            temporaryUids.remove(targetUid);
        }
    }

    synchronized boolean continues(String target, int callerUid, boolean taskResume) {
        if (!target.equals(owner)) return false;
        if (relatedCaller(callerUid)) return true;
        return taskResume && (target.equals(foreground) || !temporaryUids.isEmpty());
    }

    /** Returns true only after a real independent foreground transition or Home. */
    synchronized boolean resumed(String target, int uid, boolean home, boolean locked) {
        if (locked) return false;
        foreground = target;
        if (home) { end(); return true; }
        if (owner == null || owner.equals(target)) return false;
        if (proposedTemporaryUids.remove(uid)) {
            temporaryUids.add(uid);
            return false;
        }
        if (temporaryUids.contains(uid)) return false;
        end();
        return true;
    }

    synchronized boolean ownedBy(int uid) { return owner != null && ownerUid == uid; }

    synchronized boolean relatedCaller(int uid) {
        return owner != null && (uid == ownerUid || temporaryUids.contains(uid) || proposedTemporaryUids.contains(uid));
    }

    static int effectiveCaller(int creatorUid, int realUid, boolean anchoredSystemResult) {
        return !anchoredSystemResult && realUid >= 0 && creatorUid != realUid ? realUid : creatorUid;
    }

    synchronized void end() {
        owner = null;
        ownerUid = -1;
        temporaryUids.clear();
        proposedTemporaryUids.clear();
    }
}
