# Without bract: page through --help after --help, guess, hit a required-flag
# error, then retype the full command.
# Render: `mise run demo` (needs vhs + ttyd + ffmpeg, and kubectl on PATH).
Output docs/demo-bare.gif

Set Shell "bash"
Set FontSize 18
Set Width 1100
Set Height 660
Set Padding 18
Set TypingSpeed 110ms

Hide
Type `cd tmp/demo && eval "$(mise env)" && clear`
Enter
Sleep 1s
Show

Type "kubectl --help | less"
Enter
Sleep 2s
Space
Sleep 1.8s
Space
Sleep 1.8s
Type "q"
Sleep 1s

Type "kubectl create --help | less"
Enter
Sleep 2s
Space
Sleep 1.8s
Type "q"
Sleep 1s

Type "kubectl create deployment --help | less"
Enter
Sleep 2s
Space
Sleep 1.8s
Down
Down
Down
Sleep 1.8s
Type "q"
Sleep 1.2s

Type "kubectl create deployment web"
Enter
Sleep 2.5s

Type "kubectl create deployment web --image=nginx --dry-run=client -o yaml"
Enter
Sleep 4s
