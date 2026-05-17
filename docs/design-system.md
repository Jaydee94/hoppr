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
