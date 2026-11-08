# top bar and spaces

- the top bar is up to three floating pills: left, center, right; the window is shaped to the pills and still reserves the top strip of the work area
- items are chosen in config.toml under [bar] as comma lists: left = 'settings,search,spaces', center = 'clock', right = 'wall,battery'
- items: settings, search, spaces, clock, time, battery, wall, name; at most 6 per pill, unknown names are ignored, an empty list hides the pill
- click: settings opens the settings app, search opens search, a dot in spaces switches to that desktop, wall steps the wallpaper
- the bar repaints every minute and after a click; a desktop switch made with other keys shows up on the next repaint

# spaces

- off by default; turn it on in settings, components tab, SPACES, or set [spaces] enabled = true
- win+1 to win+9 switch to virtual desktop 1 to 9; missing desktops are created first
- switching sends the documented win+ctrl+left, win+ctrl+right and win+ctrl+d shortcuts; no undocumented com interfaces are used
- the current desktop is read from the registry (explorer VirtualDesktops values, undocumented, read only); if it cannot be read, nothing is sent
- win+1 to win+9 stop launching taskbar items while spaces is on; win+ctrl+digit, win+alt+digit and win+shift+digit are left alone
- the keyboard hook exists only while spaces or search is on
