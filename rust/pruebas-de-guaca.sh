#!/usr/bin/env bash
# Corre el banco de `guaca` EN EL `rev` QUE ESTE REPOSITORIO FIJA.
#
# POR QUÉ EXISTE, y es un agujero medido, no una precaución. El 2026-08-12 se
# podaron de aquí `dosfactores.rs` y `qr.rs` —945 + 381 líneas idénticas a las de
# guaca— y con ellas se fueron **17 pruebas y el oráculo dorado del QR**: la
# igualdad módulo a módulo contra libqrencode en las 40 versiones, la máscara por
# penalización, la regla de la ventana del TOTP a nivel unitario.
#
# Lo que quedó en informes mide el CABLEADO —que el enrolamiento devuelve el SVG
# de ESTE URI, que un código de fuera de la ventana no abre sesión— y eso está
# bien, es lo que este repositorio tiene que garantizar. Lo que ya no mide es la
# CORRECCIÓN del codificador. Y ahí está el peligro concreto: **un QR mal
# generado no falla**. Sale un dibujo plausible, con su `<svg>` y sus `<path>`,
# que ningún teléfono lee. Los dos `grep` estructurales de `humo.sh` lo darían
# por bueno y la pantalla se vería perfecta.
#
# O sea que un `rev` de guaca que rompiera el codificador entraría aquí con TODO
# en verde. La red existe —las 7 pruebas del QR y las 10 del TOTP— pero vive
# aguas arriba y `verde_local.py` no la ejecuta: corre los pasos del `ci.yml` de
# ESTE repositorio, y guaca es una dependencia `git`, no un miembro del
# workspace.
#
# LA REGLA, que va escrita en el CLAUDE.md: **mover el `rev` de guaca exige
# correr esto y decir el resultado**, igual que mergear exige decir el SHA de la
# revisión de seguridad (directiva 31). No es opcional y no lo sustituye que
# «compile y las 246 pasen»: eso ya pasaría con el codificador roto.
#
#   bash rust/pruebas-de-guaca.sh          # tarda ~1 min: compila guaca entero
#
# Lee el `rev` del `Cargo.lock` —lo que cargo construye de verdad, no un número
# escrito a mano que envejece— y corre el banco sobre ESE árbol, del espejo local.
set -euo pipefail

RAIZ="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ESPEJO=/home/juan/espejos/guaca.git
MANIFIESTO="$RAIZ/rust/siger-api/Cargo.toml"

# EL REV SE LEE DE LOS DOS SITIOS Y TIENEN QUE COINCIDIR. Ninguno vale solo, y
# las dos formas de equivocarse están medidas:
#
#   · solo el MANIFIESTO: un `grep` no distingue una línea viva de una COMENTADA,
#     y comentar el pin anterior encima del nuevo es un hábito corriente. Medido:
#     con el viejo comentado arriba, se elegía el viejo y se corría el banco del
#     árbol equivocado con salida 0.
#   · solo el LOCK: el lock dice qué resolvió cargo la ÚLTIMA vez, no qué va a
#     construir. Y está rancio exactamente en el instante en que esta regla
#     dispara —acabas de mover el pin y todavía no has construido—, así que mide
#     el árbol VIEJO y también sale 0. Medido moviendo el pin a `fb6065c` sin
#     dejar re-resolver: el guion dijo «fijado en 161a687», corrió y salió 0.
#
# O sea que el arreglo del primer agujero abrió el segundo. Exigir que los dos
# concuerden cierra los dos, y además convierte «el lock está sin actualizar» en
# un mensaje en vez de en un verde.
sin_comentarios() { grep -vE '^[[:space:]]*#' "$1"; }
REV_MAN="$(sin_comentarios "$MANIFIESTO" | grep -oE 'guaca = [{][^}]*rev = "[0-9a-f]{40}"' | grep -oE '[0-9a-f]{40}' || true)"
REV_LOCK="$(grep -A2 '^name = "guaca"$' "$RAIZ/rust/Cargo.lock" | grep -oE 'rev=[0-9a-f]{40}' | grep -oE '[0-9a-f]{40}' || true)"

for par in "manifiesto:$REV_MAN" "lock:$REV_LOCK"; do
    donde="${par%%:*}"; valor="${par#*:}"
    n="$(printf '%s' "$valor" | grep -c . || true)"
    if [ "$n" != 1 ]; then
        echo "esperaba UN rev de guaca en el $donde y encontré $n." >&2
        echo "no se puede saber cuál se construye; no se corre nada." >&2
        exit 2
    fi
done

if [ "$REV_MAN" != "$REV_LOCK" ]; then
    echo "el manifiesto y el Cargo.lock NO coinciden:" >&2
    echo "    manifiesto: $REV_MAN" >&2
    echo "    lock:       $REV_LOCK" >&2
    echo "El lock está sin actualizar, así que esto mediría el árbol VIEJO y" >&2
    echo "diría que todo bien. Corre \`cargo build\` (o \`cargo metadata\`) y repite." >&2
    exit 2
fi
REV="$REV_MAN"
echo "guaca fijado en: $REV"

if ! git -C "$ESPEJO" cat-file -e "$REV^{commit}" 2>/dev/null; then
    echo "ese rev no existe en $ESPEJO. El build se bajaría de un sitio que no es el espejo." >&2
    exit 2
fi

ARBOL="$(mktemp -d)"
trap 'rm -rf "$ARBOL"' EXIT
git -C "$ESPEJO" archive "$REV" | tar -x -C "$ARBOL"

echo "corriendo el banco de guaca sobre ese árbol (compila desde cero, tarda)…"
cd "$ARBOL"

# `--all-features` NO es exceso de celo: sin él este guion SE QUEDÓ CIEGO, y
# hizo falta contar para verlo.
#
# El 2026-08-12 el commit `c2bcddf` de guaca puso `dosfactores` y `qr` detrás de
# features —«lo que no se usa no entra en el árbol»—, y desde ese día
# `cargo test --all-targets` a secas deja de compilarlos. Medido sobre `f769b12`
# el 2026-08-14:
#
#     por defecto      71 pruebas
#     --all-features   88 pruebas      <- 17 que no corrían
#
# Y las 17 son EXACTAMENTE las de los dos módulos que informes usa. O sea que el
# guion escrito para validar el `rev` antes de adoptarlo había dejado de mirar
# justo lo que hay que mirar, sin dar un solo error: el banco seguía saliendo
# verde con menos pruebas.
#
# El motivo por el que esto importa aquí y no en otro sitio está en el CLAUDE.md
# de informes: **un QR mal generado no falla**. Sale un dibujo plausible que
# ningún teléfono lee, así que pasaría los dos `grep` estructurales de `humo.sh`
# y se vería perfecto en pantalla. Un `rev` que rompiera el codificador entraría
# con las pruebas de este repositorio en verde.
#
# Se usa `--all-features` y no la lista de features que declara el consumidor
# —que sería más preciso— porque es un SUPERCONJUNTO: no puede quedarse ciego
# cuando guaca añada la siguiente feature, y ese es el fallo que acabamos de
# pagar.
SALIDA="$(cargo test --all-targets --all-features 2>&1)" || { echo "$SALIDA"; exit 1; }
echo "$SALIDA"

# SUELO DE CONTEO, y no es celo: es el MISMO modo de fallo que este guion acaba
# de pagar, visto por el otro lado.
#
# `--all-features` protege de que una feature nueva vuelva a esconder pruebas.
# NO protege de que un `rev` futuro las BORRE o las RENOMBRE: ahí `cargo test`
# sale con 0 y el guion diría verde. «Cero pruebas» no es verde, es nada.
#
# `construir.sh` de este mismo repositorio ya sabía hacerlo —su línea 56 exige
# `[1-9][0-9]* passed` justo para que «0 passed; 7 ignored» no pase por bueno— y
# aquí faltaba. Lo señaló la revisión de seguridad del 2026-08-14.
#
# El número de referencia va en el mensaje, no en la comparación: fijar «88» aquí
# obligaría a editarlo cada vez que guaca gane una prueba, y una guarda que
# estorba es una que alguien quita.
if ! echo "$SALIDA" | grep -qE 'test result: ok\. [1-9][0-9]* passed'; then
    echo "" >&2
    echo "EL BANCO DE GUACA NO EJECUTÓ NI UNA PRUEBA." >&2
    echo "No es verde: es que no se midió nada. Sobre el rev $REV se esperaban" >&2
    echo "del orden de 88 (con --all-features). Mira la salida de arriba." >&2
    exit 2
fi
