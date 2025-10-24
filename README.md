<img src=".github/assets/icon.png" align="left" width="100" height="100">

### `Split Happens`

A Steam Deck-optimized fork of the PartyDeck split-screen launcher for Linux/SteamOS.

---

<p align="center">
    <img src=".github/assets/launcher.png" width="49%" />
    <img src=".github/assets/gameplay1.png" width="49%" />
</p>

> [!IMPORTANT]
> Split Happens builds on the incredible groundwork laid by PartyDeck. While this fork focuses on Steam Deck polish and new
> platform integrations, please continue sharing feedback so we can keep tightening the experience for everyone.

## What's new in Split Happens

- Steam Deck-first tuning with Gamescope, Proton, and performance defaults ready out of the box.
- Controller-optimized interface with focus-friendly layouts ideal for couch play and Big Picture Mode.
- Epic Online Services support through integrated Nemirtingas configuration management.
- Refined profile, handler, and logging flows tailored for multi-user Deck setups.

## Core features

- Runs up to 4 instances of a game at a time and automatically fits each game window onto the screen.
- Supports native Linux games as well as Windows games through Proton.
- Handler system that tells the launcher how to handle game files, meaning very little manual setup is required.
- Steam multiplayer API is emulated, allowing for multiple instances of Steam games.
- Works with most game controllers without any additional setup, drivers, or third-party software.
- Multi-keyboard and mouse support for local co-op titles that need dedicated desktop input.
- Uses sandboxing software to mask out controllers so that each game instance only detects the controller assigned to it, preventing input interference.
- Profile support allows each player to have their own persistent save data, settings, and stats for games.
- Works out of the box on SteamOS.

## Installing & usage

Download the latest release [here](https://github.com/blckink/suckmydeck/releases) and extract it into a folder. Download game handlers [here](https://drive.proton.me/urls/D9HBKM18YR#zG8XC8yVy9WL).

### SteamOS

SteamOS already ships everything Split Happens needs, but make sure you're running SteamOS 3.7.0 or later for the splitscreen script.

If you're in desktop mode, simply run `split-happens`. To use Split Happens in Gaming Mode, add `split-happens` as a non-Steam game by right-clicking that file and selecting "Add to Steam". Then open the game's properties, append `--kwin --fullscreen` to the launch options, and disable Steam Input.

For an even smoother Big Picture experience, mark the provided `split_happens_big_picture.sh` script as executable and add it to Steam instead. The helper locates the launcher binary automatically and forwards `--kwin --fullscreen` so you can start Split Happens directly from the couch UI.

### Desktop Linux

Install KDE Plasma, Gamescope, and Bubblewrap using your distro's package manager. While in a KDE Plasma session, run `split-happens` to get started. If you're running Steam, make sure none of the controllers are using a Steam Input desktop layout, as Steam Input causes issues such as duplicate controllers being detected.

### Getting started

Once in the main menu, click the + button to add a game: this can be just a regular Linux executable, a Windows game (.exe), or a Split Happens Handler (.pdh). Create profiles if you want to store save data, and have a look through the settings menu.

### Nemirtingas Epic Emu

Some games ship a patched `EOSSDK-Win64-Shipping.dll` that reads a `NemirtingasEpicEmu.json` configuration. Handlers can expose this by adding an `eos.config_path` field pointing to the expected location of the JSON file **relative to the game's root directory**. This path should include the file name itself. For example, if the DLL loads `nepice_settings/NemirtingasEpicEmu.json` next to it, add `"eos.config_path": "nepice_settings/NemirtingasEpicEmu.json"` to the handler. Split Happens will then create a per-profile `nepice_settings` folder containing `NemirtingasEpicEmu.json` and bind it to that location when launching the game so logs and config live per profile. Each profile's JSON sets `username` to the profile name, `language` to `"en"`, `appid` to a fixed game identifier, and `log_level` to `"DEBUG"`. The patched `EOSSDK` DLL is **not** bundled with Split Happens; handlers should include it themselves. Place `EOSSDK-Win64-Shipping.dll` inside the handler's `copy_to_symdir` folder mirroring where the game expects it so Split Happens can copy or symlink it into the game directory at launch.

### Goldberg Steam API overrides

Handlers can opt into a custom Goldberg build on a per-game basis. To do so, point `steam.api_path` in the handler JSON to the folder that should contain Goldberg inside the game directory (for example, `"steam.api_path": "Engine/Binaries/ThirdParty/Steamworks/Steamv147/Win64"`). When Split Happens prepares the instance folder, it binds that directory and copies Goldberg's default files there. If the handler bundles a patched `steam_api64.dll`, `steam_api.dll`, or `libsteam_api.so`, place those files beside the handler JSON (the same directory that contains `handler.json`). Split Happens automatically copies the override matching the platform/architecture into the Goldberg directory, letting specific handlers keep using their known-good Steam API build without impacting other games.

## Building

To build Split Happens, you'll need a Rust toolchain installed with the 2024 Edition and a system installation of `gamescope`. Clone the repo with submodules by running `git clone --recurse-submodules https://github.com/blckink/suckmydeck.git`.

In the main Split Happens folder, run `build.sh`. This will build the executable and place it in the `build` folder along with the relevant dependencies and resources.

## How it works

Split Happens uses a few software layers to provide a console-like split-screen gaming experience:

- **KWin Session:** This KWin Session displays all running game instances and runs a script to automatically resize and reposition each Gamescope window.
- **Gamescope:** Contains each instance of the game to its own window. Also has the neat side effect of receiving controller input even when the window is not currently active, meaning multiple Gamescope instances can all receive input simultaneously.
- **Bubblewrap:** Uses bindings to mask out evdev input files from the instances, so each instance only receives input from one specific controller. Also uses directory binding to give each player their own save data and settings within the games.
- **Runtime (Steam Runtime/Proton):** If needed, the app can run native Linux games through a Steam Runtime (currently, 1.0 (scout) and 2.0 (soldier) are supported) for better compatibility. Windows games are launched through UMU Launcher.
- **Goldberg Steam Emu:** On games that use the Steam API for multiplayer, Goldberg is used to allow the game instances to connect to each other, as well as other devices running on the same LAN.
- **And finally, the game itself.**

## Known issues, limitations, and to-dos

- AppImages and Flatpaks are not supported yet for native Linux games. Handlers can only run regular executables inside folders.
- "Console-like splitscreen experience" means single-screen only for now. Multi-monitor support is possible but will require a better understanding of the KWin Scripting API.
- Controller navigation is vastly improved, but we're always interested in refinements to focus order, haptics, and Steam Input glyphs.
- Games using Goldberg might have trouble discovering LAN games from other devices. If this happens, you can try adding a firewall rule for port 47584. If connecting two Steam Decks through LAN, their hostnames should be changed from the default "steamdeck".

## Credits/Thanks

- Valve for [Gamescope](https://github.com/Plagman/gamescope)
- [@blckink](https://github.com/blckink) for contributions
- MrGoldberg & Detanup01 for [Goldberg Steam Emu](https://github.com/Detanup01/gbe_fork/)
- GloriousEggroll and the rest of the contributors for [UMU Launcher](https://github.com/Open-Wine-Components/umu-launcher)
- Inspired by [Tau5's Coop-on-Linux](https://github.com/Tau5/Co-op-on-Linux) and [Syntrait's Splinux](https://github.com/Syntrait/splinux)
- Talos91 and the rest of the Splitscreen.me team for [Nucleus Coop](https://github.com/SplitScreen-Me/splitscreenme-nucleus), and for helping with handler creation

## Disclaimer
This software has been created purely for the purposes of academic research. It is not intended to be used to attack other systems. Project maintainers are not responsible or liable for misuse of the software. Use responsibly.
