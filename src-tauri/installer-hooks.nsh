!define FLUX_INSTALLER_HOOK_DIR "${__FILEDIR__}"
!include "${FLUX_INSTALLER_HOOK_DIR}\installer-locale.nsh"
!macro STOP_FLUX_BROWSER_HOSTS
  Push $R9
  InitPluginsDir
  File /oname=$PLUGINSDIR\flux-stop-browser-hosts.ps1 "${FLUX_INSTALLER_HOOK_DIR}\stop-browser-hosts.ps1"
  StrCpy $R9 "$SYSDIR\WindowsPowerShell\v1.0\powershell.exe"
  ; The 32-bit installer must inspect the 64-bit native host through native PowerShell.
  ${If} ${FileExists} "$WINDIR\Sysnative\WindowsPowerShell\v1.0\powershell.exe"
    StrCpy $R9 "$WINDIR\Sysnative\WindowsPowerShell\v1.0\powershell.exe"
  ${EndIf}
  nsExec::ExecToStack '"$R9" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$PLUGINSDIR\flux-stop-browser-hosts.ps1" -Directory "$INSTDIR"'
  Pop $0
  Pop $1
  Pop $R9
  ${If} $0 != 0
    SetErrorLevel 1
    Abort "Flux could not close its browser bridge. Close your browser and retry."
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREINSTALL
  ${If} ${FileExists} "$INSTDIR\flux-desktop.exe"
    ExecWait '"$INSTDIR\flux-desktop.exe" --shutdown' $0
    ${If} $0 != 0
      SetErrorLevel 1
      Abort "Flux could not finish saving active downloads. Close the engine and retry."
    ${EndIf}
  ${EndIf}
  !insertmacro STOP_FLUX_BROWSER_HOSTS
!macroend

!macro NSIS_HOOK_POSTINSTALL
  !insertmacro FLUX_SAVE_INSTALLER_LOCALE
  ExecWait '"$INSTDIR\flux-desktop.exe" --register-browser' $0
  ${If} $0 != 0
    DetailPrint "Chrome, Edge, Brave, Chromium and Firefox bridge registration failed ($0). Reconnect from Flux settings."
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ExecWait '"$INSTDIR\flux-desktop.exe" --shutdown' $0
  ${If} $0 != 0
    SetErrorLevel 1
    Abort "Flux could not finish saving active downloads. Close the engine and retry."
  ${EndIf}
  !insertmacro STOP_FLUX_BROWSER_HOSTS
  ExecWait '"$INSTDIR\flux-desktop.exe" --unregister-browser' $0
  ${If} $0 != 0
    SetErrorLevel 1
    Abort "Flux could not unregister its browser bridges. Repair the installation and retry."
  ${EndIf}
!macroend
