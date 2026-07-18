@echo off
REM Lanzador de instalación para Windows. Haz doble clic en este archivo.
REM Busca Python, avisa con claridad si no está, y cede el control a instalar.py.

setlocal
cd /d "%~dp0"

echo.
echo   Informes - instalador para Windows
echo.

REM El lanzador `py` es el que instala python.org; `python` puede ser el alias
REM de la Microsoft Store, que no sirve. Se prueba `py` primero a propósito.
where py >nul 2>&1
if %errorlevel% equ 0 (
    py -3 instalar.py %*
    goto :fin
)

where python >nul 2>&1
if %errorlevel% equ 0 (
    python instalar.py %*
    goto :fin
)

echo   [X] No se encontro Python en este equipo.
echo.
echo   Instalalo desde:  https://www.python.org/downloads/
echo.
echo   IMPORTANTE: en la primera pantalla del instalador de Python,
echo   marca la casilla "Add python.exe to PATH" antes de continuar.
echo.
echo   Cuando termine, vuelve a hacer doble clic en este archivo.
echo.

:fin
echo.
pause
endlocal
