# Color SSH TUI User Guide

This guide explains how to use the interactive TUI (session manager) in `color-ssh` (`cossh`) as an end user.

## Start the TUI

Use one of these commands:

```bash
cossh
cossh -d
```

Notes:

- `cossh` starts the interactive session manager.
- `cossh -d` also starts the session manager, with debug logging enabled.
- If you pass an SSH target (for example `cossh user@host`), `cossh` runs direct connection mode instead of opening the TUI.

## Understand the Layout

The TUI has these areas:

- Host panel (left): SSH host tree and optional host info pane.
- Main pane (right): host details (when no tabs are open) or terminal tabs (when sessions are open).
- Tab bar: appears at the top of the main pane when tabs are open.
- Status bar (bottom): active context and key hints.

Focus matters:

- Manager focus: keys control host tree and manager UI.
- Terminal focus: keys mostly go to the active SSH terminal tab.

Switch focus:

- `Shift+Tab`: toggle between manager and terminal focus (when tabs exist).

## Host Manager Basics

Typical flow:

1. Use arrow keys to select a host or folder.
2. Press `Enter` on a host to open it in a new tab.
3. Press `Shift+Tab` to move focus to the terminal tab.

Manager keybindings:

| Key | Action |
| --- | --- |
| `Up` / `Down` | Move selection |
| `PageUp` / `PageDown` | Move by page |
| `Home` / `End` | Jump to top/bottom |
| `Left` | Collapse selected folder |
| `Right` | Expand selected folder |
| `Enter` | Toggle folder, or open selected host |
| `c` | Collapse/expand all folders (when no filter is active) |
| `i` | Toggle host info pane |
| `q` | Open Quick Connect modal |
| `Ctrl+F` | Start host search mode |
| `Ctrl+Left` / `Ctrl+Right` | Resize host panel width |
| `Shift+Tab` | Switch focus to terminal tabs |
| `Ctrl+Q` | Quit TUI |

Filter clearing:

- In manager mode, `Ctrl+C` clears the current host filter if one exists.

## Host Search Mode

Enter host search from manager focus with `Ctrl+F`.

Host search keys:

| Key | Action |
| --- | --- |
| `Type text` | Update filter |
| `Backspace` | Delete one character |
| `Enter` | Exit search mode and keep current filter |
| `Esc` | Exit search mode and clear filter |
| `Ctrl+C` | Exit search mode and clear filter |

Search behavior:

- Matches host alias, `HostName`, and `User`.
- Matching is case-insensitive.
- Uses strict contiguous matching first; falls back to fuzzy matching if needed.
- Matching folders auto-expand during search.

## Terminal Tabs

After opening a host, a tab appears in the main pane.

Terminal/tab keybindings:

| Key | Action |
| --- | --- |
| `Shift+Tab` | Move focus back to manager |
| `Alt+Left` / `Alt+Right` | Switch active tab |
| `Ctrl+Left` / `Ctrl+Right` | Move tab left/right (reorder) |
| `Ctrl+W` | Close current tab |
| `Ctrl+B` | Show/hide host panel |
| `Shift+PageUp` / `Shift+PageDown` | Scroll terminal scrollback |
| `Ctrl+F` | Start terminal search mode (unless remote app mouse mode captures it) |
| `Alt+C` | Copy current selection to clipboard |
| `Enter` | Reconnect if session exited; otherwise send Enter to terminal |
| `Ctrl+O` | Exit TUI and open current host in direct `cossh` mode |

Behavior notes:

- Most keys are forwarded to the remote SSH session while terminal has focus.
- When you type in terminal focus, scrollback view resets to live output.
- `Ctrl+Q` does not quit from terminal focus. Use `Shift+Tab`, then `Ctrl+Q`.

## Terminal Search Mode

Enter terminal search with `Ctrl+F` in terminal focus.

Terminal search keys:

| Key | Action |
| --- | --- |
| `Type text` | Update search query |
| `Backspace` | Delete one character |
| `Enter` / `Down` | Next match |
| `Up` | Previous match |
| `Esc` | Clear and exit terminal search |

Search behavior:

- Literal, case-insensitive search over terminal buffer.
- All matches are highlighted.
- Current match is highlighted separately and the viewport jumps to it.

## Quick Connect Modal

Open Quick Connect with `q` from manager focus.

Fields:

- `User` (optional)
- `Host` (required)
- `Profile` (from discovered config profiles)
- `SSH Logging` toggle
- `Connect` action

Quick Connect keys:

| Key | Action |
| --- | --- |
| `Tab` / `Down` | Next field |
| `Shift+Tab` / `Up` | Previous field |
| `Left` / `Right` | Change profile (when Profile field is selected) |
| `Space` | Toggle SSH logging (when Logging field is selected) |
| `Enter` | Field-dependent action (toggle/cycle/submit) |
| `Esc` | Cancel and close modal |

Notes:

- Host is required. If empty, the modal shows an error and keeps focus on `Host`.
- Profile list is discovered from config filenames like `*.cossh-config.yaml` in the active config directory.
- `default` profile means no `-P` is passed.
- SSH logging toggle maps to the `-l` behavior for the connection opened from this modal.

## Mouse Controls

Host panel:

- Click a row to select.
- Click a folder to expand/collapse.
- Double-click a host to open a tab.
- Mouse wheel over host list moves selection.
- Drag host list scrollbar thumb to scroll long lists.
- Drag vertical divider between host panel and main pane to resize width.
- Drag horizontal divider above info pane to resize info pane height.

Tab bar:

- Click tab title to focus tab.
- Click `x` on a tab to close it.
- Drag a tab title to reorder tabs.
- Click overflow markers (`<` / `>`) to scroll tab strip.
- Mouse wheel over tab bar switches tabs.

Terminal pane:

- Drag to select text.
- Right-click to copy the current selection to clipboard (selection is then cleared).
- Mouse wheel scrolls scrollback when local mouse handling is active.
- If the remote app enables mouse reporting (for example full-screen TUIs), mouse events go to the remote app.
- In remote mouse mode, hold `Shift` (or `Alt`) while dragging to force local text selection.

## SSH Config Metadata for Better TUI Results

You can add metadata comments inside `~/.ssh/config` host blocks:

| Tag | Effect in TUI |
| --- | --- |
| `#_Desc <text>` | Description shown in host info pane |
| `#_Profile <name>` | Uses profile `<name>.cossh-config.yaml` when opening that host |
| `#_sshpass <true\|yes\|1>` | Runs that host via `sshpass -e` |
| `#_hidden <true\|yes\|1>` | Hides host from interactive host list |

Example:

```sshconfig
Host prod-fw
  #_Desc Production firewall
  #_Profile network
  HostName 10.0.0.10
  User admin
  Port 22
```

Additional behavior:

- Host aliases containing `*` or `?` are not shown in the interactive host list.
- Standard OpenSSH `Include` directives are supported and shown as folder nodes.

## TUI-Related Config Options

In your active `cossh` config file (`cossh-config.yaml` or `<profile>.cossh-config.yaml`), these keys affect the TUI:

```yaml
interactive_settings:
  history_buffer: 1000
  host_tree_uncollapsed: false
  info_view: true
  host_view_size: 25
  info_view_size: 40
```

What they do:

- `interactive_settings.history_buffer`: terminal scrollback lines per tab.
- `interactive_settings.host_tree_uncollapsed`: start folder tree expanded if `true`.
- `interactive_settings.info_view`: show host info pane by default.
- `interactive_settings.host_view_size`: initial host panel width percentage.
- `interactive_settings.info_view_size`: initial host info height percentage.

Validation and limits:

- `host_view_size` is clamped to `10..70`.
- `info_view_size` is clamped to `10..80`.
- Runtime panel width is also bounded by terminal size.
