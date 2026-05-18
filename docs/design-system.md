# Design system

All UI styling for `hoppr` flows through `src/theme.rs`. New colors, modifiers and layout primitives must be added there — **no inline hex codes anywhere else**.

## Palette — "midnight"

| token           | hex       | use                                                       |
| --------------- | --------- | --------------------------------------------------------- |
| `bg`            | `#0a0b10` | Canvas. Near-black with a tiny indigo bias.               |
| `surface`       | `#12131a` | Popovers, modal background.                               |
| `surface_alt`   | `#1a1c25` | Selected row tint.                                        |
| `border`        | `#252834` | Default panel border.                                     |
| `border_strong` | `#3a3e4e` | Hover / form-field border.                                |
| `text`          | `#f1f3f9` | Primary text.                                             |
| `text_dim`      | `#8a92a6` | Secondary text — inactive labels.                         |
| `text_muted`    | `#555c70` | Tertiary text — meta info, hints.                         |
| `primary`       | `#7c5cff` | Brand primary. Active-panel border, accents.              |
| `primary_glow`  | `#a78aff` | Active titles, highlighted list items.                    |
| `accent`        | `#00e5ff` | Hover state for hosts; high-energy moments (caret, search).|
| `success`       | `#00d68f` | Sync OK, save confirmation.                               |
| `warning`       | `#ffb547` | Dirty buffer, unsaved changes.                            |
| `error`         | `#ff5470` | Sync failed, validation errors.                           |

## Style helpers

| method                       | semantic                                       |
| ---------------------------- | ---------------------------------------------- |
| `Theme::base()`              | Default canvas style (`text` on `bg`).         |
| `Theme::surface_style()`     | Modal / popover surface.                       |
| `Theme::muted()`             | Secondary text on canvas.                      |
| `Theme::dim()`               | Tertiary text on canvas.                       |
| `Theme::border_style(active)`| Primary when `active`, neutral otherwise.      |
| `Theme::highlight_primary()` | Selected-row tint, brand-accented.             |
| `Theme::highlight_accent()`  | Selected-row tint, accent-colored.             |

## Layout primitives

- Borders: `BorderType::Rounded` for inactive panels, `BorderType::Thick` for the focused panel — focus is always unambiguous.
- Active section glyph: `▍ ` (`theme::ACTIVE_GLYPH`).
- Hints in the footer use `[key] label` pairs separated by `  ·  ` dots.

## Status bar

The 1-row status bar above the hint line has three slots:

| slot                    | shows                                                                                  |
| ----------------------- | -------------------------------------------------------------------------------------- |
| left (32 chars)         | Sync chip: `●` colored by state + label. When the repo has been pulled at least once in the session the label becomes `synced 2m ago`, refreshed every 200 ms. A bold `· unpushed` suffix appears when `sync::has_uncommitted_changes` is true. |
| middle (flex)           | Optional `filter: "q"` chip (or `global: "q"` with `Ctrl+A`), then either a transient status message colored by `MessageKind` (✓ success / · info / ⚠ warn / ✕ error) or, when no message is active, the resolved connection command of the selected host (`[name] ssh user@ip:22`). |
| right (20 chars)        | `<filtered>/<total> hosts`.                                                            |

`MessageKind` controls both color and TTL: `Success`/`Info` fade after 3 s, `Warn` after 6 s, `Error` after 10 s. Once the TTL expires the middle slot reverts to the selected-host preview. Always pick the right severity at the call site — use `App::set_status_success` / `set_status_warn` / `set_status_error` rather than plain `set_status` (which defaults to info).

## Brand assets

| file                  | purpose                                            |
| --------------------- | -------------------------------------------------- |
| `assets/logo.svg`     | Square logo (256×256). Rounded dark tile + hop arc.|
| `assets/icon.svg`     | 64×64 favicon-grade variant.                       |
| `assets/banner.svg`   | 1280×320 README banner with wordmark + tagline.    |
| `assets/demo.svg`     | Static TUI screenshot mockup for documentation.    |
| `assets/demo.tape`    | VHS tape — regenerate `assets/demo.gif` locally.   |

## Adding a new color

1. Add the token to `Theme` in `src/theme.rs`.
2. Set the value in `Theme::midnight()`.
3. Add a row to the palette table above.
4. Reference it via `theme.<token>` in your renderer.

Never accept a PR with a raw `Color::Rgb(...)` outside `theme.rs`.
