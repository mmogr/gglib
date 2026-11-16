; gglib's additions to the desktop app's Windows installer. Tauri's NSIS
; template includes this file (bundle > windows > nsis > installerHooks in
; tauri.windows.conf.json) and runs each macro at its point.
;
; Before an install writes its files, and before an uninstall removes them,
; the desktop app is closed and then gglib's service is stopped. The service
; is the gglib.exe in this folder; it goes on running after the app's window
; closes, and Windows will not replace or delete a program while it runs.
; The app goes first because it is the service's client and can start the
; service again (at its launch, or from its tray). If gglib.exe is still
; running after that, the hook says so and stops the install or uninstall
; before the template writes or removes a file, rather than replacing half
; of them. With its pages shown, an upgrade may first run the installed
; uninstaller (the reinstall page's default), before these hooks: one from
; before this file removes what it can and leaves the running gglib.exe,
; which the install's hook then stops.
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
; PATH edit that fails or times out adds a line of what to do, and the
; install or uninstall carries on: the app works without gglib on PATH.

; Stop the service and make sure nothing runs this folder's gglib.exe any
; more; otherwise say why and abort, before a file is written or removed.
;
; The gglib.exe that stops it is the one already installed, the only one
; here before an install: on an upgrade, the previous release. So it runs
; `daemon stop` and nothing a newer gglib might add. That asks the service
; on this user's port to stop, ends its engines and waits for it, a little
; past the service's own 30-second watchdog; with none running it says so
; and exits 0. On a first install there is no gglib.exe and nothing to stop.
;
; Its exit code does not decide whether to go on. A service started with
; another GGLIB_DAEMON_PORT is not found, yet runs this gglib.exe; one from
; another gglib on the same port, the CLI archive's, is stopped, which does
; no harm, though its program is not in this folder; a gglib command in a
; terminal is no service at all. What decides is whether this folder's
; gglib.exe opens for writing, which Windows refuses while any process runs
; it. It is tried for ten seconds, for a process that is still exiting.
!macro GGLIB_STOP_SERVICE
  ${If} ${FileExists} "$INSTDIR\gglib.exe"
    DetailPrint "Stopping gglib's service, if it is running"
    nsExec::ExecToLog /TIMEOUT=60000 '"$INSTDIR\gglib.exe" daemon stop'
    Pop $0
    ${If} $0 != "0"
      DetailPrint "gglib daemon stop did not finish cleanly ($0)."
    ${EndIf}
    StrCpy $1 0
    ${Do}
      ClearErrors
      FileOpen $2 "$INSTDIR\gglib.exe" a
      ${IfNot} ${Errors}
        FileClose $2
        ${Break}
      ${EndIf}
      IntOp $1 $1 + 1
      ${If} $1 >= 20
        StrCpy $3 "gglib is still running from $INSTDIR, so its files cannot be replaced or removed. Run `gglib daemon stop`, close any terminal running gglib, then try again."
        DetailPrint "$3"
        ${If} ${Silent}
          System::Call 'kernel32::AttachConsole(i -1)i.r0'
          ${If} $0 != 0
            System::Call 'kernel32::GetStdHandle(i -11)i.r0'
            FileWrite $0 "$3$\n"
          ${EndIf}
        ${Else}
          MessageBox MB_ICONSTOP|MB_OK "$3" /SD IDOK
        ${EndIf}
        Abort "$3"
      ${EndIf}
      Sleep 500
    ${Loop}
  ${EndIf}
!macroend

; Before the template writes a file. It asks whether to close a running app
; only after this hook, so the same question is asked here first, then the
; service is stopped; the template's own check then finds nothing running.
!macro NSIS_HOOK_PREINSTALL
  !insertmacro CheckIfAppIsRunning "$INSTDIR\${MAINBINARYNAME}.exe" "${PRODUCTNAME}"
  !insertmacro GGLIB_STOP_SERVICE
!macroend

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
; stops the uninstall, so the same question is asked here first, then the
; service is stopped, which can stop the uninstall too: either way a
; stopped uninstall leaves PATH as it was. The template's own check then
; finds nothing running. An update (/UPDATE) installs into the same folder
; straight after, so it leaves the PATH entry; its files go all the same,
; so it closes the app and stops the service too.
!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro CheckIfAppIsRunning "$INSTDIR\${MAINBINARYNAME}.exe" "${PRODUCTNAME}"
  !insertmacro GGLIB_STOP_SERVICE
  ${If} $UpdateMode <> 1
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
