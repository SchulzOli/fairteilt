# Mitmachen bei Fairteilt

Schön, dass du da bist! Fairteilt ist ein Familienprojekt, das offen entwickelt wird. Jede Hilfe
zählt: ein Fehlerbericht, eine Idee, eine Übersetzung, ein Test oder ein Pull Request.

> **English speakers welcome.** The app and most docs are German, but issues and pull requests in
> English are just as welcome.

## Wo anfangen?

- **Fehler gefunden?** Öffne ein [Issue „Fehler melden“](../../issues/new/choose). Am besten mit
  Gerät, App-Version (Build-Nummer aus App Tester bzw. Commit) und den Schritten, die zum Fehler führen.
- **Idee?** Öffne ein [Issue „Idee“](../../issues/new/choose) und beschreib, welches Problem im
  Familienalltag sie löst. Größere Änderungen bitte erst besprechen, bevor du Code schreibst.
- **Erste Aufgabe?** Schau nach Issues mit dem Label
  [`good first issue`](../../issues?q=is%3Aopen+label%3A%22good+first+issue%22).
- **Wie alles zusammenhängt:** [docs/KONZEPT.md](docs/KONZEPT.md) (Regeln und Entscheidungen) und
  [docs/ROADMAP.md](docs/ROADMAP.md) (was fertig ist und was kommt).

## Entwicklungsumgebung

Kurz: Rust (stable), Node 22+, pnpm, Docker. Die ausführliche Anleitung mit Android-Build, API-Übersicht
und Push steht in **[docs/ENTWICKLUNG.md](docs/ENTWICKLUNG.md)**.

```bash
docker compose -f deploy/docker-compose.dev.yml up -d        # Postgres für die Entwicklung
cp server/.env.example server/.env && cargo run -p punkte-server
pnpm --dir app install && pnpm --dir app dev                 # App im Browser auf :1420
```

## Bevor du einen Pull Request öffnest

Die CI prüft dasselbe. Lokal geht es so (die Integrationstests brauchen die Datenbank von oben):

```bash
cargo clippy --workspace --all-targets -- -D warnings
DATABASE_URL=postgres://punkte:punkte-dev@127.0.0.1:5432/punkte cargo test --workspace
pnpm --dir app check
```

Hast du Typen im Server oder in `punkte-core` geändert, erzeuge die TypeScript-Typen neu und committe
sie mit:

```bash
cargo test -p punkte-core --features ts export_bindings
cargo test -p punkte-server --features ts --lib export_bindings
```

## Leitlinien

- **Regeln gehören in `punkte-core`.** Rechnungen (XP, Zustand, Ausgleich, Rennen gegen Krümel)
  bleiben rein und ohne IO und bekommen Tests. Server und App rufen sie nur auf.
- **Migrationen nur hinzufügen,** nie ändern, was schon ausgeliefert ist. Neue Datei in
  `server/migrations/` mit dem nächsten Zeitstempel, Zeilenenden LF.
- **Oberfläche:** mobil zuerst, große Touch-Ziele, hell und dunkel. Texte auf Deutsch, kurz und
  freundlich, „Aufholaufgabe“ statt „Strafe“. Keine Ranglisten.
- **Datenschutz:** kein Tracking, keine Werbung, keine Fotos von Kindern auf dem Server.
- **Commits** gern klein und mit einer Nachricht, die sagt, was sich für die Nutzer ändert.

## Lizenz

Mit deinem Beitrag stimmst du zu, dass er unter der [AGPL-3.0-or-later](LICENSE) veröffentlicht wird,
wie der Rest des Projekts.

## Umgang miteinander

Wir halten uns an den [Verhaltenskodex](CODE_OF_CONDUCT.md). Sei freundlich, geduldig und
konstruktiv, auch wenn es um Hausarbeit geht.
