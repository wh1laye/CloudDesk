# installer

usage: clouddesk-installer [install|update|uninstall] [--silent] [--dir path] [--payload path] [--autostart] [--purge] [--skip-checks] [--log path]

- default dir: %LOCALAPPDATA%\Programs\CloudDesk, no admin rights
- writes only hkcu: uninstall entry, optional run entry
- payload: files from the payload folder next to the installer
- update moves the old install to a .bak folder and restores it on failure
- uninstall.exe is copied into the install dir; it relaunches from temp to delete its own folder
- uninstall refuses any folder without the .clouddesk-install marker
- log: %TEMP%\clouddesk-installer.log
