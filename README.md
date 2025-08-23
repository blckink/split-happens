<img src=".github/assets/icon.png" align="left" width="100" height="100">

### `PartyDeck`

A split-screen game launcher for Linux/SteamOS

---

<p align="center">
    <img src=".github/assets/launcher.png" width="49%" />
    <img src=".github/assets/gameplay1.png" width="49%" />
</p>

> [!IMPORTANT]
> This is the first serious software project I've ever done. It surely contains many violations of software best practices and security flaws; use at your own discretion! If you are experienced in software I would love to know what aspects of the codebase could be improved and how I can do better.

## Features

- Runs up to 4 instances of a game at a time and automatically fits each game window onto the screen
- Supports native Linux games as well as Windows games through Proton
- Handler system that tells the launcher how to handle game files, meaning very little manual setup is required
- Steam multiplayer API is emulated, allowing for multiple instances of Steam games
- Works with most game controllers without any additional setup, drivers, or third-party software
- Now works with multiple keyboards and mice!
- Uses sandboxing software to mask out controllers so that each game instance only detects the controller assigned to it, preventing input interference
- Profile support allows each player to have their own persistent save data, settings, and stats for games
- Works out of the box on SteamOS

## Installing & Usage

Download the latest release [here](https://github.com/wunnr/partydeck-rs/releases) and extract it into a folder. Download game handlers [here](https://drive.proton.me/urls/D9HBKM18YR#zG8XC8yVy9WL).

### SteamOS

SteamOS includes all of PartyDeck's dependencies, but you will need to be on SteamOS 3.7.0 or above for the splitscreen script to work.

If you're in desktop mode, simply run `partydeck-rs`. To use PartyDeck in Gaming Mode, add `partydeck-rs` as a non-Steam game by right-clicking that file and selecting "Add to Steam", then go into the properties of the non-Steam game, add `--kwin --fullscreen` to the launch options, and disable Steam Input.

### Desktop Linux

You'll need to install KDE Plasma, Gamescope, and Bubblewrap using your distro's package manager. Then, while in a KDE Plasma session, run `partydeck-rs` to get started. If you're running Steam, make sure none of the controllers are using a Steam Input desktop layout, as Steam Input causes issues such as duplicate controllers being detected.

### Getting Started
Once in the main menu, click the + button to add a game: this can be just a regular Linux executable, a Windows game (.exe), or a PartyDeck Handler (.pdh). Create profiles if you want to store save data, and have a look through the settings menu.

### Nemirtingas Epic Emu
Some games ship a patched `EOSSDK-Win64-Shipping.dll` that reads a
`NemirtingasEpicEmu.json` configuration. Handlers can expose this by
adding an `eos.config_path` field pointing to the expected location of the
JSON file **relative to the game's root directory**. This path should
include the file name itself. For example, if the DLL loads
`nepice_settings/NemirtingasEpicEmu.json` next to it, add
`"eos.config_path": "nepice_settings/NemirtingasEpicEmu.json"` to the
handler. PartyDeck will then create a per-profile JSON and bind it to that
location when launching the game. Each profile's file sets both `epicid`
and `username` to the profile name so player identity remains constant
across sessions. The patched `EOSSDK` DLL is **not** bundled with
PartyDeck; handlers should include it themselves. Place
`EOSSDK-Win64-Shipping.dll` inside the handler's `copy_to_symdir` folder
mirroring where the game expects it so PartyDeck can copy or symlink it
into the game directory at launch.


## Building

To build PartyDeck, you'll need a Rust toolchain installed with the 2024 Edition and a system installation of `gamescope`.
Clone the repo with submodules by running `git clone --recurse-submodules https://github.com/wunnr/partydeck-rs.git`.

In the main PartyDeck folder, run `build.sh`. This will build the executable and place it in the `build` folder along with the relevant dependencies and resources.


## How it Works

PartyDeck uses a few software layers to provide a console-like split-screen gaming experience:

- **KWin Session:** This KWin Session displays all running game instances and runs a script to automatically resize and reposition each Gamescope window.
- **Gamescope:** Contains each instance of the game to its own window. Also has the neat side effect of receiving controller input even when the window is not currently active, meaning multiple Gamescope instances can all receive input simultaneously
- **Bubblewrap:** Uses bindings to mask out evdev input files from the instances, so each instance only receives input from one specific controller. Also uses directory binding to give each player their own save data and settings within the games.
- **Runtime (Steam Runtime/Proton):** If needed, the app can run native Linux games through a Steam Runtime (currently, 1.0 (scout) and 2.0 (soldier) are supported) for better compatibility. Windows games are launched through UMU Launcher
- **Goldberg Steam Emu:** On games that use the Steam API for multiplayer, Goldberg is used to allow the game instances to connect to each other, as well as other devices running on the same LAN.
- **And finally, the game itself.**

## Known Issues, Limitations and To-dos

- AppImages and Flatpaks are not supported yet for native Linux games. Handlers can only run regular executables inside folders.
- "Console-like splitscreen experience" means single-screen only for now. Multi-monitor support is possible but will require a better understanding of the KWin Scripting API.
- Controller navigation support in the launcher is super primitive; I'd love to try making a more controller-friendly, Big-Picture-style UI in the future, but have no immediate plans for it.
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
