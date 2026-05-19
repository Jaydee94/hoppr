# Keybindings

## Browse mode (default)

| key                  | action                                                      |
| -------------------- | ----------------------------------------------------------- |
| `Tab`                | Toggle focus between Categories ↔ Hosts                     |
| `/`                  | Focus the search input (always starts a fresh query)        |
| `Esc`                | When in search: clear focus back to Hosts. Else: quit.      |
| `q`                  | Quit                                                        |
| `Ctrl+C`             | Quit (from any mode)                                        |
| `↑` / `k`            | Previous item                                               |
| `↓` / `j`            | Next item                                                   |
| `Enter`              | When in Hosts: connect. When in Search: confirm and return. |
| `t`                  | Open connection in a new terminal tab (if detected).        |
| `f`                  | Toggle star / favorite on the selected host.                |
| `Ctrl+A`             | Search all (toggle global vs. current category)             |
| `e`                  | Open the in-TUI settings menu                               |
| `Backspace`          | When in Search: delete a character                          |
| `Ctrl+U`             | When in Search: clear the query (stays in search focus)     |

### Virtual categories

Two virtual categories appear at the top of the category list when they have content:

- **🕒 Recent** — last 10 connections you made (persisted across sessions).
- **★ Starred** — hosts you marked with `f`. Select a host and press `f` again to unstar.

Both virtual categories support the normal search query and vim navigation.

## Edit mode (settings)

Open with `e`. Esc returns to the previous screen.

If the editor has unsaved changes when you press `Esc` on the settings menu, hoppr shows an **unsaved changes prompt** instead of silently writing the config. Choose `[s]` to save and exit, `[d]` to discard your edits (reloads the file from disk), or `[c]` (or `Esc`) to cancel and stay in the editor.

| key                  | action                                                      |
| -------------------- | ----------------------------------------------------------- |
| `↑↓ j k`             | Move within a list / between form fields                    |
| `Tab` / `Shift+Tab`  | Next / previous form field (in form views). In the Hosts view, cycles to the next / previous **category** — not the next panel. Use `↑↓` or `j`/`k` to move within the host list. |
| `Enter`              | Open the selected item · submit the current form            |
| `a`                  | Add a new category / host                                   |
| `r`                  | Rename / edit the selected item                             |
<<<<<<< HEAD
| `d`                  | Delete the selected item (no undo)                          |
| `Ctrl+S`             | Save the config to disk (universal across all editor views) |
=======
| `d`                  | Request deletion of the selected item (prompts `y/n`)       |
| `y` / `n`            | Confirm / cancel a pending delete prompt                    |
| `s`                  | Save the config to disk (`Ctrl+S` inside form views)        |
>>>>>>> 8d0e954 (feat(editor): confirm destructive deletes in editor)
| `Esc`                | Back to the previous screen                                 |

### Settings menu

```
▍ Manage categories
  Manage hosts
  Connection defaults
  Central repo sync
```

- **Manage categories** — list, add (`a`), rename (`r` / `Enter`), delete (`d` then confirm with `y`).
- **Manage hosts** — `Tab` / `Shift+Tab` cycle to the next / previous category. Use `↑↓` (or `j`/`k`) to move within the host list. Same `a/r/d` semantics; `d` opens a `y/n` confirmation modal so a stray keypress can't wipe an entry.
- **Connection defaults** — edit `defaults.command` (program), default port, default user.
- **Central repo sync** — edit the `sync.*` block. Tab/↑↓ moves through the six form fields and the three action buttons underneath; `Space` toggles `auto-pull` / `auto-push`; `Enter` either applies the form (text fields), flips the toggle (booleans), or fires the button. `Ctrl+S` is a universal save shortcut.

### Sync editor controls

| element / key            | action                                                                 |
| ------------------------ | ---------------------------------------------------------------------- |
| `↑↓ Tab`                 | Move between fields and the three action buttons                       |
| `Space`                  | Toggle `auto-pull` / `auto-push` on the focused boolean row            |
| `Enter` on a text field  | Apply the form (auto-clones the repo if missing locally)               |
| `Enter` on a toggle      | Flip the boolean                                                       |
| `[Test]`                 | Read-only probe — `ls-remote` against the URL currently in the form    |
| `[Pull now]`             | Apply the form, then clone or fast-forward pull (no local write)       |
| `[Save & push]`          | Apply the form, write the local config to disk, push if `auto_push`    |
| `Ctrl+S`                 | Same as the `[Save & push]` button — universal save shortcut           |
| `Esc`                    | Back to the settings menu                                              |

## Mnemonics

- `e` for **edit** — opens settings.
- `Ctrl+S` for **save** — flush to disk (works in every editor view).
- `r` for **rename / replace** — also acts as "edit selected".
- `/` from many TUIs you've used before.
