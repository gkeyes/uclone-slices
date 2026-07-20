package com.uclone.slots.preview

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.lifecycle.viewmodel.compose.viewModel
import com.uclone.slots.preview.ui.SlotsManagerApp
import com.uclone.slots.preview.ui.SlotsTheme

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            SlotsTheme {
                SlotsManagerApp(viewModel())
            }
        }
    }
}
