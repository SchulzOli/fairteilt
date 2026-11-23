# Play Store: Eintrag und Freigabe (Entwurf)

Paketname `eu.olitech.fairteilt`, Version aus `app/src-tauri/tauri.conf.json`
(`0.1.0` → versionCode `1000`). Gebaut wird in GitHub Actions („Apps“, siehe unten).

## Texte

**App-Name (30 Zeichen):** Fairteilt – Haushalt mit Punkten

**Kurzbeschreibung (80 Zeichen):**
Haushalt fair verteilen: Aufgaben, Punkte und ein Familienziel für alle.

**Beschreibung (bis 4000 Zeichen):**

Fairteilt macht den Haushalt zur gemeinsamen Sache. Jede Aufgabe bringt Punkte, und am Monatsende
gleicht Fairteilt aus, wer mehr und wer weniger getragen hat, nach Regeln, die eure Familie selbst
festlegt.

• Räume und Aufgaben mit festem Rhythmus oder „nach Bedarf“: Aufgaben werden langsam gelb und rot,
  wenn sie fällig werden
• Schritt für Schritt abhaken, mit freundlicher Nachfrage, falls jemand hängen bleibt
• Kinder melden sich mit Tier und PIN an, Eltern bestätigen und passen Punkte an
• Serien, Boni auf ungeliebte Aufgaben, Punkte verschenken
• Familienziel: gemeinsam gegen Krümel rennen und eine Belohnung freischalten
• Kalender, Board und eigene Ansichten, Wandanzeige fürs Tablet
• Erinnerungen, Postfach und Push-Benachrichtigungen, die ihr pro Kategorie abstellen könnt
• Funktioniert auch ohne Netz: Änderungen warten und werden übertragen, sobald ihr wieder online seid
• Eure Daten gehören euch: Export als Datei, Konto und Haushalt in der App löschbar
• Keine Werbung, kein Tracking, freie Software (AGPL-3.0)

Den Server könnt ihr auch selbst betreiben: github.com/SchulzOli/fairteilt

**Kategorie:** Lifestyle (Alternative: Produktivität) · **Tags:** Familie, Haushalt, Aufgaben

## Grafiken (noch zu erstellen)

| Was | Größe |
| --- | --- |
| App-Symbol | 512 × 512 PNG (aus `app/icon-src/`) |
| Vorstellungsgrafik | 1024 × 500 PNG/JPG |
| Screenshots Telefon | mindestens 2, 1080 × 1920 oder größer (Heute, Aufgabe mit Schritten, Punkte, Rennen gegen Krümel, Plan) |
| Screenshots Tablet (7″/10″) | optional, Wandanzeige |

## Datensicherheit (Formular „Data safety“)

| Frage | Antwort |
| --- | --- |
| Erhebt oder teilt die App Daten? | Ja, erhebt; teilt keine (Push über FCM gilt als Dienstleister) |
| Verschlüsselung bei der Übertragung | Ja (HTTPS) |
| Kann man die Löschung beantragen? | Ja, in der App und per E-Mail |
| Persönliche Daten | E-Mail-Adresse, Name (Anzeigename): Kontoverwaltung, App-Funktionen; Pflicht |
| App-Aktivität | Andere nutzergenerierte Inhalte (Aufgaben, Punkte): App-Funktionen |
| Geräte-IDs | Push-Token: App-Funktionen (Benachrichtigungen) |
| Werbung, Analyse, Standort, Kontakte, Fotos | Nein |

**URL zum Löschen des Kontos** (Pflicht bei Apps mit Konten): eine Seite, die erklärt, wie man das
Konto löscht (App → Familie → Deine Daten → Konto löschen, oder E-Mail an [Kontakt]). Kann ein
Abschnitt der Datenschutzseite sein, z. B. `https://fairteilt.oli-tech.eu/datenschutz#loeschen`.

**Datenschutzerklärung:** Entwurf in [docs/DATENSCHUTZ.md](../DATENSCHUTZ.md), muss unter einer
öffentlichen Adresse liegen.

## Zielgruppe (Entscheidung offen)

Wenn Kinder unter 13 zur Zielgruppe gehören, gilt die **Familienrichtlinie** von Google Play
(strengere Prüfung, Vorgaben für SDKs). Einfacher: Zielgruppe „18+“ (die Eltern richten ein und
verwalten), Kinder nutzen die App über das Konto der Familie. Inhaltsbewertung über den
IARC-Fragebogen (keine Gewalt, kein Chat zwischen Fremden, keine Käufe) → voraussichtlich „USK 0“.

## Schritte bis zum ersten Release

1. **Play-Console-Konto** anlegen (einmalig 25 US-$, Identitätsprüfung). Bei neuen privaten
   Konten verlangt Google einen **geschlossenen Test mit mindestens 12 Testern über 14 Tage**,
   bevor die App öffentlich werden darf; die Familie und Freunde reichen dafür.
2. **Upload-Schlüssel** erzeugen (nur einmal, gut sichern; bei Verlust hilft Google beim Tausch):
   ```powershell
   & "$env:JAVA_HOME\bin\keytool.exe" -genkeypair -v -keystore upload.jks -alias upload -keyalg RSA -keysize 4096 -validity 10000
   ```
   Für lokale Builds `app/src-tauri/gen/android/app/keystore.properties` (steht in `.gitignore`):
   ```properties
   storeFile=D:/Schluessel/upload.jks
   storePassword=…
   keyAlias=upload
   keyPassword=…
   ```
3. In GitHub unter Settings → Secrets → Actions eintragen: `ANDROID_KEYSTORE_BASE64`
   (`[Convert]::ToBase64String([IO.File]::ReadAllBytes("upload.jks"))`), `ANDROID_KEYSTORE_PASSWORD`,
   `ANDROID_KEY_ALIAS`, `ANDROID_KEY_PASSWORD` und `GOOGLE_SERVICES_JSON` (Inhalt der Datei).
4. Actions → **Apps** → Run workflow (oder Tag `v0.1.0` pushen). Ergebnis: `app-universal-release.aab`
   und eine APK zum direkten Installieren.
5. In der Play Console: App anlegen, „Play App Signing“ annehmen, AAB in den internen bzw.
   geschlossenen Test laden, Store-Eintrag, Datensicherheit, Zielgruppe, Inhaltsbewertung ausfüllen.

## iOS

Der Workflow baut ohne Apple-Konto nur für den Simulator. Für TestFlight braucht es ein
**Apple-Developer-Konto** (99 €/Jahr), dann: Team-ID als Secret `APPLE_DEVELOPMENT_TEAM`,
Zertifikat und Provisioning-Profil (oder App-Store-Connect-API-Schlüssel) als Secrets, Push über
APNs (Schlüssel in Firebase hinterlegen). Die App-Store-Texte können von oben übernommen werden.
