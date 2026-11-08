# installer

usage: clouddesk-installer [install|update|uninstall] [--silent] [--dir path] [--payload path] [--autostart] [--purge] [--profile light|full] [--skip-checks] [--log path]

- default dir: %LOCALAPPDATA%\Programs\CloudDesk, no admin rights
- writes only hkcu: uninstall entry, optional run entry
- payload: files from the payload folder next to the installer
- update moves the old install to a .bak folder and restores it on failure
- uninstall.exe is copied into the install dir; it relaunches from temp to delete its own folder
- uninstall refuses any folder without the .clouddesk-install marker
- log: %TEMP%\clouddesk-installer.log

## profiles

- light (default): explorer stays, autostart is optional through hkcu run, nothing replaces the system shell
- full: sets the per-user shell value in hkcu winlogon to clouddesk-launcher.exe, requires launcher and host in the payload
- full saves the previous shell value to %APPDATA%\CloudDesk\shell-backup.txt before changing it and never overwrites that backup on update
- uninstall restores the shell first and aborts without deleting anything if the restore fails
- launcher restarts the host after a crash; three crashes within 20 seconds each restore the shell value and start explorer.exe
- launcher exits cleanly and starts explorer.exe when the host is closed normally
- recovery without installer or network: ctrl+alt+shift+e, restore.cmd in the install folder, or clouddesk-launcher.exe --recover
- full mode is verified only by the ci registry roundtrip; actual logon with a replaced shell must be checked in a virtual machine
