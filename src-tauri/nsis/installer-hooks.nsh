; Tauri NSIS installer hooks.
; tauri.conf.json's bundle.windows.nsis.installerHooks points here.
; No custom hooks are currently needed; the no-op macros keep Windows NSIS builds valid.

!macro NSIS_HOOK_PREINSTALL
!macroend

!macro NSIS_HOOK_POSTINSTALL
!macroend

!macro NSIS_HOOK_PREUNINSTALL
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
!macroend
