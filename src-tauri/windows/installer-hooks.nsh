; gglib's additions to the desktop app's Windows installer. Tauri's NSIS
; template includes this file (bundle > windows > nsis > installerHooks in
; tauri.windows.conf.json) and runs each macro at its point.
;
; The install puts its folder, which holds gglib.exe beside the app, on this
; user's own PATH, so `gglib` runs in a terminal opened afterwards, and the
; uninstall takes it off again if the install put it there: a folder that
; was on PATH already stays. gglib.exe edits the variable itself
; (`gglib config path-entry`, gglib_core::paths::user_path): an NSIS string
; holds 1024 characters in the build Tauri uses, and a longer PATH read into
; one and written back here would be cut short.
;
; gglib.exe prints one line of what it did, which nsExec puts in the log. A
; failure or a timeout adds a line of what to do, and the install or
; uninstall carries on: the app works without gglib on PATH.

!macro NSIS_HOOK_POSTINSTALL
  DetailPrint "Putting $INSTDIR on your PATH, so a new terminal runs gglib"
  nsExec::ExecToLog /TIMEOUT=60000 '"$INSTDIR\gglib.exe" config path-entry add'
  Pop $0
  ${If} $0 == "timeout"
    DetailPrint "gglib did not finish putting $INSTDIR on your PATH in time. Check whether it is on your PATH; if it is not, add it to run gglib in a terminal."
  ${ElseIf} $0 != "0"
    DetailPrint "gglib could not put $INSTDIR on your PATH ($0). The app works; to run gglib in a terminal, add $INSTDIR to your PATH."
  ${EndIf}
!macroend

; Before the files go, since gglib.exe is one of them. The template asks
; whether to close a running app only after this hook, and Cancel there
; stops the uninstall, so the same question is asked here first: a
; cancelled uninstall leaves PATH as it was. The template's own check then
; finds nothing running. An update (/UPDATE) installs into the same folder
; straight after, so it leaves the entry.
!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode <> 1
    !insertmacro CheckIfAppIsRunning "$INSTDIR\${MAINBINARYNAME}.exe" "${PRODUCTNAME}"
    DetailPrint "Taking $INSTDIR off your PATH, if the install put it there"
    nsExec::ExecToLog /TIMEOUT=60000 '"$INSTDIR\gglib.exe" config path-entry remove'
    Pop $0
    ${If} $0 == "timeout"
      DetailPrint "gglib did not finish taking $INSTDIR off your PATH in time. Check whether it is still on your PATH."
    ${ElseIf} $0 != "0"
      DetailPrint "gglib could not take $INSTDIR off your PATH ($0). If the install put it there, remove it by hand."
    ${EndIf}
  ${EndIf}
!macroend
