; Restore the previous URL handler before deleting our executable.
; An update keeps the same installed executable path and must retain the opt-in.
!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode <> 1
    ; Inspect names so an empty or wrong-type backup still reaches Rust validation.
    StrCpy $2 0
    StrCpy $3 0
    ${Do}
      ClearErrors
      EnumRegValue $1 HKCU "Software\QB Gate\DeepLinkCompatibility" $2
      ${If} ${Errors}
        ${ExitDo}
      ${EndIf}
      ${If} $1 == "ccswitch-backup"
        StrCpy $3 1
        ${ExitDo}
      ${EndIf}
      IntOp $2 $2 + 1
    ${Loop}
    ${If} $3 = 1
      ClearErrors
      ExecWait '"$INSTDIR\qb-gate.exe" --restore-ccswitch-handler' $0
      ${IfThen} ${Errors} ${|} StrCpy $0 1 ${|}
      ${If} $0 <> 0
        MessageBox MB_OK|MB_ICONEXCLAMATION "Unable to restore the previous CC Switch URL handler. Open QB Gate and turn off web import compatibility before uninstalling."
        Abort
      ${EndIf}
    ${EndIf}
  ${EndIf}
!macroend
