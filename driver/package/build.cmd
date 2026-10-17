@echo off
rem Builds, catalogs and test-signs the Airtight virtual speaker into out\.
rem Needs the VS 2022 Build Tools and the WDK 10.0.26100.
setlocal
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul || exit /b 1
set SRC=%~dp0..\source
set OUT=%~dp0out
set K=C:\Program Files (x86)\Windows Kits\10
set V=10.0.26100.0
set KMDF=1.15
if not exist "%OUT%\obj" mkdir "%OUT%\obj"
set INC=/I "%K%\Include\%V%\km\crt" /I "%K%\Include\%V%\km" /I "%K%\Include\%V%\shared" /I "%K%\Include\wdf\kmdf\%KMDF%" /I "%SRC%" /I "%SRC%\Inc" /I "%SRC%\Filters" /I "%SRC%\Utilities" /I "%SRC%\Main"
set DEF=/D_AMD64_ /DAMD64 /D_WIN64 /D_WIN32 /DUNICODE /D_UNICODE /DPC_IMPLEMENTATION /D_USE_WAVERT_ /D_NEW_DELETE_OPERATORS_ /DDEBUG_LEVEL=DEBUGLVL_TERSE /DPOOL_NX_OPTIN=1 /DNTDDI_VERSION=0x0A00000C /D_WIN32_WINNT=0x0A00 /DWINVER=0x0A00 /DKMDF_VERSION_MAJOR=1 /DKMDF_VERSION_MINOR=15
set CF=/nologo /c /kernel /Zp8 /Gy /GR- /GF /GS /Gz /O2 /Oi /W3 /Zi /Zc:wchar_t- /Zc:inline /fp:precise /FS /Fd"%OUT%\obj\\"
for %%D in (Main Filters Utilities) do for %%F in ("%SRC%\%%D\*.cpp") do (
  cl %CF% %INC% %DEF% /Fo"%OUT%\obj\%%~nF.obj" "%%F" || exit /b 1
)
rc /nologo %INC% %DEF% /fo"%OUT%\obj\driver.res" "%SRC%\Main\SimpleAudioSample.rc" || exit /b 1
set L=%K%\Lib\%V%\km\x64
set W=%K%\Lib\wdf\kmdf\x64\%KMDF%
link /nologo /DRIVER /SUBSYSTEM:NATIVE,10.00 /ENTRY:FxDriverEntry /NODEFAULTLIB /INTEGRITYCHECK /MERGE:_TEXT=.text;_PAGE=PAGE /SECTION:INIT,d /OPT:REF /OPT:ICF /DEBUG /RELEASE /guard:cf /OUT:"%OUT%\airtightaudio.sys" "%OUT%\obj\*.obj" "%OUT%\obj\driver.res" "%L%\ntoskrnl.lib" "%L%\hal.lib" "%L%\wmilib.lib" "%L%\BufferOverflowFastFailK.lib" "%L%\portcls.lib" "%L%\stdunk.lib" "%L%\libcntpr.lib" "%L%\ks.lib" "%L%\ksguid.lib" "%W%\WdfDriverEntry.lib" "%W%\WdfLdr.lib" || exit /b 1
copy /y "%~dp0airtightaudio.inf" "%OUT%\" >nul
"%K%\bin\%V%\x86\Inf2Cat.exe" /driver:"%OUT%" /os:10_x64 /uselocaltime || exit /b 1
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0sign.ps1" "%OUT%" || exit /b 1
echo BUILD OK: %OUT%
