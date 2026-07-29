#!/usr/bin/env bash
# Construye el artefacto que recibe el cliente: UN archivo.
#
#   ./rust/construir.sh
#
# Orden obligatorio y por qué: la interfaz se compila primero porque el binario
# la lleva DENTRO (`rust-embed`). Si se invierte, el binario sale con la interfaz
# de la vez anterior y nadie lo nota hasta que alguien pregunta por qué su
# arreglo no aparece — el mismo patrón de fallo silencioso que el orden
# esquema/migración ya costó una vez.
set -euo pipefail

RAIZ="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$RAIZ"

echo "==> 1/3  interfaz (Angular)"
cd angular
npm ci --silent || npm install --silent
npx ng build --configuration production
cd "$RAIZ"

echo "==> 2/3  empotrar la interfaz en el binario"
# Se vacía antes de copiar: si un archivo desaparece de la compilación nueva
# —un fragmento que se dividió distinto— y aquí quedara el viejo, el binario
# viajaría con código muerto que además podría servirse.
rm -rf rust/siger-api/estaticos
mkdir -p rust/siger-api/estaticos
cp -r angular/dist/siger-web/browser/. rust/siger-api/estaticos/

echo "==> 3/3  binario (Rust)"
cd rust
cargo build --release

BIN="rust/target/release/siger-api"
cd "$RAIZ"
echo
echo "Listo: $BIN  ($(du -h "$BIN" | cut -f1))"
echo
echo "Para arrancar:"
echo "  SIGER_SECRETO_SESION=\$($BIN generar-secreto) $BIN"
