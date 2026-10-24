package com.uclone.slices.v2.gate;

import android.app.ActivityOptions;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.pm.ActivityInfo;
import android.content.pm.ApplicationInfo;
import android.content.pm.ResolveInfo;
import android.content.pm.PackageManager;
import android.os.Handler;
import android.os.Binder;
import android.os.IBinder;
import android.util.Log;
import android.os.Build;
import android.os.Looper;
import android.os.Process;

import java.io.File;
import java.lang.reflect.Method;
import java.lang.reflect.Constructor;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.atomic.AtomicBoolean;

import io.github.libxposed.api.XposedModule;

/**
 * Only system_server is in scope. App code and public SDK methods are not spoofed.
 *
 * An unsupported framework found while installing disables launch support for this boot.
 * A failure in one later callback is logged and affects only that start or resume: new
 * process starts stay guarded by Zygisk and Application binding, so one exception must
 * not leave every configured App and the Manager unusable until a reboot.
 */
public final class SystemLaunchHook extends XposedModule {
    private final AtomicBoolean started = new AtomicBoolean();
    private final LaunchRequestContext resultToOwner = new LaunchRequestContext();
    private SystemBridge bridge;
    private Context context;
    private ResolveInfo preparation;
    private Object service;
    private Object globalLock;
    private Object rootWindow;
    private Class<?> activityRecordClass;
    private boolean hooksInstalled;

    @Override public void onModuleLoaded(ModuleLoadedParam loaded) {
        if (!loaded.isSystemServer()) detach();
    }

    @Override public void onSystemServerStarting(SystemServerStartingParam starting) {
        try {
            bridge = new SystemBridge();
            // TaskFragment, mInTaskFragment and mTaskSupervisor first exist in Android 13.
            if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) throw new IllegalStateException("Android 13 or later is required");
            ClassLoader loader = starting.getClassLoader();
            Class<?> interceptor = Class.forName("com.android.server.wm.ActivityStartInterceptor", false, loader);
            Class<?> taskFragment = Class.forName("com.android.server.wm.TaskFragment", false, loader);
            Class<?> starter = Class.forName("com.android.server.wm.ActivityStarter", false, loader);
            Class<?> manager = Class.forName("com.android.server.am.ActivityManagerService", false, loader);
            Class<?> tasks = Class.forName("com.android.server.wm.ActivityTaskManagerService", false, loader);
            activityRecordClass = Class.forName("com.android.server.wm.ActivityRecord", false, loader);
            // Resolve every surface before installing any Hook, so a ROM missing one of them
            // never ends up with resume gating installed but no preparation path.
            List<Method> intercepts = methods(interceptor, "intercept");
            List<Method> resumes = methods(taskFragment, "resumeTopActivity");
            List<Method> completions = methods(activityRecordClass, "completeResumeLocked");
            List<Method> requests = methods(starter, "executeRequest");
            List<Method> binds = new ArrayList<>();
            for (Method method : manager.getDeclaredMethods()) {
                Class<?>[] args = method.getParameterTypes();
                if (!method.getName().equals("attachApplicationLocked") || (args.length != 4 && args.length != 5) ||
                    args[1] != int.class || args[2] != int.class || args[3] != long.class ||
                    !"android.app.IApplicationThread".equals(args[0].getName()) ||
                    (method.getReturnType() != boolean.class && method.getReturnType() != void.class)) continue;
                binds.add(method);
            }
            if (binds.isEmpty()) throw new IllegalStateException("unsupported Application bind surface");
            List<Method> ready = methods(tasks, "onSystemReady");
            // Members first read at systemReady are checked here too, so a ROM missing one
            // disables launch support before any Hook exists rather than leaving the bind
            // guard live without a Root bridge.
            FrameworkAccess.requireFields(tasks, "mContext", "mGlobalLock", "mRootWindowContainer");
            FrameworkAccess.requireFields(manager, "mPidsSelfLocked");
            FrameworkAccess.requireFields(interceptor, "mAInfo", "mRInfo", "mIntent", "mResolvedType", "mUserId",
                "mCallingUid", "mCallingPid", "mRealCallingUid", "mRealCallingPid", "mInTask", "mInTaskFragment",
                "mActivityOptions");
            FrameworkAccess.requireMethod(Class.forName("com.android.server.wm.RootWindowContainer", false, loader),
                "getTopDisplayFocusedRootTask");
            hookAll(intercepts, "intercept", chain -> {
                Object result = chain.proceed();
                if (Boolean.FALSE.equals(result)) {
                    try { if (intercept(chain.getThisObject())) return true; }
                    catch (Throwable error) { failedOnce("intercept", error); }
                }
                return result;
            });
            hookAll(resumes, "resumeTopActivity", chain -> {
                // A resumed record already runs in its verified process view; only the
                // default-account choice is skipped when this one check fails.
                try { if (beforeResume(chain.getThisObject())) return false; }
                catch (Throwable error) { failedOnce("resume", error); }
                return chain.proceed();
            });
            hookAll(completions, "completeResumeLocked", chain -> {
                Object result = chain.proceed();
                try { completedResume(chain.getThisObject()); }
                catch (Throwable error) { failedOnce("resumed", error); }
                return result;
            });
            hookAll(requests, "executeRequest", chain -> {
                boolean owned = false;
                try {
                    Object request = chain.getArg(0);
                    Object result = field(request, "resultTo");
                    int requestCode = (Integer) field(request, "requestCode");
                    if (result != null && requestCode >= 0) {
                        Object record = FrameworkAccess.callStatic(activityRecordClass, "forTokenLocked",
                            new Class<?>[]{IBinder.class}, result);
                        ActivityInfo target = info(record);
                        owned = target != null && bridge.cache.resultOwnedBy(target.applicationInfo.uid);
                    }
                } catch (Throwable ignored) { }
                return resultToOwner.during(owned, chain::proceed);
            });
            for (Method method : binds) {
                hook(method).setId("slice.application-bind").intercept(chain -> {
                    try { if (beforeApplication(chain)) return blockedBindResult(method); }
                    catch (Throwable error) {
                        failedOnce("bind", error);
                        if (bridge.cache.managedUid((Integer) chain.getArg(2))) {
                            Process.killProcess((Integer) chain.getArg(1));
                            return blockedBindResult(method);
                        }
                    }
                    return chain.proceed();
                });
            }
            for (Constructor<?> constructor : tasks.getDeclaredConstructors()) {
                hook(constructor).setId("slice.task-service").intercept(chain -> {
                    Object result = chain.proceed();
                    service = chain.getThisObject();
                    return result;
                });
            }
            hookAll(ready, "onSystemReady", chain -> {
                Object result = chain.proceed();
                initialize(chain.getThisObject());
                return result;
            });
            hooksInstalled = true;
        } catch (Throwable error) { unsupported(); }
    }

    private static List<Method> methods(Class<?> owner, String name) {
        List<Method> found = new ArrayList<>();
        for (Method method : owner.getDeclaredMethods()) if (method.getName().equals(name)) found.add(method);
        if (found.isEmpty()) throw new IllegalStateException("unsupported framework surface " + name);
        return found;
    }

    private void hookAll(List<Method> methods, String name, Hooker hooker) {
        for (Method method : methods) hook(method).setId("slice." + name).intercept(hooker);
    }

    private void initialize(Object taskService) {
        if (!hooksInstalled || !started.compareAndSet(false, true)) return;
        try {
            service = taskService;
            context = (Context) field(service, "mContext");
            globalLock = field(service, "mGlobalLock");
            rootWindow = field(service, "mRootWindowContainer");
            Intent gate = gateIntent("0");
            // systemReady precedes CE unlock. Resolve metadata for both boot states;
            // OS launch checks and Runtime's unlock gate still govern actual execution.
            preparation = context.getPackageManager().resolveActivity(gate,
                PackageManager.MATCH_DIRECT_BOOT_AWARE | PackageManager.MATCH_DIRECT_BOOT_UNAWARE);
            if (preparation == null || preparation.activityInfo == null || !preparation.activityInfo.exported) {
                throw new IllegalStateException("preparation Activity is unavailable");
            }
            bridge.start(context);
        } catch (Throwable error) { unsupported(); }
    }

    private boolean intercept(Object interceptor) throws ReflectiveOperationException {
        ActivityInfo target = (ActivityInfo) field(interceptor, "mAInfo");
        Intent original = (Intent) field(interceptor, "mIntent");
        int user = (Integer) field(interceptor, "mUserId");
        if (target == null || original == null || user != 0 || target.applicationInfo.uid >= 100000) return false;
        if (LaunchPolicyCache.GATE.equals(target.name)) return false;
        if (original.hasCategory(Intent.CATEGORY_HOME)) bridge.cache.homeRequested();
        int caller = (Integer) field(interceptor, "mCallingUid");
        int real = (Integer) field(interceptor, "mRealCallingUid");
        boolean anchored = resultToOwner.isOwned() && real == 1000;
        int effective = LaunchSessions.effectiveCaller(caller, real, anchored);
        bridge.cache.requested(target.packageName, target.applicationInfo.uid, effective, anchored);
        String permit = null;
        if (bridge.cache.isManagerCaller(effective)) {
            try {
                permit = original.getStringExtra(LaunchPolicyCache.PERMIT_EXTRA);
                original.removeExtra(LaunchPolicyCache.PERMIT_EXTRA);
            } catch (RuntimeException invalidExtra) { permit = null; }
        }
        if (!bridge.cache.gateRequired(target.packageName, target.applicationInfo.uid, effective, permit, false, target.applicationInfo.sourceDir)) return false;
        if (preparation == null) return false;  // Native + Application-binding guards fail closed until ready.
        String ticket = bridge.cache.begin(target.packageName, true);
        Intent gate = gateIntent(ticket, target.packageName);
        FrameworkAccess.set(interceptor, "mIntent", gate);
        FrameworkAccess.set(interceptor, "mRInfo", new ResolveInfo(preparation));
        FrameworkAccess.set(interceptor, "mAInfo", new ActivityInfo(preparation.activityInfo));
        FrameworkAccess.set(interceptor, "mResolvedType", null);
        FrameworkAccess.set(interceptor, "mInTask", null);
        FrameworkAccess.set(interceptor, "mInTaskFragment", null);
        FrameworkAccess.set(interceptor, "mActivityOptions", ActivityOptions.makeBasic());
        FrameworkAccess.set(interceptor, "mCallingPid", (Integer) field(interceptor, "mRealCallingPid"));
        FrameworkAccess.set(interceptor, "mCallingUid", real);
        return true;
    }

    private boolean beforeResume(Object fragment) throws ReflectiveOperationException {
        Object record = topActivity(fragment);
        ActivityInfo target = info(record);
        if (target == null || !user0Display0(record) || LaunchPolicyCache.GATE.equals(target.name)) return false;
        if (!bridge.cache.gateRequired(target.packageName, target.applicationInfo.uid, -1, null, true, target.applicationInfo.sourceDir)) return false;
        // Without a preparation path the resume proceeds in the record's existing process view
        // rather than leaving a paused, untouchable screen.
        if (context == null || globalLock == null || preparation == null) return false;
        String ticket = bridge.cache.begin(target.packageName, false);
        if (ticket == null || !bridge.cache.ticketPending(ticket)) return false;  // Root is disconnected.
        if (!bridge.cache.queueGate(ticket)) return true;  // A gate for this ticket is already on its way.
        int expectedTask = taskId(record);
        new Handler(Looper.getMainLooper()).post(() -> {
            try {
                synchronized (globalLock) {
                    if (!bridge.cache.ticketPending(ticket)) return;
                    if (bridge.cache.ticketShown(ticket)) return;
                    Object focused = FrameworkAccess.call(rootWindow, "getTopDisplayFocusedRootTask");
                    Object top = topActivity(focused);
                    ActivityInfo topInfo = info(top);
                    if (topInfo != null && LaunchPolicyCache.GATE.equals(topInfo.name)) return;
                    if (top == null || taskId(top) != expectedTask) {
                        bridge.cache.cancel(ticket);
                        return;
                    }
                }
                // The potentially blocking start is outside WindowManager's global lock.
                context.startActivity(gateIntent(ticket, target.packageName));
            } catch (Throwable error) { bridge.cache.cancel(ticket); failedOnce("gate", error); }
        });
        return true;
    }

    private void completedResume(Object record) throws ReflectiveOperationException {
        ActivityInfo target = info(record);
        if (target == null || !user0Display0(record)) return;
        Intent intent = (Intent) field(record, "intent");
        if (LaunchPolicyCache.GATE.equals(target.name)) {
            bridge.cache.gateShown(intent == null ? null : intent.getStringExtra(LaunchPolicyCache.TICKET_EXTRA));
            return;
        }
        boolean home = Boolean.TRUE.equals(FrameworkAccess.call(record, "isActivityTypeHome"));
        boolean locked = false;
        try {
            Object supervisor = field(service, "mTaskSupervisor");
            Object keyguard = field(supervisor, "mKeyguardController");
            locked = Boolean.TRUE.equals(FrameworkAccess.call(keyguard, "isKeyguardLocked", new Class<?>[]{int.class}, 0));
        } catch (Throwable ignored) { }
        bridge.cache.resumed(target.packageName, target.name, target.applicationInfo.uid, home, locked);
    }

    private boolean beforeApplication(Chain chain) throws Exception {
        int pid = (Integer) chain.getArg(1);
        int uid = (Integer) chain.getArg(2);
        if (uid < 10000 || uid >= 100000 || (uid >= 20000 && uid < 30000)) return false;
        Object pids = field(chain.getThisObject(), "mPidsSelfLocked");
        Object process;
        synchronized (pids) { process = FrameworkAccess.call(pids, "get", new Class<?>[]{int.class}, pid); }
        if (process == null) return false;
        ApplicationInfo app = (ApplicationInfo) field(process, "info");
        if (app == null || !bridge.cache.managed(app.packageName)) return false;
        long started;
        try { started = processStarted(pid); }
        catch (java.io.IOException vanished) { return true; }
        if (bridge.cache.approvedProcess(app.packageName, pid, uid, started)) return false;
        Object ams = chain.getThisObject();
        Method method = (Method) chain.getExecutable();
        Object[] arguments = chain.getArgs().toArray();
        Invoker<?, Method> original = getInvoker(method).setType(Invoker.Type.ORIGIN);
        // App-Zygote children can skip the ordinary Zygisk specialization callback.
        // Delay Application binding, verify through Root outside the AMS lock, then
        // resume the same checked OS operation. system_server need not read App ns FDs.
        bridge.verifyProcess(app.packageName, pid, uid, started, () -> {
            synchronized (ams) {
                if (!sameProcess(pid, started)) return;
                if (!bridge.cache.approvedProcess(app.packageName, pid, uid, started)) {
                    Process.killProcess(pid);
                    return;
                }
                long identity = Binder.clearCallingIdentity();
                try { original.invoke(ams, arguments); }
                catch (Throwable failure) { if (sameProcess(pid, started)) Process.killProcess(pid); }
                finally { Binder.restoreCallingIdentity(identity); }
            }
        }, () -> { if (sameProcess(pid, started)) Process.killProcess(pid); });
        return true;
    }

    private static boolean sameProcess(int pid, long started) {
        try { return processStarted(pid) == started; }
        catch (Exception vanished) { return false; }
    }

    private static Object blockedBindResult(Method method) {
        return method.getReturnType() == void.class ? null : false;
    }

    private static long processStarted(int pid) throws Exception {
        byte[] bytes = Files.readAllBytes(new File("/proc/" + pid + "/stat").toPath());
        String stat = new String(bytes, StandardCharsets.UTF_8);
        int end = stat.lastIndexOf(')');
        if (end < 0) throw new IllegalArgumentException("invalid process stat");
        return Long.parseLong(stat.substring(end + 1).trim().split("\\s+")[19]);
    }

    private static ActivityInfo info(Object record) throws ReflectiveOperationException {
        return record == null ? null : (ActivityInfo) field(record, "info");
    }

    private static Object topActivity(Object container) throws ReflectiveOperationException {
        if (container == null) return null;
        try { return FrameworkAccess.call(container, "topRunningActivity"); }
        catch (NoSuchMethodException absent) { return FrameworkAccess.call(container, "topRunningActivity", new Class<?>[]{boolean.class}, false); }
    }

    private static boolean user0Display0(Object record) throws ReflectiveOperationException {
        return (Integer) field(record, "mUserId") == 0 &&
            ((Integer) FrameworkAccess.call(record, "getDisplayId")) == 0;
    }

    private static int taskId(Object record) throws ReflectiveOperationException {
        return (Integer) field(FrameworkAccess.call(record, "getTask"), "mTaskId");
    }

    private static Object field(Object object, String name) throws ReflectiveOperationException { return FrameworkAccess.get(object, name); }

    private static Intent gateIntent(String ticket, String packageName) {
        return new Intent().setComponent(new ComponentName(LaunchPolicyCache.MANAGER, LaunchPolicyCache.GATE))
            .putExtra(LaunchPolicyCache.TICKET_EXTRA, ticket)
            .putExtra(LaunchPolicyCache.PACKAGE_EXTRA, packageName)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_EXCLUDE_FROM_RECENTS | Intent.FLAG_ACTIVITY_NO_ANIMATION);
    }

    private void unsupported() {
        if (bridge != null) bridge.disable();
        log(Log.ERROR, "Slice", "Launch support unavailable; configured process starts remain guarded");
    }

    private void failedOnce(String stage, Throwable error) {
        log(Log.ERROR, "Slice", "Launch support callback failed at " + stage + "; only this request is affected: " + error);
    }
}
