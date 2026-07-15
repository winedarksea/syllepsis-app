package com.syllepsis.app

import android.os.Bundle
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
  private external fun nativeInitializeTlsVerifier(context: android.content.Context)

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    nativeInitializeTlsVerifier(applicationContext)
  }
}
