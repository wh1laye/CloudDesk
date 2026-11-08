# dock and search

- dock: floating pill with real app icons, icons grow under the cursor, right click removes an item, the last item cannot be removed
- search: win+d or alt+space; enter launches, ctrl+enter pins the selected app to the dock, esc closes
- dock items live in %APPDATA%\CloudDesk\config.toml under [dock] as items = 'NAME|path;NAME|path'
- icons: exe files are read at 128 px, everything else uses the system large icon; items without an icon show their first letter
- win+d is intercepted with a low level keyboard hook only while the search component is on

# island

- floating pill under the top bar with the current track title, artist and previous, play or pause, next buttons
- data comes from the system media session (smtc), so it follows any player that registers there; it appears only while a session exists
- a worker thread polls every 1.5 seconds and exists only while the island component is on
- the island component switch is ISLAND in settings

# visualizer

- dot-matrix equalizer widget at the bottom left of the desktop, behind all windows
- audio comes from the default output device through wasapi loopback; the thread exists only while the component is on
- the widget repaints only when the levels change, silence stops the repaints
- if the default output device changes, restart the shell to follow it

# lock screen

- ctrl+alt+l covers all monitors with a clock and a password field; component switch is LOCK
- the password is checked against the current windows account with LogonUser and wiped from memory after the check
- this is a session cover, not a security boundary: ctrl+alt+del and task manager still work, and it does not replace the windows sign-in screen
- if USERNAME is empty the lock opens on enter so the session can never be trapped
