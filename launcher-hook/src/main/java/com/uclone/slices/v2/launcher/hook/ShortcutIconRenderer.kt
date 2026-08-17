package com.uclone.slices.v2.launcher.hook

import android.content.Context
import android.content.res.Configuration
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint

internal object ShortcutIconRenderer {
    const val DAY_FALLBACK = 0xFF1F1F1F.toInt()
    const val NIGHT_FALLBACK = 0xFFF5F5F5.toInt()

    fun themedColor(context: Context): Int {
        val fallback = fallbackColor(context.resources.configuration.uiMode)
        return runCatching {
            val attributes = context.obtainStyledAttributes(
                intArrayOf(android.R.attr.textColorPrimary),
            )
            try {
                attributes.getColor(0, fallback)
            } finally {
                attributes.recycle()
            }
        }.getOrDefault(fallback)
    }

    fun fallbackColor(uiMode: Int): Int =
        if (uiMode and Configuration.UI_MODE_NIGHT_MASK == Configuration.UI_MODE_NIGHT_YES) {
            NIGHT_FALLBACK
        } else {
            DAY_FALLBACK
        }

    fun render(color: Int): Bitmap {
        val bitmap = Bitmap.createBitmap(48, 48, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(bitmap)
        val stroke = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            style = Paint.Style.STROKE
            strokeCap = Paint.Cap.ROUND
            strokeJoin = Paint.Join.ROUND
            strokeWidth = 3f
            this.color = color
        }

        canvas.drawLine(12f, 17f, 36f, 17f, stroke)
        canvas.drawLine(36f, 17f, 30f, 11f, stroke)
        canvas.drawLine(36f, 17f, 30f, 23f, stroke)

        canvas.drawLine(36f, 31f, 12f, 31f, stroke)
        canvas.drawLine(12f, 31f, 18f, 25f, stroke)
        canvas.drawLine(12f, 31f, 18f, 37f, stroke)
        return bitmap
    }
}
