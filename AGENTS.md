# Agent Instructions

- Only run automated tests or checks when implementing large or complex changes. Small tweaks spanning just a few lines do not require executing the test suite.
- Comment each newly introduced code block to document its purpose clearly.
- Preserve existing functionality: double-check for syntax errors or regressions before finalizing changes.
- For UI changes, ensure a modern, well-aligned, and consistent presentation without unnecessary spacing around elements.
- Default Nemirtingas configuration log levels to debug severity so multiplayer invite issues remain inspectable.
- Persist launch warnings to a text log under the PARTY directory in addition to printing them to the console for easier debugging.
- Generate and persist unique Nemirtingas `EpicId`/`ProductUserId` pairs for each profile so invite codes stay stable between sessions.
- Prefer distinct UDP sockets for Nemirtingas LAN beacons and Goldberg discovery. Force `EOS_OVERRIDE_LAN_PORT` to the deterministic Nemirtingas port exposed in profile configs instead of mirroring Goldberg's `listen_port.txt`, since sharing the same socket causes the emulator to auto-increment per instance.
- When a handler ships a Nemirtingas config (`eos.config_path`), still normalize Goldberg `listen_port` deterministically but allow it to remain independent from Nemirtingas so both systems keep stable yet non-conflicting sockets.
- Default Goldberg `gc_token`/`new_app_ticket` toggles to `1` (files and INI flags) so the experimental steam_api build bundled in `res/goldberg` works without manual edits.
- Keep Goldberg's `auto_accept_invite.txt` empty when enabling auto-accept so the experimental overlay bypass matches upstream documentation; avoid writing sentinel values like `1`.
- Capture any newly provided project-wide user instructions in this file so they are not forgotten on future tasks.
- When a task exposes a recurring mistake or introduces a new global rule from the user, document it here immediately so future work remains aligned.
- Audit complete emulator logs before summarizing state transitions (e.g., JOINABLE flips) so transient values are not misreported.
