# Azoth Scope

**Language:** **English** · [简体中文](./README.zh-CN.md)

<img src="assets/azoth-scope.png" width="72" height="72" alt="Azoth Scope icon: a mint dial and pointer" />

See CPU and GPU usage history on the OLED of the original ROG Azoth (M701). Azoth Scope is a portable Windows tray app written in Rust. By default, it samples every two seconds and shows the last two minutes. It uses the Windows HID driver; there is nothing to import into Armoury Crate.

## Getting started

![Azoth Scope dashboard with a live OLED preview, CPU and GPU readings, and display settings](assets/dashboard.png)

1. Put `azoth-scope.exe` in a folder you intend to keep and run it. On first launch, the app shows a preview without writing to the keyboard. Choose **English** or **简体中文** under **Language** in the upper-right corner. The setting also applies to status messages and the tray menu and is remembered on restart.
2. Choose **Single graph** or **Two graphs** and select a source for each. By default, the first source is the CPU and the second is the GPU with the most dedicated video memory. You can compare two GPUs instead. Selecting the source already in the other graph swaps the two.
3. Check the preview, then click **Start OLED output**. **Full image** mode overwrites the keyboard’s custom image slot, so keep the original file if you want to restore it. The status bar reports the connection and any device errors. Missing readings appear as `—` on the dashboard and as gaps in split lines.
4. Close the window with **×** to hide it. Double-click the tray icon to reopen settings, or right-click to pause, resume, or exit. Pausing stops further writes; a transfer already in progress may finish, and the last frame can remain on screen. To restore a factory display, pause output and reapply the desired preset in Armoury Crate.

**History** sets the time span (1–30 minutes). **Refresh** sets the sampling interval (1–30 seconds). Both can be changed while output is running. The app keeps up to 30 minutes of timestamped samples in memory: changing either setting preserves existing data, but cannot fill in time before the app was running. Actual image refresh speed depends on the firmware and transfer time.

Starting at sign-in is off by default. Enable **Start in tray at sign-in** if you want it, and turn it off before moving the EXE. Preferences, including whether OLED output was enabled, are stored in `%LOCALAPPDATA%\AzothScope\settings.json`. History is not saved to disk. The app does not require a network or a separate sensor service.

## Device support

**Keyboard battery:** The dashboard shows the M701’s remaining charge, read from the keyboard about every 30 seconds over its control channel. The OLED also shows a small battery icon and percentage (in Native trends, enable **Show labels on OLED**). An unavailable reading is shown as a dash. Preview-only mode can read the charge without changing the keyboard image.

| Device or connection | Status |
| --- | --- |
| Original M701, wired USB (`0B05:1A83`), Windows 11 x64 | Tested on hardware. Full image and Banner both display updates but can briefly go black on refresh. |
| 2.4 GHz receiver (`0B05:1A85`) or OMNI (`0B05:1ACE`) | Implemented, not yet hardware-verified. With OMNI, the app checks pairing before selecting the M701 control report. |
| Bluetooth, Azoth Extreme, other keyboards | Not supported. |

CPU usage is calculated from Windows system time counters. GPUs are enumerated with DXGI and matched to Windows GPU Engine counters by adapter LUID. The app sums usage across processes on each engine, then reports the busiest engine for each GPU. Sampling intervals, numbering, and rounding can differ from Task Manager; **choose your GPU by model name**. Restart the app after replacing a GPU or driver to refresh the device list.

The custom image area is 208×64 grayscale pixels. The app changes the OLED display, not key mappings, the control knob, firmware, or onboard profiles. The long-term endurance of repeated custom image slot updates and behavior across firmware versions have not been verified. The preview continues if the keyboard disconnects or sleeps, and the app retries output. Armoury Crate services are left running; when both apps update the OLED, one can overwrite the other’s display.

## Graph styles and labels

Choose **Native trends** under **Output channel**. If another channel is active, pause output before changing it. Once in Native trends, you can switch **Style** without pausing or clearing history.

| Style | What you see |
| --- | --- |
| **Split lines** | In the two-graph layout, each side is drawn independently from timestamped samples. The center stays clear and the baseline sits four pixels above the bottom. Changed frames use the media-information channel. Consecutive frame acknowledgements have been observed over USB; judge the visible result on the keyboard itself. |
| **Bar chart** | The keyboard draws the history using its spectrum engine. A single graph uses 26 points; two graphs use 12 points per side with two zero points between them. On the tested wired M701, this avoided the black flash seen in image mode. |
| **Continuous line** | Uses the same spectrum data as Bar chart, but the firmware joins adjacent points, including the two center zero points. Choose Split lines if you want a visible break. |

**Show labels on OLED** is on by default. In a two-graph layout, labels such as `CPU` and `GPU0` identify the left and right sources; swapping sources swaps their labels. A single label is centered. Within each graph the newest data is on the right. The dashboard shows the exact current percentages. The native spectrum implementation draws on [Omaglow’s M701 protocol notes and code](https://github.com/peraltcant/omaglow/blob/main/src/protocol.cpp).

## Checking the physical display

Start with USB and leave Armoury Crate running. Compare the keyboard with the preview for a full history window, then try switching layouts or GPU sources. Pause output and reapply an Armoury Crate preset to check that the app releases the display. For a 2.4 GHz test, pause before changing connection and resume afterward. When reporting a problem, include the connection type, keyboard firmware version, graph style, and status message.

**If the screen flashes black:** on the tested M701, both Full image and Banner flashed on refresh and stopped flashing when paused. The factory **System Information** display did not flash on its own. USB observations showed both image acknowledgements (`61`) and factory metric acknowledgements (`66:00`) while both apps were active. With the factory fireworks preset selected instead, the factory metric updates stopped but the black flash remained. Competing metric updates are therefore not required for this symptom. Try **Native trends → Bar chart** as a comparison.

**If the onboard preset briefly reappears:** leave the settings unchanged and note when it happens. The following developer tools are read-only and support wired M701 devices:

- `cargo run --locked --example observe_oled` counts OLED acknowledgements for 12 seconds. Add `-- --timeline 300` for a five-minute UTC trace of mode acknowledgements, frame timing, firmware rejections, and disconnects. It does not record images or keystrokes. Native `67:02` data often has no per-frame acknowledgement; silence alone does not mean a disconnect.
- For a visual event marker, run `cargo build --locked --example observe_oled`, then `.\examples\mark-flash.ps1`. Click **Saw the wallpaper** when the preset appears. Markers and traces share a UTC clock and are saved under `target\oled-event-*` (`.marks`, `.log`, `.err`). The session lasts ten minutes by default; close the marker window to stop early.

## Building from source

On Windows, install stable Rust with the MSVC toolchain, Visual Studio C++ Build Tools, and the Windows SDK. From the repository root, run:

```powershell
cargo test --locked
cargo build --release --locked
.\target\release\azoth-scope.exe
```

To check the performance counters and OLED control interface without uploading an image, run `cargo run --locked -- --diagnose`. It prints the device and one sample; OMNI uses a read-only pairing query. The [Windows build workflow](.github/workflows/windows.yml) produces a downloadable x64 artifact on GitHub Actions.

## Credits and license

The OLED frame protocol and receiver handling are informed by [community M701 hardware tests](https://github.com/awesomehy/rog-azoth-oled). This independent Rust implementation does not require that project, Gear Link Companion, or an ASUS SDK. It is not official ASUS software. See the [ASUS OLED guide](https://rog.asus.com/us/support/faq/1050647/) for factory features.

Azoth Scope is licensed under the [MIT License](LICENSE).
