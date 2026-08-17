package com.uclone.slices.v2.desktop

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Intent
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import androidx.core.app.NotificationCompat
import com.uclone.slices.v2.BuildConfig
import com.uclone.slices.v2.R
import com.uclone.slices.v2.runtime.ErrorCode
import com.uclone.slices.v2.runtime.RootRuntimeClient
import java.util.concurrent.atomic.AtomicBoolean
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.cancelChildren
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

class DesktopQuickSwitchService : Service() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val operationMutex = Mutex()
    private val channelCreated = AtomicBoolean(false)
    private val mainHandler = Handler(Looper.getMainLooper())
    private var pendingOperations = 0

    override fun onCreate() {
        super.onCreate()
        ensureNotificationChannel()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        pendingOperations += 1
        startForeground(NOTIFICATION_ID, progressNotification())
        val packageName = intent
            ?.takeIf { it.action == DesktopShortcutContract.ACTION_QUICK_SWITCH }
            ?.getStringExtra(DesktopShortcutContract.KEY_PACKAGE_NAME)
            ?.takeIf(String::isNotBlank)
        val requestId = intent
            ?.getStringExtra(DesktopShortcutContract.EXTRA_REQUEST_ID)
            ?.takeIf(String::isNotBlank)
        if (packageName == null || requestId == null || intent.data?.lastPathSegment != requestId) {
            finishWithFailure(getString(R.string.desktop_switch_invalid_request))
            return START_NOT_STICKY
        }

        scope.launch {
            operationMutex.withLock {
                val runner = DesktopQuickSwitchRunner(
                    client = RootRuntimeClient(),
                    projection = SharedPreferencesDesktopShortcutProjection(applicationContext),
                    expectedRuntimeVersion = BuildConfig.VERSION_NAME,
                )
                when (val result = runner.run(packageName)) {
                    is DesktopQuickSwitchResult.Success -> finishSuccessfully()
                    DesktopQuickSwitchResult.RuntimeUnavailable -> finishWithFailure(
                        getString(R.string.desktop_switch_runtime_unavailable),
                    )
                    DesktopQuickSwitchResult.VersionMismatch -> finishWithFailure(
                        getString(R.string.desktop_switch_version_mismatch),
                    )
                    is DesktopQuickSwitchResult.Failed -> finishWithFailure(
                        failureMessage(result.code),
                    )
                }
            }
        }
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        scope.cancel()
        super.onDestroy()
    }

    override fun onTimeout(startId: Int) {
        finishAllWithFailure(getString(R.string.desktop_switch_timed_out))
    }

    override fun onTimeout(startId: Int, fgsType: Int) {
        finishAllWithFailure(getString(R.string.desktop_switch_timed_out))
    }

    override fun onBind(intent: Intent?): IBinder? = null

    private fun finishSuccessfully() {
        completeOperation()
    }

    private fun finishWithFailure(message: String) {
        mainHandler.post {
            notificationManager().notify(FAILURE_NOTIFICATION_ID, failureNotification(message))
            completeOperationOnMainThread()
        }
    }

    private fun completeOperation() {
        mainHandler.post { completeOperationOnMainThread() }
    }

    private fun completeOperationOnMainThread() {
        check(Looper.myLooper() == Looper.getMainLooper())
        pendingOperations = (pendingOperations - 1).coerceAtLeast(0)
        if (pendingOperations == 0) {
            stopForeground(STOP_FOREGROUND_REMOVE)
            stopSelf()
        }
    }

    private fun finishAllWithFailure(message: String) {
        scope.coroutineContext.cancelChildren()
        mainHandler.post {
            notificationManager().notify(FAILURE_NOTIFICATION_ID, failureNotification(message))
            pendingOperations = 0
            stopForeground(STOP_FOREGROUND_REMOVE)
            stopSelf()
        }
    }

    private fun failureMessage(code: ErrorCode): String = when (code) {
        ErrorCode.InvalidRequest -> getString(R.string.error_invalid_input)
        ErrorCode.NotFound -> getString(R.string.error_missing_entity)
        ErrorCode.StateConflict -> getString(R.string.error_state_changed)
        ErrorCode.IdentityMismatch -> getString(R.string.error_identity_protected)
        ErrorCode.OperationFailed -> getString(R.string.error_operation_failed)
    }

    private fun progressNotification(): Notification = NotificationCompat.Builder(this, CHANNEL_ID)
        .setSmallIcon(R.mipmap.ic_launcher)
        .setContentTitle(getString(R.string.desktop_switch_in_progress))
        .setContentText(getString(R.string.desktop_switch_wait))
        .setOngoing(true)
        .setOnlyAlertOnce(true)
        .setPriority(NotificationCompat.PRIORITY_LOW)
        .build()

    private fun failureNotification(message: String): Notification =
        NotificationCompat.Builder(this, CHANNEL_ID)
            .setSmallIcon(R.mipmap.ic_launcher)
            .setContentTitle(getString(R.string.desktop_switch_failed))
            .setContentText(message)
            .setAutoCancel(true)
            .setPriority(NotificationCompat.PRIORITY_DEFAULT)
            .build()

    private fun ensureNotificationChannel() {
        if (!channelCreated.compareAndSet(false, true)) return
        notificationManager().createNotificationChannel(
            NotificationChannel(
                CHANNEL_ID,
                getString(R.string.desktop_switch_channel),
                NotificationManager.IMPORTANCE_LOW,
            ),
        )
    }

    private fun notificationManager(): NotificationManager =
        getSystemService(NotificationManager::class.java)

    private companion object {
        const val CHANNEL_ID = "desktop_quick_switch"
        const val NOTIFICATION_ID = 1801
        const val FAILURE_NOTIFICATION_ID = 1802
    }
}
