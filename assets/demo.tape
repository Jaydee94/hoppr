# VHS tape file — regenerate animated demo.gif locally.
#
#   1. install VHS:  https://github.com/charmbracelet/vhs
#   2. install hoppr: `cargo install --path .`
#   3. seed config:   `hoppr config init`  (or point HOPPR_CONFIG at a demo file)
#   4. run:           `vhs assets/demo.tape`
#
# Output is written to assets/demo.gif.
#
# The tour mirrors assets/demo.svg: browse → search → settings → connect.

Output assets/demo.gif

Set FontSize 14
Set Width 1200
Set Height 720
Set Theme "Catppuccin Mocha"
Set Padding 24
Set Shell zsh
Set TypingSpeed 60ms
Set PlaybackSpeed 1.0

# ── 1. Launch ──────────────────────────────────────────────────────────────
Hide
Type "clear" Enter
Show
Sleep 600ms
Type "hoppr" Sleep 300ms Enter
Sleep 2s

# ── 2. Browse — show that Tab moves focus between Categories and Hosts ─────
Tab Sleep 700ms
Down Sleep 400ms
Down Sleep 400ms
Up   Sleep 400ms
Tab  Sleep 700ms

# ── 3. Search — type a query, watch hosts narrow, pick one ─────────────────
Type "/" Sleep 500ms
Type "prod" Sleep 1.2s
Enter Sleep 800ms
Down Sleep 350ms
Down Sleep 350ms
Up   Sleep 500ms

# ── 4. Settings — open the in-TUI editor and tour the menu items ───────────
Type "e" Sleep 1s
Down Sleep 500ms
Down Sleep 500ms
Down Sleep 500ms
Up   Sleep 400ms
Up   Sleep 400ms
Up   Sleep 400ms
Sleep 600ms
Escape Sleep 800ms

# ── 5. Connect — hand off to ssh on the highlighted host ───────────────────
Sleep 500ms
Enter
Sleep 2s
