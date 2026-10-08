# Records demo/demo.gif for the README: `make demo` (requires vhs).

Output demo/demo.gif

Set Shell "bash"
Set FontSize 18
Set Width 1200
Set Height 640
Set Padding 20
Set Theme "Catppuccin Mocha"
Set TypingSpeed 45ms

Hide
Type "source demo/setup.sh" Enter
Sleep 500ms
Show

# Quick capture
Type "jotty -- Retry logic in the sync worker swallows timeouts" Sleep 400ms Enter
Sleep 1s

# Piped input
Type "echo 'Ask ops how big the staging DB is' | jotty" Sleep 400ms Enter
Sleep 1s

# List and render
Type "jotty ls" Sleep 400ms Enter
Sleep 2s
Type "jotty show 1" Sleep 400ms Enter
Sleep 2s

# Browser
Type "jotty" Sleep 400ms Enter
Sleep 2s
Down Sleep 700ms
Down Sleep 2s
Down Sleep 700ms
Down Sleep 2.5s

# Filter
Type "/" Sleep 400ms
Type "cookie" Sleep 1.5s
Escape Sleep 1s

# New jot in the editor
Type "n" Sleep 800ms
Type "## Talk to design" Enter Enter
Type "about the **empty state** for the inbox" Sleep 1.5s
Escape Sleep 3s

Type "q" Sleep 1s
