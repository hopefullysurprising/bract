# With bract: narrow to the tool, step into the command, fill it, then run it.
# Render: `mise run demo` (needs vhs + ttyd + ffmpeg, and a kubectl-active dir).
Output docs/demo-bract.gif

Set Shell "bash"
Set FontSize 18
Set Width 1100
Set Height 660
Set Padding 18
Set TypingSpeed 110ms

Hide
Type `cd tmp/demo && eval "$(mise env)" && export PATH="$OLDPWD/target/release:$PATH" && export BRACT_NO_MEMORY=1 && clear`
Enter
Sleep 1s
Show

Type "bract"
Enter
Sleep 3.5s

# A few letters narrow the toolchain to kubectl — no need to type it out.
Type "/kube"
Sleep 1.8s
Enter
Sleep 3s

# `create` is already under the cursor; just step into it.
Enter
Sleep 2.5s

# `depl` lands straight on `deployment`; open its form.
Type "/depl"
Sleep 1.8s
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
