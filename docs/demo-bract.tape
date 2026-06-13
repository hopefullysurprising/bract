# With bract: search, navigate, and fill the same kubectl command — then run it.
# Render: `mise run demo` (needs vhs + ttyd + ffmpeg, and a kubectl-active dir).
Output docs/demo-bract.gif

Set Shell "bash"
Set FontSize 18
Set Width 1100
Set Height 660
Set Padding 18
Set TypingSpeed 110ms

Hide
Type `cd tmp/demo && eval "$(mise env)" && export PATH="$OLDPWD/target/release:$PATH" && clear`
Enter
Sleep 1s
Show

Type "bract"
Enter
Sleep 3.5s

Down
Sleep 1s
Down
Sleep 1.5s
Right
Sleep 3.5s

Type "/create"
Sleep 1.5s
Enter
Sleep 2.5s

Type "/deployment"
Sleep 1.5s
Enter
Sleep 3s

Type "web"
Sleep 1.5s

Tab
Sleep 500ms
Tab
Sleep 500ms
Tab
Sleep 500ms
Tab
Sleep 1s
Type "client"
Sleep 1.2s

Tab
Sleep 500ms
Tab
Sleep 1s
Type "nginx"
Sleep 1.2s

Tab
Sleep 1s
Type "yaml"
Sleep 1.5s

Ctrl+R
Sleep 4s
