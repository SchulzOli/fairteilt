package eu.olitech.fairteilt

// "Mit Google anmelden" on Android: Google's sign-in sheet through the Credential Manager.
// Returns the ID token; the server checks it (server/src/google.rs). Google blocks its web
// sign-in inside app webviews, so the app asks Android instead. JS: invoke("plugin:google-signin|sign_in").

import android.app.Activity
import androidx.credentials.CredentialManager
import androidx.credentials.CustomCredential
import androidx.credentials.GetCredentialRequest
import androidx.credentials.exceptions.GetCredentialCancellationException
import androidx.credentials.exceptions.NoCredentialException
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import com.google.android.libraries.identity.googleid.GetSignInWithGoogleOption
import com.google.android.libraries.identity.googleid.GoogleIdTokenCredential
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch

@InvokeArg
class SignInArgs {
  /** The OAuth *web* client ID: Google issues the ID token for it (the server's audience). */
  lateinit var serverClientId: String
}

@TauriPlugin
class GoogleSignInPlugin(private val activity: Activity) : Plugin(activity) {
  @Suppress("FunctionName")
  @Command
  fun sign_in(invoke: Invoke) {
    val args = invoke.parseArgs(SignInArgs::class.java)
    val option = GetSignInWithGoogleOption.Builder(args.serverClientId).build()
    val request = GetCredentialRequest.Builder().addCredentialOption(option).build()
    CoroutineScope(Dispatchers.Main).launch {
      try {
        val credential = CredentialManager.create(activity).getCredential(activity, request).credential
        if (credential is CustomCredential && credential.type == GoogleIdTokenCredential.TYPE_GOOGLE_ID_TOKEN_CREDENTIAL) {
          val google = GoogleIdTokenCredential.createFrom(credential.data)
          val ret = JSObject()
          ret.put("idToken", google.idToken)
          invoke.resolve(ret)
        } else {
          invoke.reject("Unerwartete Antwort von Google.")
        }
      } catch (e: GetCredentialCancellationException) {
        invoke.reject("cancelled")
      } catch (e: NoCredentialException) {
        invoke.reject("Auf diesem Gerät ist kein Google-Konto angemeldet.")
      } catch (e: Exception) {
        invoke.reject(e.message ?: "Die Anmeldung mit Google hat nicht geklappt.")
      }
    }
  }
}
