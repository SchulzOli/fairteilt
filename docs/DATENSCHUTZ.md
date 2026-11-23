# Datenschutzerklärung für Fairteilt (Entwurf)

> **Entwurf, keine Rechtsberatung.** Vor der Veröffentlichung im Play Store bzw. App Store die
> Platzhalter in eckigen Klammern füllen, prüfen lassen und unter einer festen Adresse
> veröffentlichen (z. B. `https://fairteilt.oli-tech.eu/datenschutz`). Stand: [Datum]

## 1. Verantwortlich

[Name des Betreibers]
[Anschrift]
E-Mail: [Kontaktadresse für Datenschutzanfragen]

Fairteilt ist freie Software (AGPL-3.0). Wer einen eigenen Server betreibt, ist für die Daten auf
diesem Server selbst verantwortlich. Diese Erklärung gilt für den Server `fairteilt.oli-tech.eu`.

## 2. Welche Daten wir verarbeiten und wozu

| Daten | Zweck | Rechtsgrundlage |
| --- | --- | --- |
| E-Mail-Adresse, Passwort (nur als Argon2id-Hash), Anzeigename | Konto, Anmeldung | Art. 6 Abs. 1 lit. b DSGVO (Vertrag) |
| Kinderprofile: Anzeigename, Tier, Farbe, PIN (nur als Hash), keine E-Mail | Profil im Haushalt | Art. 6 Abs. 1 lit. b, angelegt von den Eltern |
| Inhalte des Haushalts: Räume, Aufgaben, Erledigungen, Punkte, Übertragungen, Belohnungen, Abschlüsse, Postfach | Kernfunktion der App | Art. 6 Abs. 1 lit. b |
| Geräte: Anmelde-Sitzungen (Refresh-Token als Hash, Gerätename), Push-Token | angemeldet bleiben, Benachrichtigungen | Art. 6 Abs. 1 lit. b |
| Technische Protokolle des Servers (IP-Adresse, Zeitpunkt, Pfad), höchstens [7] Tage | Betrieb und Sicherheit | Art. 6 Abs. 1 lit. f (berechtigtes Interesse) |

Es gibt **keine Werbung, kein Tracking und keine Analyse-Dienste**. Daten werden nicht verkauft.

## 3. Empfänger

- **Hosting:** [Hetzner Online GmbH, Industriestr. 25, 91710 Gunzenhausen], Server in Deutschland,
  Auftragsverarbeitung nach Art. 28 DSGVO.
- **Push-Benachrichtigungen (Android):** Google Firebase Cloud Messaging (Google Ireland Ltd.).
  Übermittelt werden das Push-Token des Geräts sowie Titel und kurzer Text der Benachrichtigung.
  Dabei ist eine Übermittlung in die USA möglich (EU-US Data Privacy Framework). Push lässt sich
  je Kategorie in der App ausschalten; ohne Push bleiben Postfach und Erinnerungen in der App.
- **Backups:** [Hetzner Storage Box / eigener Speicher], verschlüsselt, Aufbewahrung [30] Tage.

## 4. Speicherdauer

- Daten bleiben gespeichert, solange das Konto bzw. der Haushalt besteht.
- **Konto löschen** (App → Familie → Deine Daten): E-Mail, Passwort, Sitzungen, Geräte und
  persönliche Einstellungen werden sofort gelöscht. Einträge im gemeinsamen Punktebuch eines
  Haushalts bleiben für die anderen Mitglieder erhalten, aber nur noch unter „Gelöschtes Konto“.
- **Haushalt löschen** (Admins): alle Inhalte des Haushalts werden sofort gelöscht, ebenso
  Kinderprofile ohne eigenes Konto.
- Wiederholschutz für Offline-Änderungen: [7] Tage. Backups: siehe oben.

## 5. Deine Rechte

Auskunft, Berichtigung, Löschung, Einschränkung, Datenübertragbarkeit und Widerspruch
(Art. 15–21 DSGVO). Vieles geht direkt in der App:

- **Meine Daten herunterladen:** alle Daten zu deinem Konto als JSON-Datei
- **Haushalt exportieren** (Admins): alle Daten des Haushalts als JSON-Datei
- **Konto löschen** und **Haushalt löschen** wie oben

Sonst genügt eine E-Mail an [Kontaktadresse]. Du kannst dich bei einer Datenschutz-Aufsichtsbehörde
beschweren, z. B. [zuständige Landesbehörde].

## 6. Kinder

Kinderprofile legen Eltern an oder geben sie per Kinder-Code frei. Kinder brauchen keine E-Mail.
Eltern können Profile jederzeit entfernen (App → Familie → Mitglied → „Aus dem Haushalt entfernen“).

## 7. Sicherheit

Verbindungen nur über HTTPS. Passwörter und PINs nur als Hash, Zugriffstoken laufen nach
15 Minuten ab, Anmeldungen sind je Gerät widerrufbar. Auf dem Gerät liegt das Refresh-Token im
Schlüsselbund des Systems (Android: privater App-Speicher). Der Offline-Zwischenspeicher der App
liegt nur auf dem Gerät und wird beim Abmelden gelöscht.
