# Kumi

Personal Wayland desktop shell built on [`kumizo-kit`](https://github.com/noobmaisfier/kumizo), `gpui` (`gpui-pre`), and `gpui-component` — single process, GPU-rendered, with a size-optimized release profile (`opt-level = "z"`, thin LTO, stripped).

Produces two binaries (`kumi` and `kumi-lockscreen`), controlled via the `kumizo` CLI.

## Features & Modules

| Module | Window ID | Role |
| --- | --- | --- |
| `src/bar.rs` | `kumi-bar` | Responsive top bar with clock, MPRIS media pill, tray, and circular gauge rings (volume, brightness, RAM) |
| `src/launcher.rs` | `kumi-launcher` | Fullscreen app launcher |
| `src/todo.rs`, `src/todo_store.rs` | `kumi-todo` | Hybrid Focus / Todo / Kanban widget with persistent local storage |
| `src/emoji_picker.rs` | `kumi-emoji` | Searchable categorized emoji picker |
| `src/control.rs` | `kumi-control` | Control center (audio, Bluetooth, Wi-Fi/network, brightness, DnD) |
| `src/clipboard.rs` | `kumi-clipboard` | Clipboard history manager |
| `src/tasks.rs` | `kumi-tasks` | 2D window switcher (`TaskSwitcher`) and window actions |
| `src/notifs.rs`, `src/toasts.rs` | `kumi-notifs`, `kumi-toasts` | Notification history panel + ephemeral toast popups |
| `src/power.rs` | `kumi-power` | Power menu |
| `src/pomodoro.rs` | — | Focus / Pomodoro timer |
| `src/osd.rs` | `kumi-osd` | Volume and brightness on-screen display |
| `src/theme.rs`, `src/theme_ctl.rs`, `src/theme_picker.rs` | `kumi-themes` | 11 built-in themes with live switching (`~/.config/kumi/theme`) |
| `src/lockscreen.rs` | — | PAM-backed Wayland lockscreen (`kumi-lockscreen` binary) |
| `src/kit.rs`, `src/ui.rs`, `src/icons.rs` | — | Shared design tokens, animations, and UI helpers |

## Install (Arch Linux)

Prebuilt packages are published in the [`noobmaisfier/repo`](https://github.com/noobmaisfier/repo) custom pacman repository. Add it once to `/etc/pacman.conf`:

```ini
[noobmaisfier]
SigLevel = Optional TrustAll
Server = https://github.com/noobmaisfier/repo/releases/download/$arch
```

Then install `kumi` (provides `kumi`, `kumi-lockscreen`, and `kumizo`):

```bash
sudo pacman -Syu kumi
```

## Build from source

```bash
cargo build --locked --release
# Binaries:
#   target/release/kumi
#   target/release/kumi-lockscreen

# CLI controller (matches kumizo-kit 0.12.1):
cargo install --locked kumizo@0.12.1
```

### Dependencies

- **Runtime / compositor**: Wayland compositor supporting `wlr-layer-shell` (e.g. `sway` / [`swayfs`](https://github.com/noobmaisfier/swayfs)), PipeWire/ALSA, PAM, `playerctl` (MPRIS pill), Nerd Font (icons)
- **Arch Linux build deps**: `cargo pkgconf wayland libxkbcommon libxkbcommon-x11 fontconfig alsa-lib pam vulkan-icd-loader`
- **Debian build deps**: `build-essential pkg-config libwayland-dev libxkbcommon-dev libxkbcommon-x11-dev libfontconfig1-dev libasound2-dev libpam0g-dev libvulkan1`

## Control with `kumizo`

```bash
kumizo ping
kumizo status
kumizo toggle kumi-launcher
kumizo toggle kumi-todo
kumizo toggle kumi-emoji
kumizo toggle kumi-control
kumizo toggle kumi-tasks
kumizo toggle kumi-clipboard
kumizo toggle kumi-notifs
kumizo toggle kumi-power
kumizo toggle kumi-bar
kumizo service theme next
kumizo service theme set "Flexoki Light"
kumizo service mpris next
```

## Layout

```text
Cargo.toml          dependencies and size-optimized release profile
Cargo.lock          locked dependency graph
src/main.rs         services, layer-shell windows, and app entry point
src/lockscreen.rs   standalone PAM lockscreen binary
src/ui.rs           window IDs (BAR, LAUNCHER, TODO, EMOJI, ...) and helpers
src/theme.rs        11 built-in palettes and semantic color tokens
```

## Relationship with A28

Used as the personal modified Kumi shell alongside the [A28](https://github.com/noobmaisfier/A28) Sway dotfiles (`me.sh`).

## License

Dual-licensed under `MIT OR Apache-2.0` (`LICENSE-MIT`, `LICENSE-APACHE`).
