package eu.olitech.fairteilt

import android.content.Context
import android.util.Log
import com.google.firebase.FirebaseApp
import com.google.firebase.messaging.FirebaseMessaging
import com.google.firebase.messaging.FirebaseMessagingService
import java.io.File

/**
 * Push over Firebase Cloud Messaging (KONZEPT §8.2, stage 2).
 *
 * The FCM token is written to `dataDir/fcm_token`; the app reads it through the Rust command
 * `push_token` and registers it with the server. Messages with a notification block are shown
 * by Firebase itself while the app is in the background (channel "fairteilt"); in the
 * foreground the app's live channel shows them in-app.
 */
object PushToken {
  private const val FILE = "fcm_token"

  fun save(context: Context, token: String) {
    runCatching { File(context.dataDir, FILE).writeText(token) }
      .onFailure { Log.w("Fairteilt", "could not store push token", it) }
  }

  /** Asks Firebase for the current token (no-op when the app was built without Firebase). */
  fun refresh(context: Context) {
    if (FirebaseApp.getApps(context).isEmpty() && FirebaseApp.initializeApp(context) == null) return
    FirebaseMessaging.getInstance().token
      .addOnSuccessListener { save(context, it) }
      .addOnFailureListener { Log.w("Fairteilt", "no push token", it) }
  }
}

/** Receives new tokens (first start, reinstall, rotation). */
class PushService : FirebaseMessagingService() {
  override fun onNewToken(token: String) {
    PushToken.save(this, token)
  }
}
