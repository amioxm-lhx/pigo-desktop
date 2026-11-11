@echo off
set P=C:\ProgramData\Microsoft\VisualStudio\Packages
set T=C:\Users\Administrator\AppData\Local\Temp
set N1=Microsoft.VC.14.43.17.13.CRT.x64.Desktop.base,version=14.43.34808
set N2=Microsoft.VC.14.43.17.13.Tools.HostX64.TargetX64.base,version=14.43.34809,chip=x64
set N3=Microsoft.VC.14.43.17.13.Tools.HostX64.TargetX86.base,version=14.43.34809,chip=x64
set LOG=%T%\vs-finish.log
echo START > "%LOG%"
mkdir "%P%\%N1%" 2>nul
mkdir "%P%\%N2%" 2>nul
mkdir "%P%\%N3%" 2>nul
copy /Y "%T%\vsix1.vsix" "%P%\%N1%\payload.vsix" >> "%LOG%" 2>&1
copy /Y "%T%\vsix2.vsix" "%P%\%N2%\payload.vsix" >> "%LOG%" 2>&1
copy /Y "%T%\vsix3.vsix" "%P%\%N3%\payload.vsix" >> "%LOG%" 2>&1
echo COPIED >> "%LOG%"
taskkill /F /PID 7068 /PID 7224 /T >> "%LOG%" 2>&1
timeout /t 5 /nobreak >nul
"C:\Program Files (x86)\Microsoft Visual Studio\Installer\setup.exe" modify --installPath "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools" --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended --quiet --norestart >> "%LOG%" 2>&1
echo SETUP_EXIT=%errorlevel% >> "%LOG%"
