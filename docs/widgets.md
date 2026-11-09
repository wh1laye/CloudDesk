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

- kind: clock, text, battery, memory, cpu, date, uptime, disk
- permissions: battery, memory, cpu and disk need 'system', otherwise the widget is skipped
- text kind shows the text key, ascii only, 48 characters at most
- limits: 16 widgets, manifest up to 4096 bytes, width 120..600, height 100..400
- folders and manifests that are links are ignored
- cpu and memory repaint every 2 seconds, battery every 10, clock every minute, text never

# visualizer modes

- the mode is chosen in settings, components tab, and saved in config.toml under [visualizer] as mode = contour or dots; clicking the visualizer on the desktop switches it only until the next reload

# themes

- new themes: glass (dark acrylic), frost (light acrylic), ocean, rose, mint, violet (solid)
- glass and frost use the windows 11 acrylic backdrop (build 22621 or newer) through dwm; on older builds or when dwm refuses, windows stay black
- the backdrop is applied when a window is created: after switching to or from glass, restart the shell and the widget host (turn the component off and on)
- the lock screen, layered windows and the dock stay opaque

- widgets live in the desktop layer behind the icons; negative x anchors the widget to the right screen edge
- the default memory and cpu widgets are placed on the right side
