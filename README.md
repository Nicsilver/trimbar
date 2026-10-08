<p align="center"><img src="assets/icon.png" width="96" alt="Trimbar icon"></p>

<h1 align="center">Trimbar</h1>

<p align="center">Hide the dead rows at the bottom of a dying monitor.</p>

Some cheap panels start failing at the bottom edge: a band of rows goes dark, smeared or garbled, and the taskbar, window edges and the bottom of maximized windows disappear into it. GPU drivers let you make a smaller custom resolution, but they always center it, so the dead rows stay in use.

Trimbar is a tiny tray app for Windows and menu bar app for macOS that reserves a strip at the bottom of any monitor. Maximized, snapped and fullscreen windows stop above it, so nothing you need ends up in the broken part of the screen. Each monitor gets its own height.

## Setup

1. Download the zip for your system from [Releases](https://github.com/Nicsilver/trimbar/releases):
   - **Windows:** extract `trimbar.exe` and `trimbar_hook.dll` from `trimbar.zip` into the same permanent folder (for example `%LOCALAPPDATA%\Programs\Trimbar`).
   - **macOS:** extract `Trimbar.app` from `trimbar-macos.zip` and move it to `Applications`.
2. Run it. A setup panel opens on every monitor.
3. On a broken monitor, raise the value (arrow keys, scroll wheel or the buttons) until a red bar shows up at the bottom of the screen.
4. Lower it until the red is just gone. Everything the red bar covers is now hidden from windows.
5. Press **Save**.

Monitors left at **Off** are untouched. Click the tray or menu bar icon any time to adjust again, or launch the app a second time. Right-click the icon for the other options.

## Windows

- Reserves the strip the same way the taskbar reserves its space, so the work area itself gets smaller.
- Starts with Windows by default. Toggle it in the tray menu.
- Settings live in `%APPDATA%\Trimbar\config.txt`, keyed by the monitor's device path, so they survive display renumbering and still tell two identical monitors apart.
- Handles monitors being plugged in or out, resolution changes and Explorer restarts.
- Fullscreen windows ignore the reserved space. Browsers (Chrome, Edge, Brave, Vivaldi, Opera, Firefox) force their fullscreen window back to the full monitor on every click, so Trimbar loads `trimbar_hook.dll` into them, which reports trimmed monitors as shorter. The browser then lays out fullscreen video above the strip by itself. Other fullscreen windows (players, borderless games) that exactly cover a trimmed monitor are shrunk from outside; apps that keep snapping back, like exclusive-fullscreen games, are left alone after a few tries. Toggle it in the tray menu.
- If the Windows taskbar is shown on a trimmed monitor, it may end up above or below the strip. Turn off "Show my taskbar on all displays" for the cleanest result.
- About 200 KB, no dependencies, around 10 MB of RAM, no CPU while idle.

The exe isn't code-signed, so SmartScreen may warn on first run ("More info" then "Run anyway").

## macOS

- macOS has no way to shrink the area windows zoom and tile into, so Trimbar watches windows instead. When a window ends inside the strip, it is pushed back above it: zoomed and tiled windows are shortened, other windows are moved up. Windows are left alone while you drag them, and apps that keep snapping back are left alone after a few tries. Toggle it in the menu.
- It needs Accessibility access to move other apps' windows. macOS asks on first launch; you can also grant it under System Settings, Privacy & Security, Accessibility, or from the menu.
- Fullscreen video in Chrome, Edge, Brave, Vivaldi and Arc gets its own space that no other app can resize, so a small browser extension shrinks it from the inside instead: the whole player, controls included, is scaled down to end above the strip, with black where the strip is. Install it once from the menu (**Install Chrome extension…**): turn on Developer mode on the extensions page, click **Load unpacked** and pick the folder Finder shows. The extension asks Trimbar for the trims, so there is nothing to set up in it.
- Other native fullscreen windows (the green button) can't be resized either, so they still cover the dead rows.
- If the Dock sits at the bottom of a trimmed display, it stays in the strip. Move it to the side or put it on another display.
- Starts at login by default. Toggle it in the menu.
- Settings live in `~/Library/Application Support/Trimbar/config.txt`, keyed by the display's UUID. Heights are in points, so on a Retina display 1 step is 2 pixels.
- Universal app (Apple silicon and Intel), macOS 13 or later.

The app isn't notarized, so macOS blocks it on first launch. Open System Settings, Privacy & Security, and click **Open Anyway** next to the Trimbar message.

## Building

Windows:

```
cargo build --release --workspace
```

macOS (needs the Xcode command line tools):

```
macos/build.sh
```

The app is signed ad hoc, so every rebuild counts as a new app for Accessibility: remove Trimbar from the list and add it again after rebuilding.

The icons are generated by `python tools/make_icon.py` (needs Pillow).

## License

MIT
