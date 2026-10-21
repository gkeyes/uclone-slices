package com.uclone.slices.v2.gate;

import org.json.JSONArray;
import org.json.JSONObject;

import java.security.SecureRandom;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.function.Consumer;

/** Root-derived policy and attestations. No Root calls, disk waits, or mounts occur here. */
final class LaunchPolicyCache {
    static final String MANAGER = "com.uclone.slices.v2";
    static final String GATE = MANAGER + ".gate.LaunchPreparationActivity";
    static final String PERMIT_EXTRA = MANAGER + ".LAUNCH_PERMIT";
    static final String TICKET_EXTRA = MANAGER + ".LAUNCH_TICKET";
    private final String build;
    private final Consumer<JSONObject> send;
    private final SecureRandom random = new SecureRandom();
    private final LaunchSessions sessions = new LaunchSessions();
    private final Map<String, Integer> enrollments = new HashMap<>();
    private Map<String, JSONObject> packages = new HashMap<>();
    private Map<String, JSONObject> permits = new HashMap<>();
    private Map<Integer, JSONObject> processes = new HashMap<>();
    private final Set<String> consumed = new HashSet<>();
    private final Map<String, String> pending = new HashMap<>();
    private final Set<String> shown = new HashSet<>();
    private boolean connected;
    private boolean valid;
    private long revision = -1;
    private int managerUid = -1;

    LaunchPolicyCache(String build, Consumer<JSONObject> send) {
        this.build = build;
        this.send = send;
    }

    synchronized void managerUid(int uid) { managerUid = uid; }

    synchronized void discovery(JSONObject manifest) {
        if (!build.equals(manifest.optString("build_id"))) return;
        JSONArray list = manifest.optJSONArray("packages");
        if (list == null) return;
        for (int index = 0; index < list.length(); index++) {
            JSONObject row = list.optJSONObject(index);
            if (row != null) enrollments.put(row.optString("package"), row.optInt("uid", -1));
        }
    }

    long update(JSONObject state) throws Exception {
        if (!"state".equals(state.getString("type")) || !build.equals(state.getString("build_id"))) {
            throw new IllegalArgumentException("launch build mismatch");
        }
        long nextRevision = state.getLong("revision");
        Map<String, JSONObject> nextPackages = keyed(state.getJSONArray("packages"), "package");
        Map<String, JSONObject> nextPermits = keyed(state.getJSONArray("permits"), "token");
        Map<Integer, JSONObject> nextProcesses = new HashMap<>();
        JSONArray processRows = state.getJSONArray("processes");
        for (int index = 0; index < processRows.length(); index++) {
            JSONObject process = processRows.getJSONObject(index);
            nextProcesses.put(process.getInt("pid"), process);
        }
        synchronized (this) {
            if (nextRevision <= revision) return revision;
            consumed.retainAll(nextPermits.keySet());
            for (String token : consumed) nextPermits.remove(token);
            packages = nextPackages;
            permits = nextPermits;
            processes = nextProcesses;
            valid = state.getBoolean("valid");
            if (valid) {
                enrollments.clear();
                for (Map.Entry<String, JSONObject> entry : packages.entrySet()) {
                    enrollments.put(entry.getKey(), entry.getValue().getInt("uid"));
                }
            }
            connected = true;
            revision = nextRevision;
            return revision;
        }
    }

    synchronized void disconnected() {
        connected = false;
        valid = false;
        revision = -1;
        permits.clear();
        consumed.clear();
        processes.clear();
        pending.clear();
        shown.clear();
        sessions.end();
    }

    synchronized boolean managed(String packageName) { return enrollments.containsKey(packageName); }

    synchronized boolean managedUid(int uid) { return enrollments.containsValue(uid); }

    synchronized boolean approvedProcess(String packageName, int pid, int uid, long started) {
        if (!managed(packageName)) return true;
        JSONObject policy = packages.get(packageName);
        JSONObject process = processes.get(pid);
        return connected && valid && policy != null && policy.optBoolean("ready") && process != null &&
            packageName.equals(process.optString("package")) && uid == process.optInt("uid", -1) &&
            started > 0 && started == process.optLong("started", -1) &&
            policy.optInt("uid", -1) == process.optInt("package_uid", -2) &&
            policy.optString("active").equals(process.optString("slot"));
    }

    synchronized boolean gateRequired(String target, int uid, int callerUid, String token, boolean taskResume) {
        if (!managed(target)) return false;
        if (consumePermit(target, callerUid, token)) {
            sessions.manual(target, uid);
            return false;
        }
        if (pending.containsKey(target)) return true;
        JSONObject policy = packages.get(target);
        if (!connected || !valid || policy == null || !policy.optBoolean("ready")) return true;
        if (policy.isNull("default") || policy.optString("active").equals(policy.optString("default"))) return false;
        return !sessions.continues(target, callerUid, taskResume);
    }

    private boolean consumePermit(String target, int callerUid, String token) {
        if (!connected || !valid) return false;
        String selected = null;
        for (Map.Entry<String, JSONObject> entry : permits.entrySet()) {
            JSONObject permit = entry.getValue();
            if (!target.equals(permit.optString("package"))) continue;
            boolean root = callerUid == 0 && "root".equals(permit.optString("caller"));
            boolean manager = callerUid == managerUid && managerUid >= 10000 &&
                "manager".equals(permit.optString("caller")) && entry.getKey().equals(token);
            if (root || manager) { selected = entry.getKey(); break; }
        }
        if (selected == null || consumed.contains(selected)) return false;
        permits.remove(selected);
        consumed.add(selected);
        String ticket = pending.remove(target);
        if (ticket != null) shown.remove(ticket);
        send.accept(message("consumed", "token", selected));
        return true;
    }

    synchronized void requested(String target, int uid, int callerUid, boolean resultToOwner) {
        if (!GATE.equals(target) && !pending.containsKey(target) && !sessions.relatedCaller(callerUid) && !resultToOwner) {
            cancelPending();
        }
        sessions.requested(target, uid, callerUid, resultToOwner);
    }

    synchronized boolean resultOwnedBy(int uid) { return sessions.ownedBy(uid); }

    synchronized String begin(String target, boolean replace) {
        String existing = pending.get(target);
        if (existing != null && !replace) return existing;
        if (existing != null) cancel(existing);
        byte[] bytes = new byte[32];
        random.nextBytes(bytes);
        StringBuilder token = new StringBuilder(64);
        for (byte value : bytes) token.append(String.format(java.util.Locale.ROOT, "%02x", value & 255));
        String ticket = token.toString();
        pending.put(target, ticket);
        JSONObject frame = message("begin", "ticket", ticket);
        try { frame.put("package", target); } catch (Exception ignored) { return null; }
        send.accept(frame);
        return ticket;
    }

    synchronized boolean ticketPending(String ticket) { return pending.containsValue(ticket); }
    synchronized boolean ticketShown(String ticket) { return shown.contains(ticket); }
    synchronized boolean isManagerCaller(int uid) { return uid == managerUid && managerUid >= 10000; }

    synchronized void gateShown(String ticket) {
        if (pending.containsValue(ticket)) shown.add(ticket);
    }

    synchronized void homeRequested() { cancelPending(); }

    synchronized void cancel(String ticket) {
        pending.values().remove(ticket);
        shown.remove(ticket);
        send.accept(message("cancel", "ticket", ticket));
    }

    synchronized void resumed(String target, String className, int uid, boolean home, boolean locked) {
        if (MANAGER.equals(target) && GATE.equals(className)) return;
        boolean independent = sessions.resumed(target, uid, home, locked);
        if (locked) return;
        if (!home && !independent && shown.isEmpty()) return;
        cancelPending();
    }

    private void cancelPending() {
        List<String> tickets = new ArrayList<>(pending.values());
        pending.clear();
        shown.clear();
        for (String ticket : tickets) send.accept(message("cancel", "ticket", ticket));
    }

    synchronized void finishedGate(String target) { pending.remove(target); }

    static JSONObject message(String type, String field, Object value) {
        JSONObject result = new JSONObject();
        try { result.put("type", type); result.put(field, value); }
        catch (Exception error) { throw new IllegalArgumentException("invalid launch event", error); }
        return result;
    }

    private static Map<String, JSONObject> keyed(JSONArray list, String field) throws Exception {
        Map<String, JSONObject> result = new HashMap<>();
        for (int index = 0; index < list.length(); index++) {
            JSONObject row = list.getJSONObject(index);
            String key = row.getString(field);
            if (result.put(key, row) != null) throw new IllegalArgumentException("duplicate launch state");
        }
        return result;
    }
}
