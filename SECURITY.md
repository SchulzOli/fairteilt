# Sicherheit

Fairteilt speichert Daten von Familien, auch von Kindern. Sicherheitslücken nehmen wir deshalb ernst.

## Lücke melden

Bitte **kein öffentliches Issue** öffnen. Melde Sicherheitsprobleme vertraulich über
**[Security → Report a vulnerability](../../security/advisories/new)** hier auf GitHub.

Hilfreich sind:

- betroffene Komponente (Server, App, Deployment) und Version bzw. Commit
- Schritte zum Nachstellen oder ein kurzer Proof of Concept
- was ein Angreifer damit erreichen kann

Wir bestätigen den Eingang so bald wie möglich, in der Regel innerhalb weniger Tage, und halten dich
über den Fix auf dem Laufenden. Nach dem Fix nennen wir dich gern im Advisory, wenn du möchtest.

## Unterstützte Versionen

Fairteilt ist in der Alpha-Phase. Fixes gibt es für den aktuellen Stand von `main` und das daraus
gebaute Server-Image `ghcr.io/schulzoli/fairteilt-server:main`.

## Was schon eingebaut ist

- Passwörter und PINs mit Argon2id, PIN-Sperre nach 5 Fehlversuchen
- kurze Zugriffstoken (15 min) und rotierende Refresh-Token pro Gerät, auf dem Gerät im
  Schlüsselbund bzw. app-privat gespeichert
- Google-Anmeldung: Prüfung von Signatur, Aussteller, Zielgruppe und Ablauf; am Desktop mit PKCE
- Ratenbegrenzung bei Anmeldung und PIN, CORS nur für die App-Ursprünge
- Server hinter Caddy mit TLS, Datenbank nicht öffentlich erreichbar
