package com.uclone.slices.fixture

import android.app.Activity
import android.content.Intent
import android.os.Bundle
import android.util.Log
import android.view.ViewGroup
import android.widget.Button
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.TextView
import java.io.File

class MainActivity : Activity() {
    private lateinit var status: TextView
    private lateinit var ceInput: EditText
    private lateinit var deInput: EditText

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(buildContent())
        consumeWrites(intent)
        render()
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        consumeWrites(intent)
        render()
    }

    private fun buildContent(): LinearLayout {
        status = TextView(this)
        ceInput = EditText(this).apply { hint = "CE identity" }
        deInput = EditText(this).apply { hint = "DE identity" }
        val save = Button(this).apply {
            text = "Write CE and DE"
            setOnClickListener {
                writeIdentity(ceFile(), ceInput.text.toString())
                writeIdentity(deFile(), deInput.text.toString())
                render()
            }
        }
        return LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(32, 32, 32, 32)
            addView(
                status,
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.WRAP_CONTENT,
            )
            addView(ceInput)
            addView(deInput)
            addView(save)
        }
    }

    private fun consumeWrites(intent: Intent) {
        intent.getStringExtra(EXTRA_WRITE_CE)?.let { writeIdentity(ceFile(), it) }
        intent.getStringExtra(EXTRA_WRITE_DE)?.let { writeIdentity(deFile(), it) }
        intent.removeExtra(EXTRA_WRITE_CE)
        intent.removeExtra(EXTRA_WRITE_DE)
    }

    private fun render() {
        val ce = readIdentity(ceFile())
        val de = readIdentity(deFile())
        ceInput.setText(if (ce == ABSENT) "" else ce)
        deInput.setText(if (de == ABSENT) "" else de)
        status.text = "version=${BuildConfig.VERSION_NAME}\nCE=$ce\nDE=$de"
        Log.i(LOG_TAG, "CE=$ce DE=$de")
    }

    private fun ceFile(): File = File(filesDir, IDENTITY_FILE)

    private fun deFile(): File =
        File(createDeviceProtectedStorageContext().filesDir, IDENTITY_FILE)

    private fun readIdentity(file: File): String =
        if (file.isFile) file.readText() else ABSENT

    private fun writeIdentity(file: File, value: String) {
        file.parentFile?.mkdirs()
        file.writeText(value)
    }

    private companion object {
        const val LOG_TAG = "UCloneFixture"
        const val IDENTITY_FILE = "identity.txt"
        const val EXTRA_WRITE_CE = "write_ce"
        const val EXTRA_WRITE_DE = "write_de"
        const val ABSENT = "<absent>"
    }
}
