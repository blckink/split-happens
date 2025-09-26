# Coral Island Multiplayer Debug Playbook

## Snapshot Overview
- `debug/aktuelle-config/coral gbe/` mirrors the live Goldberg directory that ships with Coral Island, including the emulator DLL and its `steam_settings` folder.【87aed6†L1-L1】【5e72c8†L1-L2】
- `debug/aktuelle-config/coral nemirtingas/Win64/` reflects the in-game Nemirtingas drop-in with the patched `EOSSDK-Win64-Shipping.dll` and a placeholder `nepice_settings` directory used at runtime.【146ca3†L1-L1】【1446b1†L1-L4】
- `debug/aktuelle-config/coral profiles/tyde/` captures a PartyDeck profile. The profile-level Nemirtingas JSON already contains unique `EpicId`/`ProductUserId` pairs and keeps logs within the profile save tree.【399901†L1-L1】【652479†L1-L44】

## Goldberg Steam Emu Checklist
1. **Essential files** – confirm `steam_appid.txt`, `configs.user.ini`, and `steam_interfaces.txt` live inside `steam_settings/`. The sample snapshot contains all three, but the files lack trailing newlines which can make quick `cat` checks confusing; open them in an editor to verify content.【9dd9b3†L1-L4】【a254d3†L1-L1】【b2fd4a†L1-L2】
2. **AppID sanity** – the bundled `steam_appid.txt` currently reads `1158160`. Ensure the handler JSON expects the same ID or update the file so the launcher’s new validation logs do not warn about mismatched AppIDs.【a254d3†L1-L1】【F:src/launch.rs†L158-L214】
3. **Per-profile saves** – PartyDeck mounts each profile’s `steam` directory into `<game>/.../goldbergsave`, so keep `configs.user.ini` focused on per-user metadata like `account_name`, `account_steamid`, and optional `local_save_path`. The snapshot only sets `local_save_path`, so add account information if Goldberg needs it for lobbies.【16d674†L130-L172】【b2fd4a†L1-L2】
4. **Interface regeneration** – if Goldberg emits interface mismatch errors, regenerate `steam_interfaces.txt` with the included `generate_interfaces` utility and drop the new file into `steam_settings/` for both profiles before launching.【9dd9b3†L1-L4】

## Nemirtingas Epic Emu Checklist
1. **Profile JSON** – PartyDeck writes one JSON per profile and now logs when that file binds into the handler path. Double-check usernames and IDs in each profile’s JSON before launch to avoid EOS account collisions.【4805f0†L36-L69】【652479†L1-L44】
2. **Runtime target** – the handler expects `nepice_settings/NemirtingasEpicEmu.json` inside the game directory. The shipped copy at runtime is currently empty (0 bytes), so verify the symlink/bind actually points to the profile JSON when the game boots.【29736b†L18-L25】【0a9bf4†L1-L10】
3. **EOS binaries** – place `EOSSDK-Win64-Shipping.dll` (and any companion `EOSShared` libraries) in the same or parent directories so the launcher’s EOS scan finds them and suppresses the missing-DLL warning.【1446b1†L1-L4】【3317fc†L49-L96】
4. **Network plugins** – the profile JSON disables Broadcast/WebSocket by default. Enable Broadcast for LAN discovery or configure WebSocket signaling servers if you rely on Nemirtingas relays.【652479†L24-L44】

## Diagnosing “Host Disconnected”
1. Launch PartyDeck and watch the console for the new Goldberg/Nemirtingas diagnostics. Any missing files or ID mismatches will emit `[PARTYDECK][WARN]` messages and persist to `logs/launch_warnings.txt` for later review.【3317fc†L17-L47】【F:src/launch.rs†L158-L214】
2. If Nemirtingas still reports “offline,” inspect the runtime copy of `NemirtingasEpicEmu.json` to ensure it is non-empty and mirrors the profile file; the captured snapshot shows the runtime file was zero bytes, which keeps EOS from initializing.【0a9bf4†L1-L10】【652479†L1-L44】
3. When Goldberg claims offline mode, verify `configs.user.ini` defines unique `account_steamid` values per profile and that `steam_appid.txt` matches the running build. Without these, Goldberg can start but refuse to advertise sessions.【b2fd4a†L1-L2】【F:src/launch.rs†L158-L214】
4. Review `debug/launch_warnings.txt` and `log.txt` for historical context; repeated warnings about missing EOSSDK files indicate the handler never saw the patched DLLs at runtime, which lines up with Nemirtingas failing to go online.【d1e225†L1-L5】【29736b†L18-L25】

## Recommended Workflow Before the Next Session
1. Refresh Goldberg/Nemirtingas assets inside the handler directory, copying the patched DLLs and regenerated configs from `debug/aktuelle-config/` as needed.
2. For each player profile, open the profile JSON and Goldberg configs, update usernames/IDs, and delete any stale `goldbergsave` caches.
3. Start PartyDeck in desktop mode, host a test session, and capture the console plus `logs/launch_warnings.txt` if problems persist. Attach those logs alongside updated snapshots for faster triage next time.【3317fc†L17-L47】【d1e225†L1-L5】

## Latest Debug Findings (Tyde ↔ Treuhandtina Snapshot)
- **Goldberg ports lined up, Nemirtingas drifted** – both profiles already expose `listen_port=20902`, and PartyDeck synchronized the Goldberg metadata to that value.【0e130b†L1-L1】【7407f1†L1-L1】【5dd15f†L88-L95】 Nemirtingas, however, booted its LAN beacon on ephemeral ports `55789` (host) and `55790` (client), so EOS invite discovery and Goldberg ran on different sockets.【F:debug/tyde/nepice_settings/applogs.txt†L34-L35】【F:debug/treuhandtina/nepice_settings/applogs.txt†L34-L35】 PartyDeck now inlines the synchronized listen port into each profile’s `NemirtingasEpicEmu.json` (and keeps exporting `EOS_OVERRIDE_LAN_PORT`) so fresh launches reuse 20902 without manual edits.【F:src/util/profiles.rs†L329-L333】【F:src/util/profiles.rs†L528-L579】【F:src/launch.rs†L610-L618】
- **Host lobby briefly flips `JOINABLE = 0` before settling at `1`** – Tyde’s log shows the attribute toggling to `0` during initialization but switching to `1` less than a second later, so the lobby does advertise an open slot once ready.【F:debug/tyde/nepice_settings/applogs.txt†L52-L176】 Focus debugging on discovery rather than treating `JOINABLE` as permanently disabled.
- **Shared EOS credentials are expected** – both profiles load identical `ClientId`, `ClientSecret`, and `ProductId` because they identify the Coral Island application, not per-user accounts.【F:debug/tyde/nepice_settings/applogs.txt†L20-L30】【F:debug/treuhandtina/nepice_settings/applogs.txt†L30-L34】 Authentication uniqueness still comes from the distinct `EpicId`/`ProductUserId` pairs PartyDeck writes per profile.【F:debug/tyde/nepice_settings/NemirtingasEpicEmu.json†L7-L13】【F:debug/treuhandtina/nepice_settings/NemirtingasEpicEmu.json†L7-L13】
- **Client search loops without results** – Treuhandtina’s log shows a flood of `EOS_LobbySearch_FindCallbackInfo` callbacks with no subsequent join, matching the “Session not found” prompt after entering KS24YL.【edcd70†L45-L68】【1ebde7†L62-L73】 Aligning the LAN port and ensuring `JOINABLE` transitions to `1` are the first fixes to try before digging into NAT or firewall rules.
