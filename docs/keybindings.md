# Keybindings

## Browse mode (default)

| key                  | action                                                      |
| -------------------- | ----------------------------------------------------------- |
| `Tab`                | Toggle focus between Categories ↔ Hosts                     |
| `/`                  | Focus the search input                                      |
| `Esc`                | When in search: clear focus back to Hosts. Else: quit.      |
| `q`                  | Quit                                                        |
| `Ctrl+C`             | Quit (from any mode)                                        |
| `↑` / `k`            | Previous item                                               |
| `↓` / `j`            | Next item                                                   |
| `Enter`              | When in Hosts: connect. When in Search: confirm and return. |
| `t`                  | Open connection in a new terminal tab (if detected).        |
| `f`                  | Toggle star / favorite on the selected host.                |
| `Ctrl+A`             | Toggle global search (all categories vs. current only).     |
| `e`                  | Open the in-TUI settings menu                               |
| `Backspace`          | When in Search: delete a character                          |

### Virtual categories

Two virtual categories appear at the top of the category list when they have content:

- **🕒 Recent** — last 10 connections you made (persisted across sessions).
- **★ Starred** — hosts you marked with `f`. Select a host and press `f` again to unstar.

Both virtual categories support the normal search query and vim navigation.

## Edit mode (settings)

Open with `e`. Esc returns to the previous screen.

| key                  | action                                                      |
| -------------------- | ----------------------------------------------------------- |
| `↑↓ j k`             | Move within a list / between form fields                    |
| `Tab` / `Shift+Tab`  | Next / previous form field                                  |
| `Enter`              | Open the selected item · submit the current form            |
| `a`                  | Add a new category / host                                   |
| `r`                  | Rename / edit the selected item                             |
| `d`                  | Delete the selected item (no undo)                          |
| `s`                  | Save the config to disk (`Ctrl+S` inside form views)        |
| `Esc`                | Back to the previous screen                                 |

### Settings menu

```
▍ Manage categories
  Manage hosts
  Connection defaults
  Central repo sync
```

- **Manage categories** — list, add (`a`), rename (`r` / `Enter`), delete (`d`).
- **Manage hosts** — `Tab` cycles between categories. Same `a/r/d` semantics.
- **Connection defaults** — edit `defaults.command` (program), default port, default user.
- **Central repo sync** — edit the `sync.*` block. `Enter` applies the form (and clones the repo on first setup); `Space` toggles the boolean field under the cursor; `Ctrl+T` runs a connection test against the entered URL; `Ctrl+P` triggers an immediate sync (clone or fast-forward pull); `Ctrl+S` writes to disk and (if `auto_push: true`) pushes upstream.

### Sync editor keys

| key       | action                                                       |
| --------- | ------------------------------------------------------------ |
| `↑↓ Tab`  | Move between sync fields                                     |
| `Space`   | Toggle `auto-pull` / `auto-push` on the focused boolean row  |
| `Enter`   | Apply the form (auto-clones the repo if missing locally)     |
| `Ctrl+T`  | Test the connection to the repo URL (no disk writes)         |
| `Ctrl+P`  | Sync now — clone or fast-forward pull                        |
| `Ctrl+S`  | Save the local config to disk                                |
| `Esc`     | Back to the settings menu                                    |

## Mnemonics

- `e` for **edit** — opens settings.
- `s` for **save** — flush to disk.
- `r` for **rename / replace** — also acts as "edit selected".
- `/` from many TUIs you've used before.
