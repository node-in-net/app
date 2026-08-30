!include "MUI2.nsh"
!include "x64.nsh"

Name "NodeInNet"
OutFile "..\\..\\distr\\nodeinnet-gtk-0.5.513-1-win64.exe"
InstallDir "$PROGRAMFILES64\NodeInNet"
Target amd64-unicode

SetCompressor /SOLID lzma

RequestExecutionLevel admin

!define MUI_ICON "assets\win32-icon.ico"
!define MUI_UNICON "assets\win32-icon.ico"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES

!define MUI_FINISHPAGE_RUN "$INSTDIR\nodeinnet-gtk.exe"
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_WELCOME
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_UNPAGE_FINISH

!insertmacro MUI_LANGUAGE "English"
!insertmacro MUI_LANGUAGE "Russian"

Function .onInit
    ${If} ${RunningX64}
        SetRegView 64
        StrCpy $INSTDIR "$PROGRAMFILES64\NodeInNet"
    ${Else}
        MessageBox MB_OK|MB_ICONSTOP "This program requires 64-bit Windows / Эта программа требует 64-битную версию Windows."
        Abort
    ${EndIf}
FunctionEnd

Section "NodeInNet (Required)" SecMain
    SectionIn RO ; required, cannot be deselected
    SetOutPath "$INSTDIR"

    File "..\..\bin\distr\exe\target\x86_64-pc-windows-gnu\release\nodeinnet-gtk.exe"
    File "..\..\bin\distr\exe\target\x86_64-pc-windows-gnu\release\node_network.dll"

    File /r "..\..\artifacts\gtk4-win32-x64\*"

    CreateDirectory "$SMPROGRAMS\NodeInNet"
    CreateShortcut "$SMPROGRAMS\NodeInNet\NodeInNet.lnk" "$INSTDIR\nodeinnet-gtk.exe" "" "$INSTDIR\nodeinnet-gtk.exe" 0
    CreateShortcut "$SMPROGRAMS\NodeInNet\Uninstall NodeInNet.lnk" "$INSTDIR\Uninstall.exe"

    WriteUninstaller "$INSTDIR\Uninstall.exe"

    WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\NodeInNet" "DisplayName" "NodeInNet"
    WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\NodeInNet" "UninstallString" '"$INSTDIR\Uninstall.exe"'
    WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\NodeInNet" "DisplayIcon" '"$INSTDIR\nodeinnet-gtk.exe"'
    WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\NodeInNet" "DisplayVersion" "0.5.513"
    WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\NodeInNet" "Publisher" "NodeInNet"
SectionEnd

Section /o "Create Desktop Shortcut" SecDesktop
    CreateShortcut "$DESKTOP\NodeInNet.lnk" "$INSTDIR\nodeinnet-gtk.exe"
SectionEnd

Section "Uninstall"
	SetRegView 64

    ExecWait 'taskkill /F /IM nodeinnet-gtk.exe'

    RMDir /r "$INSTDIR"

    Delete "$SMPROGRAMS\NodeInNet\NodeInNet.lnk"
    Delete "$SMPROGRAMS\NodeInNet\Uninstall NodeInNet.lnk"
    RMDir "$SMPROGRAMS\NodeInNet"
    Delete "$DESKTOP\NodeInNet.lnk"

    DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\NodeInNet"
SectionEnd
