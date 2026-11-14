#!/usr/bin/env bash
# Construye el artefacto que recibe el cliente: UN archivo.
#
#   ./rust/construir.sh
#
# # Ya no hay paso de interfaz, y ese es el cambio (2026-08-06)
#
# Este guion tenía cuatro pasos y el primero era `npm ci && ng build`, porque la
# pantalla era un SPA de Angular que había que compilar y COPIAR dentro del
# binario antes de compilarlo. El orden era obligatorio y su inversión producía
# el fallo silencioso clásico: un binario con la interfaz de la vez anterior.
#
# Con el panel en `maud`, el HTML lo compila `rustc` junto al resto del programa
# —es código, no un artefacto que haya que fabricar aparte— y los cuatro activos
# que se sirven tal cual (htmx, `boi.js`, `comportamientos.js` y `panel.css`)
# viven versionados en `siger-api/estaticos/` y entran por `include_str!`.
#
# O sea que **el binario no puede quedar desincronizado de su interfaz**: si el
# activo cambia, `rustc` recompila; si falta, no compila. Desaparecen de un tirón
# el paso de npm, el de copia, `build.rs` —que existía para romper el release
# cuando `estaticos/` estaba vacío— y la comprobación del index empotrado, que
# vigilaba justo lo que ya no puede pasar.
set -euo pipefail

RAIZ="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$RAIZ"

echo "==> 1/2  binario (Rust), con el panel dentro"
cd rust
cargo build --release

echo "==> 2/2  la pantalla, DIBUJADA en un navegador"
# Que el HTML compile no dice si la pantalla se abre: los `hx-swap` los ejecuta
# JavaScript, y un fragmento correcto insertado en el sitio equivocado devuelve
# el mismo 200 con el mismo cuerpo. Por eso esto arranca el binario real y lo
# mira con Chromium, no con un `assert` sobre la cadena que devuelve axum.
#
# Se exigen las dos cosas por separado, porque cada una falla en silencio a su
# manera: que la prueba se EJECUTE —un filtro que no casa con nada sale 0 con
# «0 passed»— y que no se haya saltado por falta de navegador.
#
# `--include-ignored` NO es opcional desde el 2026-08-10: las pruebas de
# `pantalla.rs` llevan `#[ignore]` para que el lane de CI no las corra —el runner
# de GitHub trae Chrome, así que SÍ las corría, y un headless que no sale nunca
# colgó la corrida 49 minutos y ayudó a agotar el cupo—. O sea que ESTE guion es
# el único sitio donde el banco de navegador se ejecuta. Si alguien quita la
# bandera, el paso 2/2 sale verde sin dibujar nada… salvo por el control de
# abajo, que exige `[1-9]* passed` y por eso convierte «0 passed; 7 ignored» en
# rojo. Esa guarda es lo que hace que este comentario no sea la única defensa.
SALIDA="$(cargo test --release -p siger-api --test pantalla -- --include-ignored --nocapture 2>&1)" || {
  echo "$SALIDA"
  echo "FALLÓ el banco de navegador: el binario NO se entrega." >&2
  exit 1
}
echo "$SALIDA" | grep -E '^test (result|pantalla)' || true
if ! echo "$SALIDA" | grep -qE 'test result: ok\. [1-9][0-9]* passed'; then
  echo "$SALIDA"
  echo "El banco de navegador no llegó a ejecutarse (filtro sin coincidencias)." >&2
  exit 1
fi
if echo "$SALIDA" | grep -qi 'sin navegador'; then
  echo "El banco se SALTÓ: no hay Chromium y la pantalla no se dibujó." >&2
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
