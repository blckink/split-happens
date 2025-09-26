# Nemirtingas Profile Configuration Audit

Dieser Überblick fasst zusammen, wie PartyDeck pro Profil die `NemirtingasEpicEmu.json` erzeugt, damit sich Spieler gegenseitig finden können.

## Stabiler Benutzerkontext pro Profil
- `ensure_nemirtingas_config` erstellt für jedes Profil einen persistenten Speicherort unter `profiles/<name>/nepice_settings/` und pflegt vorhandene IDs weiter.
- Bereits existierende `EpicId`- und `ProductUserId`-Werte werden übernommen, solange sie gültige Hex-Strings sind. Ungültige Werte führen zu einem Hinweis im Log und werden neu erzeugt, wodurch Konflikte verhindert werden.
- Die Prüfung akzeptiert dabei wahlweise rohe Hex-Zeichenketten oder Varianten mit einem optionalen `0x`/`0X`-Präfix, damit sich bestehende Konfigurationen nicht ändern müssen.

## Generierung eindeutiger IDs
- Fehlen die IDs, werden 32-stellige Hex-Werte erzeugt. Profile mit dem Standardnamen `DefaultName` erhalten zufällige IDs, während individuelle Profilnamen deterministisch über den Usernamen gehasht werden. So bleiben Identitäten auf mehreren Rechnern synchron.
- Die `ProductUserId` basiert deterministisch auf `appid` und der resultierenden `EpicId`, womit Multiplayer-Peers über Instanzen hinweg stabil bleiben.

## Netzwerk-Erkennung aktiviert
- Der erzeugte JSON-Block aktiviert den Broadcast-Plugin-Kanal (`Enabled: true`, `LocalhostOnly: false`). So können sich Spieler innerhalb desselben LANs automatisch entdecken.
- WebSocket-Signalisierung bleibt optional deaktiviert, lässt sich bei Bedarf aber durch Anpassen des JSON erweitern.

## Logging und Debugging
- Log-Level wird auf `Debug` gesetzt, und eine Log-Datei je Profil (`NemirtingasEpicEmu.log`) wird vorbereitet. Dadurch bleiben Netzwerkprobleme nachvollziehbar.

## Fazit
Aktuell stellt PartyDeck für jedes Profil eindeutige IDs bereit, aktiviert LAN-Broadcasting und behält Logs bei. Damit erfüllen die Profile die Voraussetzungen, damit Spieler sich gegenseitig finden. Zusätzliche Anforderungen (z. B. WebSocket-Signaling für Cross-NAT-Szenarien) lassen sich bei Bedarf direkt in den generierten JSON-Daten ergänzen.
