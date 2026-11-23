# „Mit Google anmelden“ einrichten

Erwachsene können sich statt mit E-Mail und Passwort mit ihrem Google-Konto anmelden. Beim ersten
Mal legt Fairteilt das Konto an (Name und E-Mail von Google); gibt es schon ein Konto mit derselben
E-Mail, wird es verbunden, das Passwort funktioniert weiter. Kinder melden sich wie bisher mit
Kinder-Code und PIN an.

So läuft es technisch:

- **Android-App:** Googles Anmelde-Fenster über den Credential Manager von Android
  (`app/src-tauri/gen/android/app/src/main/java/eu/olitech/fairteilt/GoogleSignIn.kt`).
- **Browser** (Entwicklung): der Google-Knopf von Google Identity Services (`app/src/lib/google.ts`).
- **Desktop-App** (Windows, macOS, Linux): öffnet Google im Standardbrowser. Google schickt einen
  Code an eine lokale Adresse (`http://127.0.0.1:<zufälliger Port>`, gesichert mit PKCE), die App
  reicht ihn an den Server weiter, und der Server tauscht ihn bei Google gegen das ID-Token
  (`app/src-tauri/src/google_desktop.rs`). Dafür braucht der Server das Secret des Desktop-Clients;
  in der App steht nur die öffentliche Client-ID.
- **iOS:** noch nicht angeboten. Dafür braucht es einen iOS-Client und einen Mac zum Bauen
  (Apple-Developer-Konto, siehe Roadmap).
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
4. Optional, für die Desktop-App: **Client erstellen → Desktop-App**, Name „Fairteilt Desktop“.
   Weiterleitungen trägt man nicht ein (Google erlaubt bei Desktop-Clients jeden Port auf
   `127.0.0.1`). Notieren: **Client-ID** (öffentlich) und **Client-Secret** (gehört nur auf den
   Server, nie ins Repo und nicht in die App).

## 2. Client-ID eintragen

| Wo | Was |
| --- | --- |
| Server (`/opt/fairteilt/deploy/.env`) | `GOOGLE_CLIENT_IDS=<Web-Client-ID>`, danach `sudo docker compose up -d` |
| GitHub (Apps-Workflow) | `gh variable set GOOGLE_CLIENT_ID --body <Web-Client-ID>` (Variable, kein Secret) |
| Entwicklung, Server | in `server/.env`: `GOOGLE_CLIENT_IDS=<Web-Client-ID>` |
| Entwicklung, Browser | `app/.env.local` (Vorlage `app/.env.example`): `VITE_GOOGLE_CLIENT_ID=<Web-Client-ID>` |
| Desktop-App, Server | `.env` auf dem Server bzw. `server/.env`: `GOOGLE_DESKTOP_CLIENT_ID=<Desktop-Client-ID>` und `GOOGLE_DESKTOP_CLIENT_SECRET=<Secret>` (beide oder keins) |
| Desktop-App, Bauen | `app/.env.local`: `VITE_GOOGLE_DESKTOP_CLIENT_ID=<Desktop-Client-ID>`, dann `pnpm tauri build` bzw. `pnpm tauri dev` |

Der Server meldet beim Start „sign in with Google enabled“ (mit Desktop-Client: „… also in the
desktop app“). Die nächste APK aus dem Workflow
**Apps** zeigt auf der Anmeldeseite „Mit Google fortfahren“.

## Fehlersuche

| Symptom | Ursache |
| --- | --- |
| Kein Google-Knopf in der App | `GOOGLE_CLIENT_ID` war beim Bauen nicht gesetzt (GitHub-Variable), am Desktop fehlt `VITE_GOOGLE_DESKTOP_CLIENT_ID`, oder iOS (dort noch nicht angeboten) |
| Desktop: „… am Computer ist auf diesem Server nicht eingerichtet“ | `GOOGLE_DESKTOP_CLIENT_ID`/`_SECRET` fehlen in der Server-`.env` |
| Desktop: Browser zeigt „Nicht angemeldet“ oder die App wartet weiter | im Browser abgebrochen oder ein anderes Fenster benutzt; in der App „Abbrechen“ und neu starten. Die App wartet höchstens 5 Minuten |
| Android: „Developer console is not set up correctly“ / sofort abgebrochen | SHA-1 oder Paketname im Android-Client passen nicht zur installierten APK |
| Android: „Auf diesem Gerät ist kein Google-Konto angemeldet“ | Google-Konto in den Android-Einstellungen hinzufügen |
| „… auf diesem Server nicht eingerichtet“ | `GOOGLE_CLIENT_IDS` fehlt in der Server-`.env` |
| „… hat nicht geklappt“ | Token für eine andere Client-ID (Web-Client-ID in App und Server gleich?), abgelaufen oder Uhrzeit des Handys falsch |
| Browser: Popup schließt sofort | `http://localhost:1420` fehlt bei den JavaScript-Quellen des Web-Clients |
