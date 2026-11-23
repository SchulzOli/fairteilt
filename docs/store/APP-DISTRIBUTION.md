# Testversionen über Firebase App Distribution

Die Familie bekommt neue Android-Versionen über **Firebase App Distribution**: Jeder Lauf des
Workflows **Apps** baut eine signierte APK und schickt sie an die Testergruppe `familie`. Auf den
Handys meldet die App „Firebase App Tester“ das Update. Für den Play Store gilt weiter
[PLAY-STORE.md](PLAY-STORE.md).

## Einmal einrichten

1. **App Distribution aktivieren:** Firebase Console → Projekt → *Ausführen & Überwachen* →
   **App Distribution** → Android-App `eu.olitech.fairteilt` wählen → *Jetzt starten*.
2. **Gruppe anlegen:** Reiter *Tester und Gruppen* → *Gruppe hinzufügen*, Name „Familie“,
   Alias `familie`. Tester per E-Mail (Google-Konto) hinzufügen oder unter *Einladungslinks* einen
   Link für die Gruppe erzeugen und nur in der Familie teilen.
3. **Dienstkonto nur fürs Verteilen** (nicht das Server-Dienstkonto, das mehr darf):
   [Google Cloud Console](https://console.cloud.google.com) → oben das Firebase-Projekt wählen →
   *IAM & Verwaltung* → *Dienstkonten* → *Dienstkonto erstellen*:
   - Name: `github-app-distribution`
   - Rolle: **Firebase App Distribution Admin**
   - danach im Dienstkonto: *Schlüssel* → *Schlüssel hinzufügen* → *Neuen Schlüssel erstellen* → JSON
4. **Als GitHub-Secret hinterlegen und die Datei löschen** (PowerShell, Pfad anpassen):

   ```powershell
   Get-Content -Raw "$env:USERPROFILE\Downloads\<datei>.json" | gh secret set FIREBASE_APP_DISTRIBUTION_KEY
   ```

   ```powershell
   Remove-Item "$env:USERPROFILE\Downloads\<datei>.json"
   ```

Voraussetzung sind außerdem die Secrets für Signierung und Push (`ANDROID_KEYSTORE_*`,
`ANDROID_KEY_*`, `GOOGLE_SERVICES_JSON`, siehe [PLAY-STORE.md](PLAY-STORE.md)). Fehlt eines,
baut der Workflow trotzdem, verteilt aber nichts.

## Neue Version verteilen

```powershell
gh workflow run apps.yml --ref main
```

Oder auf GitHub: *Actions* → **Apps** → *Run workflow*. Dort lassen sich einstellen:

| Feld | Standard | Bedeutung |
| --- | --- | --- |
| `distribute` | an | APK an App Distribution schicken |
| `groups` | `familie` | Testergruppen (Aliase, durch Komma getrennt) |
| `release_notes` | leer | Text für die Tester; leer = letzte Commit-Nachricht |

Ein Git-Tag `v*` (z. B. `v0.2.0`) baut und verteilt ebenfalls an `familie`. Jeder Build bekommt
einen eigenen `versionCode` (100000 + Laufnummer des Workflows), sonst würden die Handys ihn
nicht als Update erkennen. Der angezeigte Versionsname bleibt `version` aus `tauri.conf.json`.
Die Versionshinweise enthalten Build-Nummer und Commit.

## Auf den Handys

1. Einladung (E-Mail oder Link) **auf dem Handy** öffnen, annehmen, mit Google anmelden.
2. „Firebase App Tester“ installieren (einmal „Apps aus dieser Quelle“ erlauben).
3. In App Tester Fairteilt *Herunterladen* → *Installieren* (Erlaubnis für App Tester), danach
   Benachrichtigungen für Fairteilt erlauben.
4. Neue Versionen meldet App Tester per Benachrichtigung; Daten bleiben beim Update erhalten.

Gut zu wissen:

- Eine Entwicklerversion aus `tauri android dev` ist mit einem anderen Schlüssel signiert. Vor der
  ersten Installation aus App Tester muss sie deinstalliert werden, sonst meldet Android einen
  Konflikt.
- Kinder ohne eigenes Google-Konto: auf ihrem Gerät mit einem Tester-Konto der Eltern in App Tester
  anmelden, oder die APK direkt installieren (Artefakt `fairteilt-android` des Laufs).
- Releases laufen in App Distribution nach 150 Tagen ab. Installierte Apps laufen weiter.
