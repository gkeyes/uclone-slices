package com.uclone.slices.v2.gate;

import android.app.ActivityOptions;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.pm.ActivityInfo;
import android.content.pm.ApplicationInfo;
import android.content.pm.ResolveInfo;
import android.os.Handler;
import android.os.Looper;
import android.os.Process;

import java.io.File;
import java.lang.reflect.Method;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.util.Set;
import java.util.concurrent.atomic.AtomicBoolean;

import de.robv.android.xposed.IXposedHookLoadPackage;
import de.robv.android.xposed.XC_MethodHook;
import de.robv.android.xposed.XposedBridge;
import de.robv.android.xposed.XposedHelpers;
import de.robv.android.xposed.callbacks.XC_LoadPackage;

/** Only system_server is in scope. App code and public SDK methods are not spoofed. */
public final class SystemLaunchHook implements IXposedHookLoadPackage {
    private final AtomicBoolean started = new AtomicBoolean();
    private final ThreadLocal<Boolean> resultToOwner = new ThreadLocal<>();
    private SystemBridge bridge;
    private Context context;
    private ResolveInfo preparation;
    private Object service;
    private Object globalLock;
    private Object rootWindow;
    private Class<?> activityRecordClass;
    private boolean hooksInstalled;

    @Override public void handleLoadPackage(XC_LoadPackage.LoadPackageParam loaded) {
        if (!"android".equals(loaded.packageName)) return;
        try {
            bridge = new SystemBridge();
            ClassLoader loader = loaded.classLoader;
            Class<?> interceptor = XposedHelpers.findClass("com.android.server.wm.ActivityStartInterceptor", loader);
            Class<?> taskFragment = XposedHelpers.findClass("com.android.server.wm.TaskFragment", loader);
            Class<?> starter = XposedHelpers.findClass("com.android.server.wm.ActivityStarter", loader);
            Class<?> manager = XposedHelpers.findClass("com.android.server.am.ActivityManagerService", loader);
            Class<?> tasks = XposedHelpers.findClass("com.android.server.wm.ActivityTaskManagerService", loader);
            activityRecordClass = XposedHelpers.findClass("com.android.server.wm.ActivityRecord", loader);
            require(XposedBridge.hookAllMethods(interceptor, "intercept", new XC_MethodHook() {
                @Override protected void afterHookedMethod(MethodHookParam call) {
                    if (call.hasThrowable() || !Boolean.FALSE.equals(call.getResult())) return;
                    try { intercept(call); } catch (Throwable error) { unsupported(); }
                }
            }));
            require(XposedBridge.hookAllMethods(taskFragment, "resumeTopActivity", new XC_MethodHook() {
                @Override protected void beforeHookedMethod(MethodHookParam call) {
                    try { beforeResume(call); } catch (Throwable error) {
                        unsupported();
                        try {
                            ActivityInfo target = info(topActivity(call.thisObject));
                            if (target != null && bridge.cache.managed(target.packageName)) call.setResult(false);
                        } catch (Throwable ignored) { }
                    }
                }
            }));
            require(XposedBridge.hookAllMethods(activityRecordClass, "completeResumeLocked", new XC_MethodHook() {
                @Override protected void afterHookedMethod(MethodHookParam call) {
                    if (call.hasThrowable()) return;
                    try { completedResume(call.thisObject); } catch (Throwable error) { unsupported(); }
                }
            }));
            require(XposedBridge.hookAllMethods(starter, "executeRequest", new XC_MethodHook() {
                @Override protected void beforeHookedMethod(MethodHookParam call) {
                    call.setObjectExtra("slice.previous.result", resultToOwner.get());
                    boolean owned = false;
                    try {
                        Object request = call.args[0];
                        Object result = field(request, "resultTo");
                        int requestCode = (Integer) field(request, "requestCode");
                        if (result != null && requestCode >= 0) {
                            Object record = XposedHelpers.callStaticMethod(activityRecordClass, "forTokenLocked", result);
                            ActivityInfo info = info(record);
                            owned = info != null && bridge.cache.resultOwnedBy(info.applicationInfo.uid);
                        }
                    } catch (Throwable ignored) { }
                    resultToOwner.set(owned);
                }
                @Override protected void afterHookedMethod(MethodHookParam call) {
                    Boolean previous = (Boolean) call.getObjectExtra("slice.previous.result");
                    if (previous == null) resultToOwner.remove(); else resultToOwner.set(previous);
                }
            }));
            boolean attachInstalled = false;
            for (Method method : manager.getDeclaredMethods()) {
                Class<?>[] args = method.getParameterTypes();
                if (!method.getName().equals("attachApplicationLocked") || (args.length != 4 && args.length != 5) ||
                    args[1] != int.class || args[2] != int.class || args[3] != long.class ||
                    !"android.app.IApplicationThread".equals(args[0].getName()) ||
                    (method.getReturnType() != boolean.class && method.getReturnType() != void.class)) continue;
                XposedBridge.hookMethod(method, new XC_MethodHook() {
                    @Override protected void beforeHookedMethod(MethodHookParam call) {
                        try { beforeApplication(call); } catch (Throwable error) {
                            unsupported();
                            // The Binder identity already selected this PID. Refuse the
                            // bind; neither Application.onCreate nor an Activity may run.
                            if (bridge.cache.managedUid((Integer) call.args[2])) {
                                refuseBind(call, (Integer) call.args[1]);
                            }
                        }
                    }
                });
                attachInstalled = true;
            }
            if (!attachInstalled) throw new IllegalStateException("unsupported Application bind surface");
            XposedBridge.hookAllConstructors(tasks, new XC_MethodHook() {
                @Override protected void afterHookedMethod(MethodHookParam call) {
                    service = call.thisObject;
                }
            });
            require(XposedBridge.hookAllMethods(tasks, "onSystemReady", new XC_MethodHook() {
                @Override protected void afterHookedMethod(MethodHookParam call) { initialize(call.thisObject); }
            }));
            hooksInstalled = true;
        } catch (Throwable error) { unsupported(); }
    }

    private void initialize(Object taskService) {
        if (!hooksInstalled || !started.compareAndSet(false, true)) return;
        try {
            service = taskService;
            context = (Context) field(service, "mContext");
            globalLock = field(service, "mGlobalLock");
            rootWindow = field(service, "mRootWindowContainer");
            Intent gate = gateIntent("0");
            preparation = context.getPackageManager().resolveActivity(gate, 0);
            if (preparation == null || preparation.activityInfo == null || !preparation.activityInfo.exported) {
                throw new IllegalStateException("preparation Activity is unavailable");
            }
            bridge.start(context);
        } catch (Throwable error) { unsupported(); }
    }

    private void intercept(XC_MethodHook.MethodHookParam call) {
        ActivityInfo target = (ActivityInfo) field(call.thisObject, "mAInfo");
        Intent original = (Intent) field(call.thisObject, "mIntent");
        int user = (Integer) field(call.thisObject, "mUserId");
        if (target == null || original == null || user != 0 || target.applicationInfo.uid >= 100000) return;
        if (LaunchPolicyCache.GATE.equals(target.name)) return;
        if (original.hasCategory(Intent.CATEGORY_HOME)) bridge.cache.homeRequested();
        int caller = (Integer) field(call.thisObject, "mCallingUid");
        int real = (Integer) field(call.thisObject, "mRealCallingUid");
        boolean anchored = Boolean.TRUE.equals(resultToOwner.get()) && real == 1000;
        int effective = LaunchSessions.effectiveCaller(caller, real, anchored);
        bridge.cache.requested(target.packageName, target.applicationInfo.uid, effective, anchored);
        String permit = null;
        if (bridge.cache.isManagerCaller(effective)) {
            try {
                permit = original.getStringExtra(LaunchPolicyCache.PERMIT_EXTRA);
                original.removeExtra(LaunchPolicyCache.PERMIT_EXTRA);
            } catch (RuntimeException invalidExtra) { permit = null; }
        }
        if (!bridge.cache.gateRequired(target.packageName, target.applicationInfo.uid, effective, permit, false)) return;
        if (preparation == null) return;  // Native + Application-binding guards fail closed until ready.
        String ticket = bridge.cache.begin(target.packageName, true);
        Intent gate = gateIntent(ticket);
        XposedHelpers.setObjectField(call.thisObject, "mIntent", gate);
        XposedHelpers.setObjectField(call.thisObject, "mRInfo", new ResolveInfo(preparation));
        XposedHelpers.setObjectField(call.thisObject, "mAInfo", new ActivityInfo(preparation.activityInfo));
        XposedHelpers.setObjectField(call.thisObject, "mResolvedType", null);
        XposedHelpers.setObjectField(call.thisObject, "mInTask", null);
        XposedHelpers.setObjectField(call.thisObject, "mInTaskFragment", null);
        XposedHelpers.setObjectField(call.thisObject, "mActivityOptions", ActivityOptions.makeBasic());
        XposedHelpers.setIntField(call.thisObject, "mCallingPid", (Integer) field(call.thisObject, "mRealCallingPid"));
        XposedHelpers.setIntField(call.thisObject, "mCallingUid", real);
        call.setResult(true);
    }

    private void beforeResume(XC_MethodHook.MethodHookParam call) {
        Object record = topActivity(call.thisObject);
        ActivityInfo target = info(record);
        if (target == null || !user0Display0(record) || LaunchPolicyCache.GATE.equals(target.name)) return;
        if (!bridge.cache.gateRequired(target.packageName, target.applicationInfo.uid, -1, null, true)) return;
        call.setResult(false);
        if (context == null || globalLock == null || preparation == null) return;
        String ticket = bridge.cache.begin(target.packageName, false);
        if (!bridge.cache.queueGate(ticket)) return;
        int expectedTask = taskId(record);
        new Handler(Looper.getMainLooper()).post(() -> {
            try {
                synchronized (globalLock) {
                    if (!bridge.cache.ticketPending(ticket)) return;
                    if (bridge.cache.ticketShown(ticket)) return;
                    Object focused = XposedHelpers.callMethod(rootWindow, "getTopDisplayFocusedRootTask");
                    Object top = topActivity(focused);
                    ActivityInfo topInfo = info(top);
                    if (topInfo != null && LaunchPolicyCache.GATE.equals(topInfo.name)) return;
                    if (top == null || taskId(top) != expectedTask) {
                        bridge.cache.cancel(ticket);
                        return;
                    }
                }
                // The potentially blocking start is outside WindowManager's global lock.
                context.startActivity(gateIntent(ticket));
            } catch (Throwable error) { bridge.cache.cancel(ticket); unsupported(); }
        });
    }

    private void completedResume(Object record) {
        ActivityInfo target = info(record);
        if (target == null || !user0Display0(record)) return;
        Intent intent = (Intent) field(record, "intent");
        if (LaunchPolicyCache.GATE.equals(target.name)) {
            bridge.cache.gateShown(intent == null ? null : intent.getStringExtra(LaunchPolicyCache.TICKET_EXTRA));
            return;
        }
        boolean home = Boolean.TRUE.equals(XposedHelpers.callMethod(record, "isActivityTypeHome"));
        boolean locked = false;
        try {
            Object supervisor = field(service, "mTaskSupervisor");
            Object keyguard = field(supervisor, "mKeyguardController");
            locked = Boolean.TRUE.equals(XposedHelpers.callMethod(keyguard, "isKeyguardLocked", 0));
        } catch (Throwable ignored) { }
        bridge.cache.resumed(target.packageName, target.name, target.applicationInfo.uid, home, locked);
    }

    private void beforeApplication(XC_MethodHook.MethodHookParam call) throws Exception {
        int pid = (Integer) call.args[1];
        int uid = (Integer) call.args[2];
        if (uid < 10000 || uid >= 100000 || (uid >= 20000 && uid < 30000)) return;
        Object pids = field(call.thisObject, "mPidsSelfLocked");
        Object process;
        synchronized (pids) { process = XposedHelpers.callMethod(pids, "get", pid); }
        if (process == null) return;
        ApplicationInfo app = (ApplicationInfo) field(process, "info");
        if (app == null || !bridge.cache.managed(app.packageName)) return;
        if (!bridge.cache.approvedProcess(app.packageName, pid, uid, processStarted(pid))) {
            refuseBind(call, pid);
        }
    }

    private static void refuseBind(XC_MethodHook.MethodHookParam call, int pid) {
        Process.killProcess(pid);
        call.setResult(((Method) call.method).getReturnType() == void.class ? null : false);
    }

    private static long processStarted(int pid) throws Exception {
        byte[] bytes = Files.readAllBytes(new File("/proc/" + pid + "/stat").toPath());
        String stat = new String(bytes, StandardCharsets.UTF_8);
        int end = stat.lastIndexOf(')');
        if (end < 0) throw new IllegalArgumentException("invalid process stat");
        return Long.parseLong(stat.substring(end + 1).trim().split("\\s+")[19]);
    }

    private static ActivityInfo info(Object record) {
        return record == null ? null : (ActivityInfo) field(record, "info");
    }

    private static Object topActivity(Object container) {
        if (container == null) return null;
        try { return XposedHelpers.callMethod(container, "topRunningActivity"); }
        catch (NoSuchMethodError absent) { return XposedHelpers.callMethod(container, "topRunningActivity", false); }
    }

    private static boolean user0Display0(Object record) {
        return (Integer) field(record, "mUserId") == 0 &&
            ((Integer) XposedHelpers.callMethod(record, "getDisplayId")) == 0;
    }

    private static int taskId(Object record) {
        return (Integer) field(XposedHelpers.callMethod(record, "getTask"), "mTaskId");
    }

    private static Object field(Object object, String name) { return XposedHelpers.getObjectField(object, name); }

    private static Intent gateIntent(String ticket) {
        return new Intent().setComponent(new ComponentName(LaunchPolicyCache.MANAGER, LaunchPolicyCache.GATE))
            .putExtra(LaunchPolicyCache.TICKET_EXTRA, ticket)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_EXCLUDE_FROM_RECENTS | Intent.FLAG_ACTIVITY_NO_ANIMATION);
    }

    private static void require(Set<XC_MethodHook.Unhook> hooks) {
        if (hooks.isEmpty()) throw new IllegalStateException("unsupported framework surface");
    }

    private void unsupported() {
        if (bridge != null) bridge.disable();
        XposedBridge.log("Slice launch support unavailable; configured process starts remain guarded");
    }
}
