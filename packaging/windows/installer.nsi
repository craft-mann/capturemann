; CaptureMann per-user installer (NSIS 3, Unicode). Built by build-windows.ps1:
;   makensis /DVERSION=0.2.9 /DVI_VERSION=0.2.9.0 /DSTAGE=<staged tree> /DOUTFILE=<setup.exe> installer.nsi
;
; Per user: no administrator rights, files in %LOCALAPPDATA%\Programs\CaptureMann, Start Menu shortcut,
; an entry under "Apps > Installed apps" (HKCU), an optional PATH entry for capturemann.exe and cm-mcp.exe.
; The uninstaller removes only what the installer put there: the library, captures and settings are
; never touched.
;
; Silent use:   setup.exe /S [/D=C:\dir] [/ADDPATH]          (/D must be last and unquoted)
;               "<dir>\uninstall.exe" /S _?=<dir>            (_?= makes it wait; delete the folder after)
;
; NOT RUN ON WINDOWS YET (written without makensis): see packaging/windows/README.md.

Unicode true
!ifndef VERSION
  !error "pass /DVERSION=x.y.z"
!endif
!ifndef VI_VERSION
  !define VI_VERSION "${VERSION}.0"
!endif
!ifndef STAGE
  !error "pass /DSTAGE=<staged tree>"
!endif
!ifndef OUTFILE
  !define OUTFILE "capturemann-setup.exe"
!endif

!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "LogicLib.nsh"

!define APPNAME "CaptureMann"
!define UNINSTKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APPNAME}"

Name "${APPNAME}"
OutFile "${OUTFILE}"
RequestExecutionLevel user
InstallDir "$LOCALAPPDATA\Programs\${APPNAME}"
InstallDirRegKey HKCU "Software\${APPNAME}" "InstallDir"
SetCompressor /SOLID lzma
ShowInstDetails nevershow

VIProductVersion "${VI_VERSION}"
VIAddVersionKey "ProductName" "${APPNAME}"
VIAddVersionKey "FileDescription" "${APPNAME} installer"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "LegalCopyright" "Copyright the CaptureMann contributors. Apache-2.0."

!define MUI_ICON "${STAGE}\app.ico"
!define MUI_UNICON "${STAGE}\app.ico"
!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN "$INSTDIR\cm-desktop.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Start ${APPNAME}"

!insertmacro MUI_PAGE_LICENSE "${STAGE}\doc\LICENSE"
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Var AddPathRequested

Function .onInit
  ; /ADDPATH selects the PATH section (it is off by default), for silent installs.
  StrCpy $AddPathRequested "0"
  ${GetParameters} $R0
  ClearErrors
  ${GetOptions} $R0 "/ADDPATH" $R1
  ${IfNot} ${Errors}
    StrCpy $AddPathRequested "1"
  ${EndIf}
FunctionEnd

Section "${APPNAME}" SecApp
  SectionIn RO
  SetOutPath "$INSTDIR"
  ; The whole staged tree, flat: ffmpeg.exe must sit next to the programs (cm-media looks there first); the
  ; subfolders (FFmpeg, Llama, doc, Resources\Models\speaker) come with it and RMDir /r removes them again.
  File /r "${STAGE}\*.*"
  WriteUninstaller "$INSTDIR\uninstall.exe"
  CreateDirectory "$SMPROGRAMS\${APPNAME}"
  CreateShortCut "$SMPROGRAMS\${APPNAME}\${APPNAME}.lnk" "$INSTDIR\cm-desktop.exe" "" "$INSTDIR\app.ico" 0
  CreateShortCut "$SMPROGRAMS\${APPNAME}\Uninstall ${APPNAME}.lnk" "$INSTDIR\uninstall.exe"
  WriteRegStr HKCU "Software\${APPNAME}" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTKEY}" "DisplayName" "${APPNAME}"
  WriteRegStr HKCU "${UNINSTKEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINSTKEY}" "Publisher" "CaptureMann contributors"
  WriteRegStr HKCU "${UNINSTKEY}" "DisplayIcon" "$INSTDIR\app.ico"
  WriteRegStr HKCU "${UNINSTKEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTKEY}" "URLInfoAbout" "https://github.com/craft-mann/capturemann"
  WriteRegStr HKCU "${UNINSTKEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKCU "${UNINSTKEY}" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
  WriteRegDWORD HKCU "${UNINSTKEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTKEY}" "NoRepair" 1
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  IntFmt $0 "0x%08X" $0
  WriteRegDWORD HKCU "${UNINSTKEY}" "EstimatedSize" "$0"
SectionEnd

Section "Offer CaptureMann in the Open with menu of video and audio files" SecOpenWith
  ; Per user (HKCU), never the default program: the file types stay with whatever opens them now.
  ; The extensions are the ones the import reads (DOCUMENT_EXT in apps/desktop/src/import_flow.rs).
  ; Open with > CaptureMann starts `cm-desktop.exe "<file>"`; a running CaptureMann gets the file.
  WriteRegStr HKCU "Software\Classes\Applications\cm-desktop.exe" "FriendlyAppName" "${APPNAME}"
  WriteRegStr HKCU "Software\Classes\Applications\cm-desktop.exe\shell\open" "FriendlyAppName" "${APPNAME}"
  WriteRegStr HKCU "Software\Classes\Applications\cm-desktop.exe\shell\open\command" "" '"$INSTDIR\cm-desktop.exe" "%1"'
  WriteRegStr HKCU "Software\Classes\Applications\cm-desktop.exe\SupportedTypes" ".mp4" ""
  WriteRegStr HKCU "Software\Classes\.mp4\OpenWithList\cm-desktop.exe" "" ""
  WriteRegStr HKCU "Software\Classes\Applications\cm-desktop.exe\SupportedTypes" ".mov" ""
  WriteRegStr HKCU "Software\Classes\.mov\OpenWithList\cm-desktop.exe" "" ""
  WriteRegStr HKCU "Software\Classes\Applications\cm-desktop.exe\SupportedTypes" ".m4v" ""
  WriteRegStr HKCU "Software\Classes\.m4v\OpenWithList\cm-desktop.exe" "" ""
  WriteRegStr HKCU "Software\Classes\Applications\cm-desktop.exe\SupportedTypes" ".mkv" ""
  WriteRegStr HKCU "Software\Classes\.mkv\OpenWithList\cm-desktop.exe" "" ""
  WriteRegStr HKCU "Software\Classes\Applications\cm-desktop.exe\SupportedTypes" ".m4a" ""
  WriteRegStr HKCU "Software\Classes\.m4a\OpenWithList\cm-desktop.exe" "" ""
  WriteRegStr HKCU "Software\Classes\Applications\cm-desktop.exe\SupportedTypes" ".wav" ""
  WriteRegStr HKCU "Software\Classes\.wav\OpenWithList\cm-desktop.exe" "" ""
  ; Tell Explorer the associations changed (SHCNE_ASSOCCHANGED).
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
  WriteRegStr HKCU "Software\${APPNAME}" "OpenWith" "1"
SectionEnd

Section /o "Add capturemann.exe and cm-mcp.exe to my PATH" SecPath
  ; Edits the per-user PATH only (HKCU), through path.ps1, then tells running programs.
  nsExec::ExecToLog '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$INSTDIR\path.ps1" -Action add -Dir "$INSTDIR"'
  Pop $0
  WriteRegStr HKCU "Software\${APPNAME}" "PathAdded" "1"
  SendMessage ${HWND_BROADCAST} ${WM_WININICHANGE} 0 "STR:Environment" /TIMEOUT=5000
SectionEnd


Section "-pathflag"
  ; /ADDPATH on a silent install: run the same step even though the section is not ticked.
  ${If} $AddPathRequested == "1"
    ${IfNot} ${SectionIsSelected} ${SecPath}
      nsExec::ExecToLog '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$INSTDIR\path.ps1" -Action add -Dir "$INSTDIR"'
      Pop $0
      WriteRegStr HKCU "Software\${APPNAME}" "PathAdded" "1"
      SendMessage ${HWND_BROADCAST} ${WM_WININICHANGE} 0 "STR:Environment" /TIMEOUT=5000
    ${EndIf}
  ${EndIf}
SectionEnd

LangString DESC_SecApp ${LANG_ENGLISH} "CaptureMann, the command line, the MCP server, the speaker-label models and the licence files."
LangString DESC_SecOpenWith ${LANG_ENGLISH} "Lists CaptureMann under Open with for MP4, MOV, M4V, MKV, M4A and WAV files so you can import them. It does not become the default program."
LangString DESC_SecPath ${LANG_ENGLISH} "So that you can type capturemann and cm-mcp in a new terminal. Removed again by the uninstaller."
!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${SecApp} $(DESC_SecApp)
  !insertmacro MUI_DESCRIPTION_TEXT ${SecOpenWith} $(DESC_SecOpenWith)
  !insertmacro MUI_DESCRIPTION_TEXT ${SecPath} $(DESC_SecPath)
!insertmacro MUI_FUNCTION_DESCRIPTION_END

Section "Uninstall"
  ; Never RMDir /r a folder that is not ours.
  IfFileExists "$INSTDIR\cm-desktop.exe" +2 0
    Abort "$INSTDIR does not look like a CaptureMann folder; nothing was removed."
  ReadRegStr $0 HKCU "Software\${APPNAME}" "PathAdded"
  ${If} $0 == "1"
    nsExec::ExecToLog '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$INSTDIR\path.ps1" -Action remove -Dir "$INSTDIR"'
    Pop $0
    SendMessage ${HWND_BROADCAST} ${WM_WININICHANGE} 0 "STR:Environment" /TIMEOUT=5000
  ${EndIf}
  ReadRegStr $0 HKCU "Software\${APPNAME}" "OpenWith"
  ${If} $0 == "1"
    DeleteRegKey HKCU "Software\Classes\Applications\cm-desktop.exe"
    DeleteRegKey HKCU "Software\Classes\.mp4\OpenWithList\cm-desktop.exe"
    DeleteRegKey HKCU "Software\Classes\.mov\OpenWithList\cm-desktop.exe"
    DeleteRegKey HKCU "Software\Classes\.m4v\OpenWithList\cm-desktop.exe"
    DeleteRegKey HKCU "Software\Classes\.mkv\OpenWithList\cm-desktop.exe"
    DeleteRegKey HKCU "Software\Classes\.m4a\OpenWithList\cm-desktop.exe"
    DeleteRegKey HKCU "Software\Classes\.wav\OpenWithList\cm-desktop.exe"
    System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
  ${EndIf}
  Delete "$SMPROGRAMS\${APPNAME}\${APPNAME}.lnk"
  Delete "$SMPROGRAMS\${APPNAME}\Uninstall ${APPNAME}.lnk"
  RMDir "$SMPROGRAMS\${APPNAME}"
  DeleteRegKey HKCU "${UNINSTKEY}"
  DeleteRegKey HKCU "Software\${APPNAME}"
  ; The install folder holds only what the installer put there (plus files the app may have written next to
  ; itself, which it does not). The library, captures and settings live elsewhere and stay.
  RMDir /r "$INSTDIR"
SectionEnd
