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

echo "==> 1/4  interfaz (Angular)"
cd angular
npm ci --silent || npm install --silent
npx ng build --configuration production
cd "$RAIZ"

echo "==> 2/4  empotrar la interfaz en el binario"
# Se vacía antes de copiar: si un archivo desaparece de la compilación nueva
# —un fragmento que se dividió distinto— y aquí quedara el viejo, el binario
# viajaría con código muerto que además podría servirse.
rm -rf rust/siger-api/estaticos
mkdir -p rust/siger-api/estaticos
# El `.gitkeep` está versionado para que la carpeta exista en un clon limpio; el
# `rm -rf` de arriba lo borraba, así que cada compilación dejaba el árbol sucio con
# un archivo eliminado. Se repone aquí, junto al vaciado que lo quita, y no en un
# «acuérdate de restaurarlo» en otro sitio.
git -C "$RAIZ" checkout -- rust/siger-api/estaticos/.gitkeep 2>/dev/null \
  || printf 'Aquí se empotra la interfaz compilada. Ver construir.sh\n' \
       > rust/siger-api/estaticos/.gitkeep
cp -r angular/dist/siger-web/browser/. rust/siger-api/estaticos/

echo "==> 3/4  binario (Rust)"
cd rust
cargo build --release

echo "==> 4/4  comprobar la interfaz que quedó DENTRO"
# Que la interfaz ESTÉ ya lo garantiza `build.rs`: sin `estaticos/index.html` la
# compilación de release de arriba ni siquiera termina. Lo que falta comprobar
# es otra cosa —que lo empotrado se pueda EJECUTAR bajo `script-src 'self'`, sin
# manejadores en línea ni hosts externos—, y de eso se ocupa `estaticos::pruebas`.
#
# Esa prueba **se salta sola cuando no hay interfaz empaquetada**, y hace bien:
# lo que no está no se puede comprobar. Pero la consecuencia es que en el CI se
# salta SIEMPRE —allí nadie ejecuta este guion y `estaticos/` solo tiene el
# `.gitkeep`—, así que su verde de allá no dice nada del artefacto. El único
# punto del mundo donde llega a medir algo es este, con la interfaz ya copiada.
#
# Se exigen las dos cosas por separado, porque cada una falla en silencio a su
# manera: que la prueba se EJECUTE (un filtro que no casa con nada sale 0 con
# «0 passed») y que NO se haya saltado. Verificado rompiéndola a propósito:
# un `onload=` inyectado en el index la pone roja; el filtro equivocado, y el
# target equivocado (`--lib`: este paquete no tiene biblioteca), la delatan.
SALIDA="$(cargo test --release -p siger-api --bin siger-api estaticos:: -- --nocapture 2>&1)" || {
  echo "$SALIDA"
  echo "FALLÓ la comprobación del index empotrado: el binario NO se entrega." >&2
  exit 1
}
echo "$SALIDA" | grep -E '^test (result|estaticos)' || true
if ! echo "$SALIDA" | grep -qE 'test result: ok\. [1-9][0-9]* passed'; then
  echo "$SALIDA"
  echo "La prueba del index empotrado no llegó a ejecutarse (filtro sin coincidencias)." >&2
  exit 1
fi
if echo "$SALIDA" | grep -q 'sin interfaz empaquetada'; then
  echo "La prueba se SALTÓ: la interfaz no quedó dentro del binario pese al paso 2." >&2
  exit 1
fi

BIN="rust/target/release/siger-api"
cd "$RAIZ"
echo
echo "Listo: $BIN  ($(du -h "$BIN" | cut -f1))"
echo
echo "Para probarlo como lo prueba el mundo (arranca y le habla por HTTP):"
echo "  ./rust/humo.sh"
echo
echo "Para arrancar:"
echo "  SIGER_SECRETO_SESION=\$($BIN generar-secreto) $BIN"
