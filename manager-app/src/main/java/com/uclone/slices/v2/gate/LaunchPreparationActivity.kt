package com.uclone.slices.v2.gate

import android.app.ActivityOptions
import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.lifecycle.lifecycleScope
import com.uclone.slices.v2.R
import kotlinx.coroutines.launch

/** A transparent launch boundary. The target App has not been resumed at this point. */
class LaunchPreparationActivity : ComponentActivity() {
    private val client = RootLaunchClient()
    private var resumed = false
    private var submitted = false
    private var launchIssued = false
    private var ticket = ""

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        suppressAnimation()
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
        lifecycleScope.launch {
            val prepared = client.prepare(submittedTicket)
            if (submittedTicket != ticket || !resumed || isFinishing || isDestroyed) {
                client.cancelAsync(submittedTicket)
                return@launch
            }
            val target = prepared?.let { packageManager.getLaunchIntentForPackage(it.packageName) }
            if (prepared == null || target == null) {
                Toast.makeText(this@LaunchPreparationActivity, R.string.launch_preparation_unavailable, Toast.LENGTH_SHORT).show()
                client.cancelAsync(ticket)
                finish()
                return@launch
            }
            target.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_NO_ANIMATION)
            target.putExtra(prepared.permitExtra, prepared.permit)
            launchIssued = true
            try {
                startActivity(target, ActivityOptions.makeCustomAnimation(this@LaunchPreparationActivity, 0, 0).toBundle())
            } catch (_: Exception) {
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

    @Suppress("DEPRECATION")
    private fun suppressAnimation() {
        if (Build.VERSION.SDK_INT >= 34) {
            overrideActivityTransition(OVERRIDE_TRANSITION_OPEN, 0, 0)
            overrideActivityTransition(OVERRIDE_TRANSITION_CLOSE, 0, 0)
        } else overridePendingTransition(0, 0)
    }
}
