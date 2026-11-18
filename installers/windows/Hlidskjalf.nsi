Unicode true
Name "Hliðskjálf"
Caption "Instalador do Hliðskjálf"
OutFile "../../dist/windows-native/Hlidskjalf-Setup-Rust-x64.exe"
InstallDir "$LOCALAPPDATA\Programs\Hlidskjalf"
RequestExecutionLevel user
SetCompressor /SOLID lzma
Icon "..\..\crates\hlidskjalf-desktop\assets\hlidskjalf.ico"
UninstallIcon "..\..\crates\hlidskjalf-desktop\assets\hlidskjalf.ico"
VIProductVersion "0.1.0.0"
VIAddVersionKey "ProductName" "Hliðskjálf"
VIAddVersionKey "FileDescription" "Hliðskjálf desktop app"
VIAddVersionKey "ProductVersion" "0.1.0"
VIAddVersionKey "FileVersion" "0.1.0"
VIAddVersionKey "LegalCopyright" "Hliðskjálf contributors"

Page directory
Page instfiles
UninstPage uninstConfirm
UninstPage instfiles

Section "Hliðskjálf" InstallSection
  SetShellVarContext current
  SetRegView 64
  ; Remove the previous "Vortal Rust" install; its data folder is migrated by the app.
  IfFileExists "$LOCALAPPDATA\Programs\VortalRust\Uninstall.exe" 0 legacy_done
  ReadRegDWORD $1 HKCU "Software\VortalRust" "StartupDisabled"
  StrCmp $1 "1" 0 +2
  WriteRegDWORD HKCU "Software\Hlidskjalf" "StartupDisabled" 1
  ExecWait '"$LOCALAPPDATA\Programs\VortalRust\Uninstall.exe" /S _?=$LOCALAPPDATA\Programs\VortalRust'
  Delete "$LOCALAPPDATA\Programs\VortalRust\Uninstall.exe"
  RMDir "$LOCALAPPDATA\Programs\VortalRust"
legacy_done:
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "VortalRust"
  SetOutPath "$INSTDIR"
  File "..\..\target\x86_64-pc-windows-msvc\release\hlidskjalf-desktop.exe"
  File "..\..\target\x86_64-pc-windows-msvc\release\hlidskjalf.exe"
  File "..\..\target\x86_64-pc-windows-msvc\release\hlidskjalfd.exe"
  File "..\..\target\x86_64-pc-windows-msvc\release\hlidskjalf-motion.exe"
  File "..\..\crates\hlidskjalf-desktop\assets\hlidskjalf.ico"
  File "..\..\README.md"
  File /oname=README-WINDOWS.md "README.md"
  File "..\..\LICENSE-APACHE"
  File "..\..\LICENSE-MIT"
  CreateDirectory "$SMPROGRAMS\Hliðskjálf"
  CreateShortcut "$SMPROGRAMS\Hliðskjálf\Hliðskjálf.lnk" "$INSTDIR\hlidskjalf-desktop.exe" "" "$INSTDIR\hlidskjalf.ico"
  CreateShortcut "$SMPROGRAMS\Hliðskjálf\Desinstalar Hliðskjálf.lnk" "$INSTDIR\Uninstall.exe"
  CreateShortcut "$DESKTOP\Hliðskjálf.lnk" "$INSTDIR\hlidskjalf-desktop.exe" "" "$INSTDIR\hlidskjalf.ico"
  StrCpy $0 "0"
  ReadRegDWORD $0 HKCU "Software\Hlidskjalf" "StartupDisabled"
  StrCmp $0 "1" startup_done
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Hlidskjalf" '"$INSTDIR\hlidskjalf-desktop.exe" --background'
startup_done:
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Hlidskjalf" "DisplayName" "Hliðskjálf"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Hlidskjalf" "DisplayVersion" "0.1.0"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Hlidskjalf" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Hlidskjalf" "DisplayIcon" "$INSTDIR\hlidskjalf.ico"
SectionEnd

Section "Uninstall"
  SetRegView 64
  Delete "$SMPROGRAMS\Hliðskjálf\Hliðskjálf.lnk"
  Delete "$SMPROGRAMS\Hliðskjálf\Desinstalar Hliðskjálf.lnk"
  RMDir "$SMPROGRAMS\Hliðskjálf"
  Delete "$DESKTOP\Hliðskjálf.lnk"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Hlidskjalf"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Hlidskjalf"
  DeleteRegKey HKCU "Software\Hlidskjalf"
  Delete "$INSTDIR\hlidskjalf-desktop.exe"
  Delete "$INSTDIR\hlidskjalf.exe"
  Delete "$INSTDIR\hlidskjalfd.exe"
  Delete "$INSTDIR\hlidskjalf-motion.exe"
  Delete "$INSTDIR\hlidskjalf.ico"
  Delete "$INSTDIR\README.md"
  Delete "$INSTDIR\README-WINDOWS.md"
  Delete "$INSTDIR\LICENSE-APACHE"
  Delete "$INSTDIR\LICENSE-MIT"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
SectionEnd
