; Hooks into Tauri's NSIS installer (bundle.windows.nsis.installerHooks).
;
; The installer already runs elevated (installMode perMachine), so it sets up
; OpenWire's services itself rather than the app asking for admin a second
; time later. The service logic stays in the app binary: these only call its
; --netmon-install / --netmon-uninstall (core/netmon) and --tunnel-install /
; --tunnel-uninstall (core/tun) switches.

!define NETMON_SERVICE_KEY "SYSTEM\CurrentControlSet\Services\OpenWireNetMon"
; Written once the question has been answered, so updates never ask again -
; including after someone said no, or later turned monitoring off in Settings.
!define NETMON_ASKED "NetMonAsked"

!define TUNNEL_SERVICE_KEY "SYSTEM\CurrentControlSet\Services\OpenWireTunnel"
; Written once the tunnel service has been set up, so an update does not put
; it back after someone removed it in Settings.
!define TUNNEL_SET_UP "TunnelSetUp"

!macro NSIS_HOOK_POSTINSTALL
  ; The tunnel service: system-wide routing. Idle until the app asks for it,
  ; so it is set up without a question - like every VPN's own service.
  ClearErrors
  ReadRegStr $R0 HKLM "${TUNNEL_SERVICE_KEY}" "ImagePath"
  ${IfNot} ${Errors}
    ; Already there: refresh its copy of the binary to this version.
    ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --tunnel-install'
  ${Else}
    ReadRegDWORD $R1 SHCTX "${MANUPRODUCTKEY}" "${TUNNEL_SET_UP}"
    ${If} $R1 <> 1
      WriteRegDWORD SHCTX "${MANUPRODUCTKEY}" "${TUNNEL_SET_UP}" 1
      ; A failure is not worth stopping the install for: Settings > Routing
      ; offers to set it up again.
      ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --tunnel-install'
    ${EndIf}
  ${EndIf}

  ClearErrors
  ReadRegStr $R0 HKLM "${NETMON_SERVICE_KEY}" "ImagePath"
  ${IfNot} ${Errors}
    ; Already on: the service runs a copy of the app taken at install time,
    ; now older than the one just installed. Refreshing it here spares the
    ; "Repair" prompt in Settings.
    ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --netmon-install'
  ${Else}
    ReadRegDWORD $R1 SHCTX "${MANUPRODUCTKEY}" "${NETMON_ASKED}"
    ${If} $R1 <> 1
      WriteRegDWORD SHCTX "${MANUPRODUCTKEY}" "${NETMON_ASKED}" 1

      ; Passive and silent installs take the default, which is yes.
      StrCpy $R2 1
      ${If} $PassiveMode <> 1
        StrCpy $R3 "Turn on detailed monitoring?$\r$\n$\r$\n"
        StrCpy $R3 "$R3${PRODUCTNAME} counts every app, host and traffic type on this PC, "
        StrCpy $R3 "$R3not just what passes through the tunnel. It reads Windows' own "
        StrCpy $R3 "$R3network events - no driver, and never the contents of your traffic."
        StrCpy $R3 "$R3$\r$\n$\r$\nYou can change this later in Settings."
        MessageBox MB_YESNO|MB_ICONQUESTION "$R3" /SD IDYES IDYES netmon_yes
        StrCpy $R2 0
        netmon_yes:
      ${EndIf}

      ${If} $R2 = 1
        StrCpy $R0 ""
        ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --netmon-install' $R0
        ${If} $R0 != 0
        ${AndIf} $PassiveMode <> 1
          StrCpy $R3 "Detailed monitoring couldn't be set up (code $R0). "
          StrCpy $R3 "$R3You can turn it on later in Settings."
          MessageBox MB_OK|MB_ICONEXCLAMATION "$R3" /SD IDOK
        ${EndIf}
      ${EndIf}
    ${EndIf}
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; An update reinstalls over the top and keeps the service; only a real
  ; uninstall removes it, and forgets the answer for a later reinstall.
  ${If} $UpdateMode <> 1
    ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --netmon-uninstall'
    DeleteRegValue SHCTX "${MANUPRODUCTKEY}" "${NETMON_ASKED}"
    ; Also removes the Wintun driver, unless other software still uses it.
    ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --tunnel-uninstall'
    DeleteRegValue SHCTX "${MANUPRODUCTKEY}" "${TUNNEL_SET_UP}"
  ${EndIf}
!macroend
