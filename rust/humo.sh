#!/usr/bin/env bash
# Prueba de humo del ARTEFACTO: arranca el binario de release y le habla por
# HTTP como le habla el mundo.
#
#   ./rust/construir.sh && ./rust/humo.sh
#
# Por qué existe, si ya hay pruebas de las rutas: las de `cargo test` llaman al
# router de axum dentro del mismo proceso. Eso mide el código, no el producto.
# Aquí se mide lo que sale por la puerta y solo aparece cruzando la frontera:
#
#   · el binario ARRANCA con la configuración por entorno, y migra una base nueva;
#   · la interfaz empotrada se sirve de verdad, y el `.js` que el index pide
#     existe DENTRO del binario (no en el disco de quien lo compiló);
#   · las cabeceras de seguridad viajan en la respuesta real, no en un test;
#   · el freno del login cuenta en release, donde el Argon2 cuesta lo que cuesta.
#
# No toca nada del repositorio: base, archivos y puerto van a un directorio
# temporal que se borra al empezar. Sale 0 si todo cuadra, 1 en la primera
# discrepancia, con el número de la comprobación que falló.
set -uo pipefail

BIN="${BIN:-$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/target/release/siger-api}"
PUERTO="${PUERTO:-8199}"
SALA="$(mktemp -d)"
trap 'rm -rf "$SALA"' EXIT

[ -x "$BIN" ] || { echo "No existe el binario: $BIN — construya con ./rust/construir.sh" >&2; exit 1; }
mkdir -p "$SALA/entregas"

FALLOS=0
comprobar() {  # comprobar «qué se esperaba» «esperado» «obtenido»
  if [ "$2" = "$3" ]; then
    printf '  ok    %s\n' "$1"
  else
    printf '  FALLA %s — esperado «%s», obtenido «%s»\n' "$1" "$2" "$3"
    FALLOS=$((FALLOS + 1))
  fi
}

echo "==> arrancando $BIN en el puerto $PUERTO"
SECRETO="$("$BIN" generar-secreto)" || { echo "generar-secreto falló" >&2; exit 1; }

DATABASE_URL="sqlite://$SALA/humo.db?mode=rwc" \
SIGER_SECRETO_SESION="$SECRETO" \
PORT="$PUERTO" \
PAIS_ISO="${PAIS_ISO:-CO}" \
SIGER_DIR_ENTREGAS="$SALA/entregas" \
  "$BIN" > "$SALA/servidor.log" 2>&1 &
PID=$!
trap 'kill $PID 2>/dev/null; rm -rf "$SALA"' EXIT

U="http://127.0.0.1:$PUERTO"
for _ in $(seq 1 60); do
  kill -0 $PID 2>/dev/null || { echo "el binario MURIÓ al arrancar:" >&2; cat "$SALA/servidor.log" >&2; exit 1; }
  curl -fsS -o /dev/null "$U/api/salud" 2>/dev/null && break
  sleep 1
done

codigo() { curl -sS -o "${2:-/dev/null}" -w '%{http_code}' "${@:3}" "$U/$1"; }

echo "==> comprobaciones"
comprobar "el binario responde y ve su base"      '{"bd":"ok","estado":"sano"}' "$(curl -sS "$U/api/salud")"
comprobar "la raíz sirve la interfaz empotrada"   200 "$(codigo '' "$SALA/index.html")"

# El index no vale de nada si el JS que pide no viaja dentro. Es el fallo que no
# se ve compilando: binario nuevo, interfaz de la vez pasada — o ninguna.
JS="$(grep -o 'src="[^"]*\.js"' "$SALA/index.html" | head -1 | sed 's/src="//; s/"//')"
comprobar "el index pide un .js"                  0 "$([ -n "$JS" ] && echo 0 || echo 1)"
comprobar "y ese .js ($JS) está DENTRO"           200 "$(codigo "$JS")"

CAB="$(curl -sSI "$U/api/salud")"
for c in x-content-type-options x-frame-options content-security-policy referrer-policy; do
  comprobar "cabecera $c en la respuesta real"    0 "$(grep -qi "^$c:" <<<"$CAB" && echo 0 || echo 1)"
done

comprobar "sin sesión, /api/yo rechaza"           401 "$(codigo api/yo)"

# 401 y NO 404, y la diferencia es todo el valor de esta línea: un 404 diría que
# la ruta no viajó en el binario. El botón «cerrar sesión en todos los
# dispositivos» es lo único que sube `token_version`, así que si el extremo se
# cae del router la revocación vuelve a ser un mecanismo sin gatillo — y eso, sin
# esta comprobación, no lo delata nada: el resto de pruebas llaman al manejador.
comprobar "sin sesión, /api/salir-todo rechaza (401, no 404)" \
                                                  401 "$(codigo api/salir-todo /dev/null -X POST)"
comprobar "una ruta del SPA devuelve el index"    200 "$(codigo cronograma)"
comprobar "un /api/ inexistente es 404, no index" 404 "$(codigo api/no-existe)"

# El freno: cinco fallos sin castigo y el sexto frenado. Aquí, y no solo en
# `cargo test`, porque en release el Argon2 cuesta lo que cuesta y el señuelo
# tiene que costar lo mismo que un usuario real.
LOGIN=(-X POST -H 'Content-Type: application/json'
       -d '{"email":"nadie@example.com","clave":"claveMala123"}')
for n in 1 2 3 4 5; do
  comprobar "intento $n de login: rechazado sin frenar" 401 "$(codigo api/sesion /dev/null "${LOGIN[@]}")"
done
comprobar "el sexto intento SÍ frena"             429 "$(codigo api/sesion /dev/null "${LOGIN[@]}")"

echo
if [ "$FALLOS" -eq 0 ]; then
  echo "HUMO: todo en orden."
else
  echo "HUMO: $FALLOS comprobación(es) fallaron."
  echo "--- log del servidor ---"; tail -30 "$SALA/servidor.log"
fi
exit $([ "$FALLOS" -eq 0 ] && echo 0 || echo 1)
