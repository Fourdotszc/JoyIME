; JoyIME 安装程序的附加步骤（Tauri NSIS 安装程序在对应时机调用这些宏）。
; 安装后的目录：
;   $INSTDIR\joyime-app.exe          后台程序 + 设置窗口
;   $INSTDIR\joyime_tsf.dll          64 位系统输入法 DLL（64 位程序加载）
;   $INSTDIR\x86\joyime_tsf.dll      32 位系统输入法 DLL（32 位程序加载）
;   $INSTDIR\joyime.ico              输入法图标
; 安装程序本身是 32 位的：System32 会被重定向到 SysWOW64，所以 64 位的 regsvr32 用 Sysnative 这个路径。

!macro JOYIME_UNREGISTER
  ${If} ${FileExists} "$INSTDIR\joyime_tsf.dll"
    nsExec::Exec '"$WINDIR\Sysnative\regsvr32.exe" /s /u "$INSTDIR\joyime_tsf.dll"'
    Pop $0
  ${EndIf}
  ${If} ${FileExists} "$INSTDIR\x86\joyime_tsf.dll"
    nsExec::Exec '"$WINDIR\SysWOW64\regsvr32.exe" /s /u "$INSTDIR\x86\joyime_tsf.dll"'
    Pop $0
  ${EndIf}
!macroend

; 输入法 DLL 正被别的程序加载着时不能覆盖、不能删，但可以改名：改名后让它重启时删掉，新文件照常写进来
!macro JOYIME_MOVE_LOCKED_DLL path
  Delete "${path}.old"
  ${If} ${FileExists} "${path}"
    Rename "${path}" "${path}.old"
    Delete /REBOOTOK "${path}.old"
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREINSTALL
  ; 旧版本的后台程序还在跑：先关掉，不然 exe 写不进去
  nsExec::Exec 'taskkill /F /IM joyime-app.exe'
  Pop $0
  !insertmacro JOYIME_MOVE_LOCKED_DLL "$INSTDIR\joyime_tsf.dll"
  !insertmacro JOYIME_MOVE_LOCKED_DLL "$INSTDIR\x86\joyime_tsf.dll"
!macroend

!macro NSIS_HOOK_POSTINSTALL
  DetailPrint "注册系统输入法…"
  nsExec::ExecToLog '"$WINDIR\Sysnative\regsvr32.exe" /s "$INSTDIR\joyime_tsf.dll"'
  Pop $0
  nsExec::ExecToLog '"$WINDIR\SysWOW64\regsvr32.exe" /s "$INSTDIR\x86\joyime_tsf.dll"'
  Pop $1
  ${If} $0 != 0
  ${OrIf} $1 != 0
    MessageBox MB_ICONEXCLAMATION "注册系统输入法没有成功（$0 / $1）。可以重新运行安装程序再试一次。"
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  nsExec::Exec 'taskkill /F /IM joyime-app.exe'
  Pop $0
  !insertmacro JOYIME_UNREGISTER
  ; 已经打开的程序里还加载着 DLL：改名，重启后删
  !insertmacro JOYIME_MOVE_LOCKED_DLL "$INSTDIR\joyime_tsf.dll"
  !insertmacro JOYIME_MOVE_LOCKED_DLL "$INSTDIR\x86\joyime_tsf.dll"
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  RMDir /REBOOTOK "$INSTDIR\x86"
  RMDir /REBOOTOK "$INSTDIR"
!macroend
