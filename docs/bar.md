# top bar and spaces

- the top bar is one full-width strip with a hairline at the bottom; items sit in three groups (left, center, right) and the strip reserves the top of the work area
- icons are vector shapes drawn with anti-aliasing, the item under the cursor gets a soft highlight, the clock shows the date dimmed and the time bold
- what the bar shows can be changed in settings, Top bar tab; an item lives in one group only
- items are chosen in config.toml under [bar] as comma lists: left = 'settings,search,spaces', center = 'clock', right = 'wall,battery'
- items: settings, search, spaces, clock, time, battery, wall, name; at most 6 per group, unknown names are ignored, an empty list hides the group
- click: settings opens the settings app, search opens search, a dot in spaces switches to that desktop, wall steps the wallpaper
- the battery item draws a level glyph that turns red at 20 percent or less and green while charging; on a desktop pc it shows AC
- the bar repaints every minute and after a click; a desktop switch made with other keys shows up on the next repaint

# spaces

- off by default; turn it on in settings, components tab, SPACES, or set [spaces] enabled = true
- win+1 to win+9 switch to virtual desktop 1 to 9; missing desktops are created first
- switching sends the documented win+ctrl+left, win+ctrl+right and win+ctrl+d shortcuts; no undocumented com interfaces are used
- the current desktop is read from the registry (explorer VirtualDesktops values, undocumented, read only); if it cannot be read, nothing is sent
- win+1 to win+9 stop launching taskbar items while spaces is on; win+ctrl+digit, win+alt+digit and win+shift+digit are left alone
- the keyboard hook exists only while spaces or search is on

# wallpapers

- five generated presets in blue and dark tones (azure, petal, dusk, night, aurora) are written to %APPDATA%\CloudDesk\wallpapers on first start; they are original artwork made by the program, not Microsoft images
- the pictures that ship with Windows are listed after the presets, read from %SystemRoot%\Web\Wallpaper at run time (at most 24); they are never copied into the repository or the installer
- settings, Wallpaper tab, shows a gallery with thumbnails; click a picture to apply it, Restore original returns the wallpaper that was set before CloudDesk changed it
- presets from older versions (dots, waves, rings) are left in the folder if they are already there
