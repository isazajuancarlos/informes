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
#   · el panel se sirve de verdad, y los activos que pide viajan dentro
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

# La cuenta con la que se prueba todo. Se declara aquí arriba porque su
# contraseña viaja también en `SIGER_DEMO_CUENTAS`: lo que se comprueba abajo es
# que lo ANUNCIADO en la pantalla de acceso sirve de verdad para entrar.
CLAVE_HUMO='clave-de-la-prueba-de-humo'

DATABASE_URL="sqlite://$SALA/humo.db?mode=rwc" \
SIGER_SECRETO_SESION="$SECRETO" \
PORT="$PUERTO" \
PAIS_ISO="${PAIS_ISO:-CO}" \
SIGER_DIR_ENTREGAS="$SALA/entregas" \
SIGER_CLAVE_REPOSO="frase-de-reposo-solo-para-esta-prueba-de-humo" \
SIGER_DEMO_CUENTAS="humo@example.com:$CLAVE_HUMO:Humo de prueba" \
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
# La raíz REDIRIGE al panel: ya no hay `index.html` de un SPA. Se exige el 303 y
# no un 200, porque un 200 aquí significaría que volvió el comodín.
comprobar "la raíz redirige al panel"             303 "$(codigo '')"
comprobar "y la pantalla de acceso se sirve"      200 "$(codigo entrar "$SALA/entrar.html")"

# Los tres activos que el panel pide, servidos DESDE DENTRO del binario. Es el
# fallo que no se ve compilando —una CSP `script-src 'self'` no admite un CDN, y
# un `include_str!` que apunte a un archivo vacío compila igual—, así que se
# comprueba que el HTML los pida Y que respondan.
for A in htmx.min.js boi.js panel.css; do
  comprobar "el panel pide $A"                    0 \
    "$(grep -q "$A" "$SALA/entrar.html" && echo 0 || echo 1)"
  comprobar "y $A viaja DENTRO"                   200 "$(codigo "$A")"
done

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

# La administración de cuentas, por la misma razón y con el mismo criterio: 401
# dice «existe y te rechaza», 404 diría que la ruta no viajó en el binario. Son
# los extremos que dan de alta gente y cambian contraseñas; que estén servidos de
# verdad no se puede dar por hecho desde una prueba que llama al manejador.
comprobar "sin sesión, /api/cuentas rechaza (401, no 404)" \
                                                  401 "$(codigo api/cuentas)"
comprobar "sin sesión, /api/mi-clave rechaza (401, no 404)" \
                                                  401 "$(codigo api/mi-clave /dev/null -X POST)"
# Y el rescate del segundo factor, que además es un DELETE: son los que se caen
# del router sin que nadie lo note, porque casi nada los pide. Si desapareciera,
# quien pierda el teléfono y los códigos se queda sin cuenta y el único arreglo
# vuelve a ser abrir la base a mano.
comprobar "sin sesión, DELETE /api/cuentas/1/2fa rechaza (401, no 404)" \
                                                  401 "$(codigo api/cuentas/1/2fa /dev/null -X DELETE)"

# El segundo factor (#212), con el mismo criterio. `verificar` es el extremo del
# SEGUNDO paso del login: si se cayera del router, quien tenga 2FA se quedaría
# con un token pendiente y ninguna puerta donde usarlo — y las pruebas de
# `cargo test` no lo verían, porque llaman al manejador.
comprobar "sin sesión, /api/2fa rechaza (401, no 404)" \
                                                  401 "$(codigo api/2fa)"
comprobar "sin token pendiente, /api/2fa/verificar rechaza (401, no 404)" \
                                                  401 "$(codigo api/2fa/verificar /dev/null -X POST \
                                                          -H 'Content-Type: application/json' -d '{"codigo":"000000"}')"
# Sin sesión, una pantalla del panel rebota a /entrar (303). Antes esto pedía un
# 200 con el index del SPA: el comodín devolvía la aplicación entera a cualquier
# URL y la sesión la decidía el navegador.
comprobar "una pantalla del panel sin sesión rebota" 303 "$(codigo cronograma)"
# Y lo que no existe es 404 de verdad, en el panel y en la API. Con el comodín,
# una errata de URL devolvía 200.
comprobar "un /api/ inexistente es 404"          404 "$(codigo api/no-existe)"
comprobar "una ruta inventada es 404, no el panel" 404 "$(codigo no-existe-esta-pagina)"

# El QR del enrolamiento, dibujado por el binario de release (#212). Es la única
# comprobación que cruza la frontera entera de esta característica: `cargo test`
# mide la matriz contra el oráculo dentro del proceso, y lo que la pantalla recibe
# es un SVG por HTTP. Hace falta una cuenta de verdad, así que se crea aquí con el
# MISMO `hashear` del producto —un hash de otra herramienta deja a alguien fuera— y
# se entra con ella.
HASH="$("$BIN" hashear "$CLAVE_HUMO")"
python3 - "$SALA/humo.db" "$HASH" <<'PY' 2>/dev/null
import sqlite3, sys
bd = sqlite3.connect(sys.argv[1])
bd.execute(
    "INSERT INTO usuarios (nombre, email, clave_hash, rol, activo) VALUES (?,?,?,?,1)",
    ("Humo", "humo@example.com", sys.argv[2], "ADMIN"),
)
bd.commit()
PY
GALLETAS="$SALA/galletas.txt"
comprobar "la cuenta de prueba entra" 200 \
  "$(codigo api/sesion /dev/null -X POST -c "$GALLETAS" -H 'Content-Type: application/json' \
      -d "{\"email\":\"humo@example.com\",\"clave\":\"$CLAVE_HUMO\"}")"

# El cartel de la demostración pública. Lo que importa no es que el extremo
# conteste, sino que **lo que anuncia ENTRE**: un cartel con una contraseña que
# no funciona es peor que no tener cartel, porque el visitante concluye que el
# producto está roto y se va sin escribir. Así que aquí no se comprueba contra la
# variable que se puso arriba —eso sería comprobar el códec—: se lee lo que el
# binario publica y se hace login con ESO.
curl -sS "$U/api/demostracion" -o "$SALA/demo.json"
LEE_DEMO='import json,sys; d=json.load(open(sys.argv[1])); print(d[0][sys.argv[2]] if d else "")'
D_CORREO="$(python3 -c "$LEE_DEMO" "$SALA/demo.json" correo 2>/dev/null)"
D_CLAVE="$(python3 -c "$LEE_DEMO" "$SALA/demo.json" clave 2>/dev/null)"
comprobar "la demostración anuncia una cuenta"    "humo@example.com" "$D_CORREO"
comprobar "y con lo anunciado SE ENTRA"           200 \
  "$(codigo api/sesion /dev/null -X POST -H 'Content-Type: application/json' \
      -d "{\"email\":\"$D_CORREO\",\"clave\":\"$D_CLAVE\"}")"
curl -sS -b "$GALLETAS" -X POST "$U/api/2fa/iniciar" -o "$SALA/enrolamiento.json"
QR="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["qr_svg"])' "$SALA/enrolamiento.json" 2>/dev/null)"
comprobar "el enrolamiento devuelve un SVG"  0 "$(grep -q '^<svg ' <<<"$QR" && echo 0 || echo 1)"
# Un SVG «bien formado» que no dibuje nada también empieza por `<svg`. Lo que dice
# que hay un código ahí es el trazo: sin él la pantalla enseña un cuadro blanco.
comprobar "y el SVG lleva su trazo de módulos" 0 "$(grep -q '<path fill="#000000" d="M' <<<"$QR" && echo 0 || echo 1)"

# Y se ACTIVA de verdad contra el binario de release, tecleando el código que
# generaría el teléfono. Es lo único que ejercita la cadena entera —cifrar el
# secreto con la frase de reposo, guardarlo, descifrarlo y validar la ventana—
# fuera del proceso de prueba.
CODIGO_TOTP="$(python3 - "$SALA/enrolamiento.json" <<'PY' 2>/dev/null
import base64, hashlib, hmac, json, struct, sys, time
s = json.load(open(sys.argv[1]))["secreto"]
clave = base64.b32decode(s + "=" * (-len(s) % 8))
h = hmac.new(clave, struct.pack(">Q", int(time.time()) // 30), hashlib.sha1).digest()
o = h[-1] & 15
print(f"{(struct.unpack('>I', h[o:o+4])[0] & 0x7FFFFFFF) % 1000000:06d}")
PY
)"
# Las dos mitades de la misma regla, y en este orden porque la primera no gasta
# el código: con la contraseña mala no se llega a tocar la fila, así que el mismo
# código sigue sirviendo para la segunda.
comprobar "sin la contraseña, el código bueno NO activa el segundo factor" 401 \
  "$(codigo api/2fa/activar /dev/null -X POST -b "$GALLETAS" \
      -H 'Content-Type: application/json' \
      -d "{\"codigo\":\"$CODIGO_TOTP\",\"clave\":\"la-que-no-es\"}")"

comprobar "el código del autenticador y la contraseña activan el segundo factor" 200 \
  "$(codigo api/2fa/activar /dev/null -X POST -b "$GALLETAS" \
      -H 'Content-Type: application/json' \
      -d "{\"codigo\":\"$CODIGO_TOTP\",\"clave\":\"$CLAVE_HUMO\"}")"

# Renovar los códigos exige la contraseña. Se comprueba el RECHAZO y no el
# acierto: el acierto ya lo miden las pruebas del módulo, y lo que aquí no se
# puede dar por hecho es que el extremo viajara dentro y con su guardia puesta —
# un 404 diría que no está, y un 200 diría que entrega diez credenciales a
# cualquiera con la sesión abierta.
comprobar "renovar códigos sin la contraseña buena rechaza (401, no 404)" 401 \
  "$(codigo api/2fa/codigos /dev/null -X POST -b "$GALLETAS" \
      -H 'Content-Type: application/json' -d '{"clave":"la-que-no-es"}')"

# Con el factor puesto, la contraseña sola ya no abre: 200 con `segundo_factor`
# en true y SIN cookie de sesión. Es la afirmación central de #212, y aquí se
# mide en la respuesta real y no en el manejador.
SEGUNDO="$(curl -sS -D "$SALA/cab-2fa.txt" -o "$SALA/login-2fa.json" \
    -c "$SALA/pendiente.txt" -X POST "$U/api/sesion" \
    -H 'Content-Type: application/json' \
    -d "{\"email\":\"humo@example.com\",\"clave\":\"$CLAVE_HUMO\"}" \
    -w '%{http_code}')"
comprobar "con 2FA, la contraseña responde 200…"       200 "$SEGUNDO"
comprobar "…diciendo que falta el segundo factor"      0 \
  "$(grep -q '"segundo_factor":true' "$SALA/login-2fa.json" && echo 0 || echo 1)"
comprobar "…y SIN entregar cookie de sesión"           1 \
  "$(grep -qi '^set-cookie: siger_sesion=' "$SALA/cab-2fa.txt" && echo 0 || echo 1)"
comprobar "…sino un token pendiente en su propia cookie" 0 \
  "$(grep -q 'siger_pendiente' "$SALA/pendiente.txt" && echo 0 || echo 1)"

# Y el segundo paso SE COMPLETA. Lo de arriba mide que la puerta se cerró; sin
# esto, un `verificar` que rechazara siempre pasaría la prueba de humo entera y
# nadie con 2FA podría entrar al producto instalado. `cargo test` lo ve dentro
# del proceso; aquí se teclea el código contra el binario de release.
#
# El código es el de la ventana SIGUIENTE a propósito: el de la actual se acaba
# de gastar activando, y el contador guardado (el antirreplay) lo rechazaría. La
# tolerancia de una ventana lo admite, que es justo lo que le pasa a un teléfono
# adelantado unos segundos.
CODIGO_SIGUIENTE="$(python3 - "$SALA/enrolamiento.json" <<'PY' 2>/dev/null
import base64, hashlib, hmac, json, struct, sys, time
s = json.load(open(sys.argv[1]))["secreto"]
clave = base64.b32decode(s + "=" * (-len(s) % 8))
h = hmac.new(clave, struct.pack(">Q", int(time.time()) // 30 + 1), hashlib.sha1).digest()
o = h[-1] & 15
print(f"{(struct.unpack('>I', h[o:o+4])[0] & 0x7FFFFFFF) % 1000000:06d}")
PY
)"
ENTRA="$(curl -sS -D "$SALA/cab-verificar.txt" -o /dev/null \
    -b "$SALA/pendiente.txt" -c "$SALA/galletas-2fa.txt" -X POST "$U/api/2fa/verificar" \
    -H 'Content-Type: application/json' -d "{\"codigo\":\"$CODIGO_SIGUIENTE\"}" \
    -w '%{http_code}')"
comprobar "con el código, el segundo paso abre…"       200 "$ENTRA"
comprobar "…entregando ahora sí la cookie de sesión"   0 \
  "$(grep -qi '^set-cookie: siger_sesion=' "$SALA/cab-verificar.txt" && echo 0 || echo 1)"
comprobar "…y con ella se entra a la aplicación"       200 \
  "$(codigo api/yo /dev/null -b "$SALA/galletas-2fa.txt")"
# La otra mitad de la separación por dominio de firma: el token pendiente no es
# una sesión. Si lo fuera, la contraseña sola habría bastado y el segundo factor
# sería decorativo.
comprobar "el token pendiente NO vale como sesión"     401 \
  "$(codigo api/yo /dev/null -b "$SALA/pendiente.txt")"
# El freno: cinco fallos sin castigo y el sexto frenado. Aquí, y no solo en
# `cargo test`, porque en release el Argon2 cuesta lo que cuesta y el señuelo
# tiene que costar lo mismo que un usuario real.
LOGIN=(-X POST -H 'Content-Type: application/json'
       -d '{"email":"nadie@example.com","clave":"claveMala123"}')
for n in 1 2 3 4 5; do
  comprobar "intento $n de login: rechazado sin frenar" 401 "$(codigo api/sesion /dev/null "${LOGIN[@]}")"
done
comprobar "el sexto intento SÍ frena"             429 "$(codigo api/sesion /dev/null "${LOGIN[@]}")"

# La bitácora, leída del ARCHIVO que dejó el binario de release (#186). Todo lo
# demás la comprueba contra una base `:memory:` dentro del propio proceso de
# prueba: eso mide el SQL, no que el producto instalado escriba en su disco. Los
# seis intentos de arriba tienen que haber dejado cinco `acceso_fallido` y un
# `acceso_frenado` — y si no hay python3 esto FALLA, no se salta en silencio.
CUENTA="$(python3 - "$SALA/humo.db" <<'PY' 2>/dev/null || echo "sin-python3"
import sqlite3, sys
bd = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro", uri=True)
filas = dict(bd.execute("SELECT evento, COUNT(*) FROM auditoria GROUP BY evento"))
print(f"{filas.get('acceso_fallido', 0)}/{filas.get('acceso_frenado', 0)}")
PY
)"
comprobar "la bitácora del binario real anotó los rechazos" "5/1" "$CUENTA"

echo
if [ "$FALLOS" -eq 0 ]; then
  echo "HUMO: todo en orden."
else
  echo "HUMO: $FALLOS comprobación(es) fallaron."
  echo "--- log del servidor ---"; tail -30 "$SALA/servidor.log"
fi
exit $([ "$FALLOS" -eq 0 ] && echo 0 || echo 1)
