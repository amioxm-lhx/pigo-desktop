@echo off
set L=D:\Workspace\golang\pigo-desktop\desktop\probe-explorer.log
echo === spawnprobe from Temp === > "%L%"
"C:\Users\Administrator\AppData\Local\Temp\sp-a\debug\spawnprobe.exe" >> "%L%" 2>&1
echo EXIT_A=%errorlevel% >> "%L%"
echo === freshprobe from .msvc === >> "%L%"
"D:\Workspace\golang\pigo-desktop\desktop\.msvc\probe.exe" >> "%L%" 2>&1
echo EXIT_B=%errorlevel% >> "%L%"
echo PROBE_DONE >> "%L%"
