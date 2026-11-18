Unicode true
Name "Hliðskjálf"
Caption "Instalador do Hliðskjálf"
OutFile "/tmp/hlidskjalf-dist/windows-electron/Hlidskjalf-Setup-x64.exe"
InstallDir "$LOCALAPPDATA\Programs\Hlidskjalf"
RequestExecutionLevel user
SetCompressor /SOLID lzma
SetCompressorDictSize 32
Icon "..\..\crates\hlidskjalf-desktop\assets\hlidskjalf.ico"
UninstallIcon "..\..\crates\hlidskjalf-desktop\assets\hlidskjalf.ico"
VIProductVersion "0.1.0.0"
VIAddVersionKey "ProductName" "Hliðskjálf"
VIAddVersionKey "FileDescription" "Hliðskjálf Electron desktop app"
VIAddVersionKey "ProductVersion" "0.1.0"

Page directory
Page instfiles
UninstPage uninstConfirm
UninstPage instfiles

Section "Hliðskjálf"
  SetShellVarContext current
  SetOutPath "$INSTDIR"
  File /r "..\..\dist\windows-electron\win-unpacked\*.*"
  CreateDirectory "$SMPROGRAMS\Hliðskjálf"
  CreateShortcut "$SMPROGRAMS\Hliðskjálf\Hlidskjalf.lnk" "$INSTDIR\Hlidskjalf.exe" "" "$INSTDIR\Hlidskjalf.exe"
  CreateShortcut "$SMPROGRAMS\Hliðskjálf\Desinstalar Hlidskjalf.lnk" "$INSTDIR\Uninstall.exe"
  CreateShortcut "$DESKTOP\Hlidskjalf.lnk" "$INSTDIR\Hlidskjalf.exe" "" "$INSTDIR\Hlidskjalf.exe"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Hlidskjalf" "DisplayName" "Hliðskjálf"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Hlidskjalf" "DisplayVersion" "0.1.0"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Hlidskjalf" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Hlidskjalf" "DisplayIcon" "$INSTDIR\Hlidskjalf.exe"
SectionEnd

Section "Uninstall"
  SetShellVarContext current
  Delete "$SMPROGRAMS\Hliðskjálf\Hlidskjalf.lnk"
  Delete "$SMPROGRAMS\Hliðskjálf\Desinstalar Hlidskjalf.lnk"
  RMDir "$SMPROGRAMS\Hliðskjálf"
  Delete "$DESKTOP\Hlidskjalf.lnk"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Hlidskjalf"
  RMDir /r "$INSTDIR"
SectionEnd
