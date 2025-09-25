# Agent Instructions

- Only run automated tests or checks when implementing large or complex changes. Small tweaks spanning just a few lines do not require executing the test suite.
- Comment each newly introduced code block to document its purpose clearly.
- Preserve existing functionality: double-check for syntax errors or regressions before finalizing changes.
- For UI changes, ensure a modern, well-aligned, and consistent presentation without unnecessary spacing around elements.
- Default Nemirtingas configuration log levels to debug severity so multiplayer invite issues remain inspectable.
- Persist launch warnings to a text log under the PARTY directory in addition to printing them to the console for easier debugging.
