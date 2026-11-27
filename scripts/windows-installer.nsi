!include "MUI2.nsh"
!include "x64.nsh"

!ifndef APP_VERSION
  !define APP_VERSION "0.2.0"
!endif
!ifndef STAGE_DIR
  !define STAGE_DIR "..\dist\ChronoBase-windows-x64-portable"
!endif
!ifndef OUT_FILE
  !define OUT_FILE "..\dist\ChronoBase-windows-x64-setup.exe"
!endif
!ifndef ICON_FILE
  !define ICON_FILE "..\crates\app\assets\icon.ico"
!endif

Name "ChronoBase ${APP_VERSION}"
OutFile "${OUT_FILE}"
InstallDir "$PROGRAMFILES64\ChronoBase"
InstallDirRegKey HKLM "Software\ChronoBase" "InstallDir"
RequestExecutionLevel admin
Unicode True
SetCompressor /SOLID lzma

!define MUI_ABORTWARNING
!define MUI_ICON "${ICON_FILE}"
!define MUI_UNICON "${ICON_FILE}"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "${STAGE_DIR}\LICENSE"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_RUN "$INSTDIR\chronobase.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Launch ChronoBase"
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

Section "ChronoBase" SecMain
  SectionIn RO
  ${If} ${RunningX64}
    SetRegView 64
  ${EndIf}

  SetOutPath "$INSTDIR"
  File "${STAGE_DIR}\chronobase.exe"
  File "${STAGE_DIR}\README.md"
  File "${STAGE_DIR}\LICENSE"
  File "${STAGE_DIR}\FONT-LICENSE-OFL.txt"
  File "${STAGE_DIR}\SLINT-ATTRIBUTION.txt"

  WriteUninstaller "$INSTDIR\Uninstall.exe"

  CreateDirectory "$SMPROGRAMS\ChronoBase"
  CreateShortcut "$SMPROGRAMS\ChronoBase\ChronoBase.lnk" "$INSTDIR\chronobase.exe" "" "$INSTDIR\chronobase.exe" 0
  CreateShortcut "$SMPROGRAMS\ChronoBase\Uninstall ChronoBase.lnk" "$INSTDIR\Uninstall.exe"
  CreateShortcut "$DESKTOP\ChronoBase.lnk" "$INSTDIR\chronobase.exe" "" "$INSTDIR\chronobase.exe" 0

  WriteRegStr HKLM "Software\ChronoBase" "InstallDir" "$INSTDIR"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\ChronoBase" "DisplayName" "ChronoBase"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\ChronoBase" "DisplayVersion" "${APP_VERSION}"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\ChronoBase" "Publisher" "Subham Maity"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\ChronoBase" "DisplayIcon" "$INSTDIR\chronobase.exe"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\ChronoBase" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\ChronoBase" "NoModify" 1
  WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\ChronoBase" "NoRepair" 1
SectionEnd

Section "Uninstall"
  ${If} ${RunningX64}
    SetRegView 64
  ${EndIf}

  Delete "$DESKTOP\ChronoBase.lnk"
  Delete "$SMPROGRAMS\ChronoBase\ChronoBase.lnk"
  Delete "$SMPROGRAMS\ChronoBase\Uninstall ChronoBase.lnk"
  RMDir "$SMPROGRAMS\ChronoBase"

  Delete "$INSTDIR\chronobase.exe"
  Delete "$INSTDIR\README.md"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\FONT-LICENSE-OFL.txt"
  Delete "$INSTDIR\SLINT-ATTRIBUTION.txt"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"

  DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\ChronoBase"
  DeleteRegKey HKLM "Software\ChronoBase"
SectionEnd
