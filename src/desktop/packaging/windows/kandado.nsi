; The Kandado installer for Windows, built by .github/workflows/release.yml:
;
;   makensis /DVERSION=0.1.0 /DSOURCE=<folder with the two programs and LICENSE> \
;            /DOUTFILE=<path>\kandado-0.1.0-setup.exe kandado.nsi
;
; Per user, no administrator rights: the app and its browser link (D-45) are
; per user too. Installs to %LOCALAPPDATA%\Programs\Kandado. Unsigned until
; there is a code-signing certificate (P10), so SmartScreen warns on first run.
;
; The uninstaller removes the programs, the Start menu shortcut, the sign-in
; start (D-43) and the browsers' native host keys. It keeps the vault in
; %APPDATA%\Kandado: removing it is "Sign out" in the app (D-52).
;
; In-app updates (D-54) run it as `setup.exe /S /RELAUNCH`: silent, waiting for
; Kandado to quit, then opening Kandado again.

Unicode true
SetCompressor /SOLID lzma
RequestExecutionLevel user
ManifestDPIAware true

!ifndef VERSION
  !error "Pass /DVERSION=x.y.z"
!endif
!ifndef SOURCE
  !error "Pass /DSOURCE=<folder>"
!endif
!ifndef OUTFILE
  !define OUTFILE "kandado-${VERSION}-setup.exe"
!endif

!define APP "Kandado"
!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\Kandado"
!define HOST_NAME "app.kandado.native"

Name "${APP}"
OutFile "${OUTFILE}"
InstallDir "$LOCALAPPDATA\Programs\Kandado"
InstallDirRegKey HKCU "${UNINSTALL_KEY}" "InstallLocation"
BrandingText "${APP} ${VERSION}"

VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "${APP}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "FileDescription" "${APP} installer"
VIAddVersionKey "LegalCopyright" "AGPL-3.0-or-later"

!include "MUI2.nsh"
!include "FileFunc.nsh"

Var Relaunch

!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN "$INSTDIR\kandado.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Open Kandado"
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "${SOURCE}\LICENSE.txt"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!define MUI_UNCONFIRMPAGE_TEXT_TOP "Kandado will be removed from this computer. Your vault stays in %APPDATA%\Kandado; to remove it too, choose Sign out in Kandado first."
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

; A program that is running cannot be replaced. Opening it for writing fails
; while it runs. Wait a little first (an update starts while Kandado quits),
; then ask until it is closed (or the person cancels).
!macro WaitUntilClosed FILE LABEL
  !define UniqueID ${__LINE__}
  StrCpy $1 0
  retry_${UniqueID}:
    IfFileExists "${FILE}" 0 done_${UniqueID}
    ClearErrors
    FileOpen $0 "${FILE}" a
    IfErrors 0 closed_${UniqueID}
    IntCmp $1 30 ask_${UniqueID}
    IntOp $1 $1 + 1
    Sleep 500
    Goto retry_${UniqueID}
  ask_${UniqueID}:
    MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION \
      "${LABEL} is running. Quit it (also from the tray icon) and press Retry." \
      IDRETRY retry_${UniqueID}
    Abort
  closed_${UniqueID}:
    FileClose $0
  done_${UniqueID}:
  !undef UniqueID
!macroend

Function .onInit
  ${GetParameters} $0
  ClearErrors
  ${GetOptions} $0 "/RELAUNCH" $1
  StrCpy $Relaunch 0
  IfErrors +2
    StrCpy $Relaunch 1
FunctionEnd

; After an in-app update, open Kandado again (the finish page offers it otherwise).
Function .onInstSuccess
  StrCmp $Relaunch 1 0 +2
    Exec '"$INSTDIR\kandado.exe"'
FunctionEnd

Section "Kandado" Main
  SectionIn RO
  !insertmacro WaitUntilClosed "$INSTDIR\kandado.exe" "Kandado"
  ; The native host runs while a browser talks to it. A running program can
  ; be renamed but not replaced: move it aside, and the browser's next
  ; connection starts the new one. The old copy goes on the next install.
  Delete "$INSTDIR\kandado-native-host.exe.old"
  IfFileExists "$INSTDIR\kandado-native-host.exe" 0 +2
    Rename "$INSTDIR\kandado-native-host.exe" "$INSTDIR\kandado-native-host.exe.old"
  !insertmacro WaitUntilClosed "$INSTDIR\kandado-native-host.exe" \
    "Kandado's browser link (close the browsers with the Kandado extension)"

  SetOutPath "$INSTDIR"
  File "${SOURCE}\kandado.exe"
  ; Must sit beside kandado.exe (P5-31).
  File "${SOURCE}\kandado-native-host.exe"
  File "${SOURCE}\LICENSE.txt"
  WriteUninstaller "$INSTDIR\uninstall.exe"

  CreateShortcut "$SMPROGRAMS\Kandado.lnk" "$INSTDIR\kandado.exe"

  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "${APP}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "Kandado"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\kandado.exe"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKCU "${UNINSTALL_KEY}" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1
  SectionGetSize ${Main} $0
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "EstimatedSize" $0
SectionEnd

Section "Uninstall"
  !insertmacro WaitUntilClosed "$INSTDIR\kandado.exe" "Kandado"
  !insertmacro WaitUntilClosed "$INSTDIR\kandado-native-host.exe" \
    "Kandado's browser link (close the browsers with the Kandado extension)"

  ; The browser link (D-45): its manifests sit beside the native host.
  Delete "$INSTDIR\${HOST_NAME}*.json"
  DeleteRegKey HKCU "Software\Google\Chrome\NativeMessagingHosts\${HOST_NAME}"
  DeleteRegKey HKCU "Software\Microsoft\Edge\NativeMessagingHosts\${HOST_NAME}"
  DeleteRegKey HKCU "Software\Mozilla\NativeMessagingHosts\${HOST_NAME}"
  ; Start with Windows (D-43).
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Kandado"

  Delete "$SMPROGRAMS\Kandado.lnk"
  Delete "$INSTDIR\kandado.exe"
  Delete "$INSTDIR\kandado-native-host.exe"
  Delete "$INSTDIR\kandado-native-host.exe.old"
  Delete "$INSTDIR\LICENSE.txt"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  DeleteRegKey HKCU "${UNINSTALL_KEY}"
SectionEnd
