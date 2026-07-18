#!/usr/bin/env bash
# Lanzador de instalación para Linux y macOS.
#
#   chmod +x instalar.sh && ./instalar.sh
#
# Busca un Python válido, avisa con claridad si no lo hay, y cede el control
# a instalar.py, que es donde vive la lógica real.

set -euo pipefail
cd "$(dirname "$0")"

echo
echo "  Informes — instalador"
echo

encontrar_python() {
    for cmd in python3 python; do
        if command -v "$cmd" >/dev/null 2>&1; then
            # Descarta Python 2, que todavía anda por ahí como `python`.
            if "$cmd" -c 'import sys; sys.exit(0 if sys.version_info >= (3, 9) else 1)' 2>/dev/null; then
                echo "$cmd"
                return 0
            fi
        fi
    done
    return 1
}

if PY=$(encontrar_python); then
    exec "$PY" instalar.py "$@"
fi

echo "  ✗ No se encontró Python 3.9 o superior."
echo
case "$(uname -s)" in
    Darwin)
        echo "    Instálalo con Homebrew:"
        echo "        brew install python"
        echo
        echo "    O descárgalo de https://www.python.org/downloads/macos/"
        ;;
    Linux)
        echo "    Según tu distribución:"
        echo "        Debian / Ubuntu   sudo apt install python3 python3-venv"
        echo "        Fedora / RHEL     sudo dnf install python3"
        echo "        Arch              sudo pacman -S python"
        ;;
    *)
        echo "    Descárgalo de https://www.python.org/downloads/"
        ;;
esac
echo
echo "  Cuando termine, vuelve a ejecutar:  ./instalar.sh"
echo
exit 1
