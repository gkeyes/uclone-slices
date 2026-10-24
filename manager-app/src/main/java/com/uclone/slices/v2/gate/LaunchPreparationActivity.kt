package com.uclone.slices.v2.gate

import android.app.ActivityOptions
import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.view.Gravity
import android.view.View
import android.widget.Button
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.ProgressBar
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.lifecycle.lifecycleScope
import com.uclone.slices.v2.R
import com.uclone.slices.v2.apps.installedSigningIdentity
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** A transparent launch boundary. The target App has not been resumed at this point. */
class LaunchPreparationActivity : ComponentActivity() {
    private val client = RootLaunchClient()
    private var resumed = false
    private var submitted = false
    private var launchIssued = false
    private var ticket = ""
    private lateinit var progress: View

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        suppressAnimation()
        setContentView(progressView())
        ticket = intent.getStringExtra(LaunchPolicyCache.TICKET_EXTRA).orEmpty()
        if (!RootLaunchClient.validTicket(ticket) || savedInstanceState != null) {
            client.cancelAsync(ticket)
            finish()
        }
    }

    override fun onResume() {
        super.onResume()
        resumed = true
        if (submitted || isFinishing || !RootLaunchClient.validTicket(ticket)) return
        submitted = true
        val submittedTicket = ticket
        val target = intent.getStringExtra(LaunchPolicyCache.PACKAGE_EXTRA)
        val original = originalLaunch(intent)
        // A quick preparation stays invisible; a slow one shows progress and a way out.
        val reveal = lifecycleScope.launch {
            delay(PROGRESS_DELAY_MS)
            progress.visibility = View.VISIBLE
        }
        lifecycleScope.launch {
            // Root re-binds an App that was updated, or whose APK a reboot renumbered, only with
            // the signer read here; without it such a launch stays refused instead of frozen.
            val signing = target?.let { withContext(Dispatchers.IO) { installedSigningIdentity(packageManager, it) } }
            val prepared = client.prepare(submittedTicket, signing)
            reveal.cancel()
            if (submittedTicket != ticket || !resumed || isFinishing || isDestroyed) {
                client.cancelAsync(submittedTicket)
                return@launch
            }
            // The launch that was replaced comes first; the home screen covers one this page may
            // not start (a screen the App keeps to itself) and one that named another package.
            val launches = prepared?.let {
                listOfNotNull(
                    original?.takeIf { o -> OriginalLaunch.usable(o.component?.packageName, it.packageName) },
                    packageManager.getLaunchIntentForPackage(it.packageName),
                )
            }.orEmpty()
            if (prepared == null || launches.isEmpty()) {
                Toast.makeText(this@LaunchPreparationActivity, R.string.launch_preparation_unavailable, Toast.LENGTH_SHORT).show()
                client.cancelAsync(ticket)
                finish()
                return@launch
            }
            launchIssued = true
            val started = launches.any { launch ->
                launch.flags = OriginalLaunch.flags(launch.flags)
                launch.putExtra(prepared.permitExtra, prepared.permit)
                try {
                    startActivity(launch, ActivityOptions.makeCustomAnimation(this@LaunchPreparationActivity, 0, 0).toBundle())
                    true
                } catch (_: Exception) {
                    false
                }
            }
            if (!started) {
                launchIssued = false
                client.cancelAsync(ticket)
                Toast.makeText(this@LaunchPreparationActivity, R.string.launch_preparation_unavailable, Toast.LENGTH_SHORT).show()
            }
            finish()
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        val next = intent.getStringExtra(LaunchPolicyCache.TICKET_EXTRA).orEmpty()
        if (next == ticket) return
        client.cancelAsync(ticket)
        setIntent(intent)
        ticket = next
        submitted = false
        launchIssued = false
        if (!RootLaunchClient.validTicket(ticket)) finish()
    }

    override fun onPause() {
        resumed = false
        if (!launchIssued) client.cancelAsync(ticket)
        super.onPause()
        suppressAnimation()
    }

    private fun originalLaunch(source: Intent): Intent? = runCatching {
        if (Build.VERSION.SDK_INT >= 33) source.getParcelableExtra(LaunchPolicyCache.ORIGINAL_EXTRA, Intent::class.java)
        else @Suppress("DEPRECATION") source.getParcelableExtra(LaunchPolicyCache.ORIGINAL_EXTRA)
    }.getOrNull()

    private fun progressView(): View {
        val density = resources.displayMetrics.density
        val panel = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.CENTER_HORIZONTAL
            visibility = View.INVISIBLE
            addView(ProgressBar(this@LaunchPreparationActivity))
            addView(Button(this@LaunchPreparationActivity).apply {
                setText(android.R.string.cancel)
                setOnClickListener {
                    client.cancelAsync(ticket)
                    finish()
                }
            }, LinearLayout.LayoutParams(
                LinearLayout.LayoutParams.WRAP_CONTENT,
                LinearLayout.LayoutParams.WRAP_CONTENT,
            ).apply { topMargin = (12 * density).toInt() })
        }
        progress = panel
        return FrameLayout(this).apply {
            addView(panel, FrameLayout.LayoutParams(
                FrameLayout.LayoutParams.WRAP_CONTENT,
                FrameLayout.LayoutParams.WRAP_CONTENT,
                Gravity.CENTER,
            ))
        }
    }

    @Suppress("DEPRECATION")
    private fun suppressAnimation() {
        if (Build.VERSION.SDK_INT >= 34) {
            overrideActivityTransition(OVERRIDE_TRANSITION_OPEN, 0, 0)
            overrideActivityTransition(OVERRIDE_TRANSITION_CLOSE, 0, 0)
        } else overridePendingTransition(0, 0)
    }

    private companion object {
        const val PROGRESS_DELAY_MS = 300L
    }
}
