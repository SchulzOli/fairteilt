# „Mit Google anmelden“ einrichten

Erwachsene können sich statt mit E-Mail und Passwort mit ihrem Google-Konto anmelden. Beim ersten
Mal legt Fairteilt das Konto an (Name und E-Mail von Google); gibt es schon ein Konto mit derselben
E-Mail, wird es verbunden, das Passwort funktioniert weiter. Kinder melden sich wie bisher mit
Kinder-Code und PIN an.

So läuft es technisch:

- **Android-App:** Googles Anmelde-Fenster über den Credential Manager von Android
  (`app/src-tauri/gen/android/app/src/main/java/eu/olitech/fairteilt/GoogleSignIn.kt`).
- **Browser** (Entwicklung): der Google-Knopf von Google Identity Services (`app/src/lib/google.ts`).
- **Server** prüft das ID-Token gegen Googles öffentliche Schlüssel: Signatur, Aussteller, Ablauf,
  bestätigte E-Mail und dass es für unsere Client-ID ausgestellt ist (`server/src/google.rs`).
  Endpunkt `POST /api/v1/auth/google`.

Ohne Client-ID ist alles aus: Die App zeigt keinen Google-Knopf, der Server lehnt ab.

## 1. OAuth-Clients in der Google Cloud anlegen (einmalig)

Im **selben Google-Cloud-Projekt wie Firebase**: [console.cloud.google.com](https://console.cloud.google.com)
→ oben das Fairteilt-Projekt wählen → **APIs & Dienste** (bzw. **Google Auth Platform**).

1. **Branding / OAuth-Zustimmungsbildschirm:** App-Name „Fairteilt“, Support-E-Mail, Zielgruppe
   **Extern**, veröffentlichen („In Produktion“). Für Name, E-Mail und Profil (Standard-Scopes) prüft
   Google nichts weiter.
2. **Clients → Client erstellen → Webanwendung**, Name „Fairteilt Web“:
   - Autorisierte JavaScript-Quellen: `http://localhost:1420` (Entwicklung im Browser)
   - Die **Client-ID** (`…apps.googleusercontent.com`) notieren. Sie ist öffentlich, kein Geheimnis.
     Das Client-Secret wird nicht gebraucht.
3. **Client erstellen → Android**, Name „Fairteilt Android“:
   - Paketname `eu.olitech.fairteilt`
   - SHA-1 des Upload-Schlüssels:

     ```powershell
     & "D:\Android\jdk-21\bin\keytool.exe" -list -v -keystore D:\Schluessel\fairteilt-upload.jks -alias upload
     ```

   - Für Entwicklerversionen (`tauri android dev`) einen zweiten Android-Client mit der SHA-1 des
     Debug-Schlüssels (`%USERPROFILE%\.android\debug.keystore`, Passwort `android`, Alias
     `androiddebugkey`).
   - Später mit Play Store und „Play App Signing“: zusätzlich die SHA-1 des App-Signaturschlüssels
     aus der Play Console.

   Die Android-Client-ID trägt man nirgends ein: Google erkennt die App an Paketname und Signatur
   und stellt das Token für die **Web**-Client-ID aus.

## 2. Client-ID eintragen

| Wo | Was |
| --- | --- |
| Server (`/opt/fairteilt/deploy/.env`) | `GOOGLE_CLIENT_IDS=<Web-Client-ID>`, danach `sudo docker compose up -d` |
| GitHub (Apps-Workflow) | `gh variable set GOOGLE_CLIENT_ID --body <Web-Client-ID>` (Variable, kein Secret) |
| Entwicklung, Server | in `server/.env`: `GOOGLE_CLIENT_IDS=<Web-Client-ID>` |
| Entwicklung, Browser | `app/.env.local` (Vorlage `app/.env.example`): `VITE_GOOGLE_CLIENT_ID=<Web-Client-ID>` |

Der Server meldet beim Start „sign in with Google enabled“. Die nächste APK aus dem Workflow
**Apps** zeigt auf der Anmeldeseite „Mit Google fortfahren“.

## Fehlersuche

| Symptom | Ursache |
| --- | --- |
| Kein Google-Knopf in der App | `GOOGLE_CLIENT_ID` war beim Bauen nicht gesetzt (GitHub-Variable), oder Desktop/iOS (dort noch nicht angeboten) |
| Android: „Developer console is not set up correctly“ / sofort abgebrochen | SHA-1 oder Paketname im Android-Client passen nicht zur installierten APK |
| Android: „Auf diesem Gerät ist kein Google-Konto angemeldet“ | Google-Konto in den Android-Einstellungen hinzufügen |
| „… auf diesem Server nicht eingerichtet“ | `GOOGLE_CLIENT_IDS` fehlt in der Server-`.env` |
| „… hat nicht geklappt“ | Token für eine andere Client-ID (Web-Client-ID in App und Server gleich?), abgelaufen oder Uhrzeit des Handys falsch |
| Browser: Popup schließt sofort | `http://localhost:1420` fehlt bei den JavaScript-Quellen des Web-Clients |
