!macro NSIS_HOOK_PREINSTALL
  ${If} ${FileExists} "$INSTDIR\flux-desktop.exe"
    ExecWait '"$INSTDIR\flux-desktop.exe" --shutdown' $0
    ${If} $0 != 0
      Abort "Flux could not finish saving active downloads. Close the engine and retry."
    ${EndIf}
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ExecWait '"$INSTDIR\flux-desktop.exe" --register-browser' $0
  ${If} $0 != 0
    DetailPrint "Chrome, Edge, Chromium and Firefox bridge registration failed ($0). Reconnect from Flux settings."
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ExecWait '"$INSTDIR\flux-desktop.exe" --shutdown' $0
  ${If} $0 != 0
    Abort "Flux could not finish saving active downloads. Close the engine and retry."
  ${EndIf}
  ExecWait '"$INSTDIR\flux-desktop.exe" --unregister-browser' $0
  ${If} $0 != 0
    Abort "Flux could not unregister its browser bridges. Repair the installation and retry."
  ${EndIf}
!macroend
