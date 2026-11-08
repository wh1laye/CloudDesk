# widgets

- host: separate process clouddesk-widgets.exe, started by the shell while the WIDGETS component is on, stopped when it is off
- the host exits by itself when the shell process ends
- widgets are folders in %APPDATA%\CloudDesk\widgets\NAME with a widget.toml; first start creates memory and cpu examples
- the shell must be restarted to pick up new or changed manifests; theme changes are picked up within 3 seconds
- no scripts and no code from widget folders: a manifest only selects a built-in kind and its position

[widget]
name = 'MEMORY'
kind = 'memory'
permissions = 'system'
x = 24
y = 60
width = 220
height = 130

- kind: clock, text, battery, memory, cpu
- permissions: battery, memory and cpu need 'system', otherwise the widget is skipped
- text kind shows the text key, ascii only, 48 characters at most
- limits: 16 widgets, manifest up to 4096 bytes, width 120..600, height 100..400
- folders and manifests that are links are ignored
- cpu and memory repaint every 2 seconds, battery every 10, clock every minute, text never

# visualizer modes

- click the visualizer on the desktop to switch between the dot equalizer and the contour ring; the mode is not saved between runs
