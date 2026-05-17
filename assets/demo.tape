# VHS tape file — regenerate animated demo.gif locally.
#
#   1. install VHS:  https://github.com/charmbracelet/vhs
#   2. install hoppr: `cargo install --path .`
#   3. seed config:   `hoppr config init`  (or point HOPPR_CONFIG at a demo file)
#   4. run:           `vhs assets/demo.tape`
#
# Output is written to assets/demo.gif.

Output assets/demo.gif

Set FontSize 14
Set Width 1200
Set Height 700
Set Theme "Catppuccin Mocha"
Set Padding 24
Set Shell zsh

Type "hoppr" Sleep 400ms Enter

Sleep 1.2s
Type "/"
Sleep 600ms
Type "prod"
Sleep 1.2s
Enter
Sleep 800ms

Down Sleep 250ms Down Sleep 250ms Down Sleep 250ms
Up Sleep 250ms
Sleep 800ms

# Open settings
Type "e"
Sleep 1.2s
Down Sleep 300ms Down Sleep 300ms
Sleep 600ms
Escape

# Connect
Sleep 600ms
Enter

Sleep 1.5s
