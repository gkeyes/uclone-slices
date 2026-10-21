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
import android.os.Looper;
import android.os.Process;

import java.io.File;
import java.lang.reflect.Method;
import java.lang.reflect.Constructor;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.util.concurrent.atomic.AtomicBoolean;

import io.github.libxposed.api.XposedModule;

/** Only system_server is in scope. App code and public SDK methods are not spoofed. */
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
            ClassLoader loader = starting.getClassLoader();
            Class<?> interceptor = Class.forName("com.android.server.wm.ActivityStartInterceptor", false, loader);
            Class<?> taskFragment = Class.forName("com.android.server.wm.TaskFragment", false, loader);
            Class<?> starter = Class.forName("com.android.server.wm.ActivityStarter", false, loader);
            Class<?> manager = Class.forName("com.android.server.am.ActivityManagerService", false, loader);
            Class<?> tasks = Class.forName("com.android.server.wm.ActivityTaskManagerService", false, loader);
            activityRecordClass = Class.forName("com.android.server.wm.ActivityRecord", false, loader);
            hookMethods(interceptor, "intercept", chain -> {
                Object result = chain.proceed();
                if (Boolean.FALSE.equals(result)) {
                    try { if (intercept(chain.getThisObject())) return true; }
                    catch (Throwable error) { unsupported(); }
                }
                return result;
            });
            hookMethods(taskFragment, "resumeTopActivity", chain -> {
                try { if (beforeResume(chain.getThisObject())) return false; }
                catch (Throwable error) {
                    unsupported();
                    try {
                        ActivityInfo target = info(topActivity(chain.getThisObject()));
                        if (target != null && bridge.cache.managed(target.packageName)) return false;
                    } catch (Throwable ignored) { }
                }
                return chain.proceed();
            });
            hookMethods(activityRecordClass, "completeResumeLocked", chain -> {
                Object result = chain.proceed();
                try { completedResume(chain.getThisObject()); }
                catch (Throwable error) { unsupported(); }
                return result;
            });
            hookMethods(starter, "executeRequest", chain -> {
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
            boolean attachInstalled = false;
            for (Method method : manager.getDeclaredMethods()) {
                Class<?>[] args = method.getParameterTypes();
                if (!method.getName().equals("attachApplicationLocked") || (args.length != 4 && args.length != 5) ||
                    args[1] != int.class || args[2] != int.class || args[3] != long.class ||
                    !"android.app.IApplicationThread".equals(args[0].getName()) ||
                    (method.getReturnType() != boolean.class && method.getReturnType() != void.class)) continue;
                hook(method).setId("slice.application-bind").intercept(chain -> {
                    try { if (beforeApplication(chain)) return blockedBindResult(method); }
                    catch (Throwable error) {
                        unsupported();
                        if (bridge.cache.managedUid((Integer) chain.getArg(2))) {
                            Process.killProcess((Integer) chain.getArg(1));
                            return blockedBindResult(method);
                        }
                    }
                    return chain.proceed();
                });
                attachInstalled = true;
            }
            if (!attachInstalled) throw new IllegalStateException("unsupported Application bind surface");
            for (Constructor<?> constructor : tasks.getDeclaredConstructors()) {
                hook(constructor).setId("slice.task-service").intercept(chain -> {
                    Object result = chain.proceed();
                    service = chain.getThisObject();
                    return result;
                });
            }
            hookMethods(tasks, "onSystemReady", chain -> {
                Object result = chain.proceed();
                initialize(chain.getThisObject());
                return result;
            });
            hooksInstalled = true;
        } catch (Throwable error) { unsupported(); }
    }

    private void hookMethods(Class<?> owner, String name, Hooker hooker) {
        boolean installed = false;
        for (Method method : owner.getDeclaredMethods()) {
            if (!method.getName().equals(name)) continue;
            hook(method).setId("slice." + name).intercept(hooker);
            installed = true;
        }
        if (!installed) throw new IllegalStateException("unsupported framework surface");
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
        if (!bridge.cache.gateRequired(target.packageName, target.applicationInfo.uid, effective, permit, false)) return false;
        if (preparation == null) return false;  // Native + Application-binding guards fail closed until ready.
        String ticket = bridge.cache.begin(target.packageName, true);
        Intent gate = gateIntent(ticket);
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
        if (!bridge.cache.gateRequired(target.packageName, target.applicationInfo.uid, -1, null, true)) return false;
        if (context == null || globalLock == null || preparation == null) return true;
        String ticket = bridge.cache.begin(target.packageName, false);
        if (!bridge.cache.queueGate(ticket)) return true;
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
                context.startActivity(gateIntent(ticket));
            } catch (Throwable error) { bridge.cache.cancel(ticket); unsupported(); }
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

    private static Intent gateIntent(String ticket) {
        return new Intent().setComponent(new ComponentName(LaunchPolicyCache.MANAGER, LaunchPolicyCache.GATE))
            .putExtra(LaunchPolicyCache.TICKET_EXTRA, ticket)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_EXCLUDE_FROM_RECENTS | Intent.FLAG_ACTIVITY_NO_ANIMATION);
    }

    private void unsupported() {
        if (bridge != null) bridge.disable();
        log(Log.ERROR, "Slice", "Launch support unavailable; configured process starts remain guarded");
    }
}
