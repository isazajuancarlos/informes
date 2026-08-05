#!/usr/bin/env bash
# Despliega la demostración pública en el VPS. Se ejecuta DESDE la máquina de
# desarrollo; hace ssh al servidor.
#
#   ./despliegue/desplegar-demo.sh debian@192.99.42.108
#
# Qué hace, en orden, y todo idempotente:
#   0. Comprueba que el servidor le deja entrar, ANTES de construir nada.
#   1. Construye el artefacto completo (interfaz + binario) con construir.sh.
#   2. Comprueba ANTES de subir que el binario lleva la interfaz dentro.
#   3. Copia binario, semilla y unidades a /opt/siger-demo.
#   4. Escribe /etc/siger-demo/siger.env con un secreto NUEVO si no existía.
#   5. Siembra, arranca y COMPRUEBA que responde.
#
# Lo que NO hace, a propósito, porque no es reversible desde aquí:
#   - crear el registro DNS de informes.xiliux.com (va en el registrador);
#   - pedir el certificado (`certbot --nginx -d informes.xiliux.com`), que exige
#     que el DNS ya resuelva;
#   - habilitar el sitio de nginx: deja el archivo en sites-available y lo dice.
set -euo pipefail

DESTINO="${1:-}"
if [ -z "$DESTINO" ]; then
    echo "Uso: $0 usuario@servidor      (por ejemplo: debian@192.99.42.108)" >&2
    exit 2
fi

RAIZ="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$RAIZ"
BIN=rust/target/release/siger-api
REMOTO=/opt/siger-demo

# El acceso se comprueba ANTES de construir. Construir tarda medio minuto y el
# `ssh` es el primer paso que puede fallar por algo ajeno al código: usuario
# equivocado, clave no autorizada todavía, servidor apagado. Descubrirlo después
# de compilar la interfaz y el binario no cambia el resultado, pero hace pensar
# que falló el despliegue cuando lo que falla es la puerta.
#
# NO se pone `BatchMode=yes`: con contraseña y una terminal de verdad, este mismo
# `ssh` la pide y sirve para abrir la sesión que reutilizan los pasos siguientes.
# Lanzado sin terminal (por ejemplo desde un agente) fallará aquí, con el aviso
# de abajo, que es exactamente lo que hace falta.
echo "==> 0/5  comprobar el acceso a $DESTINO"
if ! ssh -o ConnectTimeout=10 "$DESTINO" true; then
    cat >&2 <<AVISO

No se pudo entrar en «$DESTINO». Las causas, por frecuencia:

  · El usuario no es ese. En el VPS de OVH (imagen Debian) es \`debian\`, no
    \`usuario\` — que es el marcador del ejemplo, no un nombre real.
  · Su clave pública no está autorizada todavía. Se arregla una vez:
        ssh-copy-id -i ~/.ssh/id_ed25519.pub $DESTINO
  · Está lanzando esto sin terminal y el servidor pide contraseña. \`ssh\` no
    tiene dónde preguntarla: córralo en una terminal, o autorice la clave.

No se construyó nada. Vuelva a lanzarlo cuando el acceso funcione.
AVISO
    exit 3
fi

echo "==> 1/5  construir el artefacto"
./rust/construir.sh

echo "==> 2/5  comprobar que el binario lleva la interfaz"
# El `build.rs` ya rompe el release si falta `estaticos/index.html`, pero eso
# comprueba el ÁRBOL, no el binario que se va a subir. Aquí se mira el artefacto:
# es la diferencia entre «se compiló bien alguna vez» y «esto que estoy subiendo
# la trae».
if ! grep -aq "<!doctype html" "$BIN"; then
    echo "El binario NO lleva la interfaz empotrada. No se sube." >&2
    exit 1
fi

echo "==> 3/5  copiar a $DESTINO:$REMOTO"
ssh "$DESTINO" "sudo mkdir -p $REMOTO /etc/siger-demo && sudo chown -R \$(id -un):\$(id -gn) $REMOTO"
# El binario viaja con OTRO NOMBRE y se pone en su sitio abajo, con el servicio
# ya parado. Escribir encima del ejecutable que un proceso está corriendo lo
# rechaza el núcleo —ETXTBSY—, y `scp` lo cuenta como
# `dest open "…/siger-api": Failure`, que no se parece en nada a la causa.
#
# No se veía en el primer despliegue, porque entonces no había servicio que
# parar: apareció en el primero que REPITE (2026-08-02). Y dejaba el destino a
# medias — el `chown` de arriba ya había pasado, así que la base quedaba con el
# dueño equivocado y el servicio viejo sin poder escribirla.
#
# Parar el servicio antes de copiar también valdría, pero alarga la caída todo lo
# que tarde la subida. Así la ventana es el `mv`, que es un renombrado.
scp "$BIN" "$DESTINO:$REMOTO/siger-api.nuevo"
scp rust/sembrar_demo.py "$DESTINO:$REMOTO/"
scp despliegue/siger-demo.service despliegue/siger-demo-sembrar.service \
    despliegue/siger-demo-sembrar.timer "$DESTINO:/tmp/"
# El respaldo NO se instala aquí: la demostración se regenera cada noche, así que
# respaldarla sería guardar copias de datos ficticios. `siger-respaldo.service` y
# su timer son para una instalación de CLIENTE.
scp despliegue/informes.xiliux.com.nginx "$DESTINO:/tmp/"

echo "==> 4/5  configurar y arrancar"
ssh "$DESTINO" bash -s <<'REMOTO_FIN'
set -euo pipefail
REMOTO=/opt/siger-demo

# Usuario de servicio sin shell: la demo es pública.
id siger >/dev/null 2>&1 || sudo useradd --system --no-create-home --shell /usr/sbin/nologin siger

# El secreto se genera UNA vez y no se rota en cada despliegue: rotarlo cierra
# la sesión de quien esté mirando la demostración en ese momento.
if [ ! -f /etc/siger-demo/siger.env ]; then
    SECRETO="$($REMOTO/siger-api generar-secreto)"
    # La firma de entregas y el cifrado en reposo se activan EN LA DEMOSTRACIÓN,
    # no solo en una instalación de pago. Son dos de las tres cosas que
    # diferencian esto de una hoja de cálculo, y con las claves ausentes la
    # entrega recién subida sale con `tiene_firma: false` — es decir, enseñando
    # en pantalla lo contrario de lo que uno acaba de decir.
    CLAVES="$($REMOTO/siger-api generar-claves-firma)"
    REPOSO="$($REMOTO/siger-api generar-secreto)"
    sudo tee /etc/siger-demo/siger.env >/dev/null <<ENV
SIGER_SECRETO_SESION=$SECRETO
DATABASE_URL=sqlite://siger.db
PORT=8003
PAIS_ISO=CO
SIGER_DIR_ENTREGAS=storage/entregas
SIGER_MAX_MB=50
# La demo va detrás de nginx, que reescribe X-Real-IP con la dirección real
# (\`proxy_set_header X-Real-IP \$remote_addr\`). Sin esto, el freno del login
# vería a todo el mundo llegar desde 127.0.0.1 y contaría a los visitantes en la
# misma casilla.
SIGER_TRAS_PROXY=1
$CLAVES
SIGER_CLAVE_REPOSO=$REPOSO
ENV
    sudo chmod 600 /etc/siger-demo/siger.env
    sudo chown siger:siger /etc/siger-demo/siger.env
    echo "  secreto de sesión generado"
else
    echo "  /etc/siger-demo/siger.env ya existe: se conserva"
fi

# EL CARTEL DE LA DEMOSTRACIÓN, y va FUERA del `if` de arriba a propósito.
#
# Un visitante cae en la pantalla de acceso sin saber ninguna contraseña, y una
# demostración en la que nadie puede entrar no demuestra nada. `SIGER_DEMO_CUENTAS`
# es lo que esa pantalla anuncia — y en cualquier instalación que no sea ésta la
# variable no existe, así que no se anuncia nada.
#
# El valor lo imprime `sembrar_demo.py --anunciar`, o sea la MISMA fuente que
# siembra las cuentas. Si saliera de aquí, el día que cambie una contraseña en el
# guion de siembra el cartel seguiría diciendo la vieja: un cartel que miente es
# peor que no tener cartel, porque el visitante concluye que el producto no
# funciona y se va sin escribir.
#
# Se reescribe en CADA despliegue —de ahí que no esté dentro del `if` que conserva
# el archivo—: el secreto de sesión no se rota para no echar a quien esté mirando,
# pero el cartel tiene que seguir a la siembra.
ANUNCIO="$(python3 "$REMOTO/sembrar_demo.py" --anunciar)"
sudo sed -i '/^SIGER_DEMO_CUENTAS=/d' /etc/siger-demo/siger.env
echo "SIGER_DEMO_CUENTAS=$ANUNCIO" | sudo tee -a /etc/siger-demo/siger.env >/dev/null
echo "  cartel de la demostración: $(echo "$ANUNCIO" | tr ';' '\n' | wc -l) cuentas anunciadas"

sudo install -m 644 /tmp/siger-demo.service /tmp/siger-demo-sembrar.service \
     /tmp/siger-demo-sembrar.timer /etc/systemd/system/
sudo install -m 644 /tmp/informes.xiliux.com.nginx \
     /etc/nginx/sites-available/informes.xiliux.com

sudo systemctl stop siger-demo.service 2>/dev/null || true

# Y AHORA el binario nuevo ocupa su sitio, con el servicio ya parado. `mv` sobre
# un ejecutable en uso sí está permitido —renombrar no es escribir dentro—, pero
# se hace igualmente con el servicio abajo: si el `mv` fallara, lo que quedaría
# corriendo sería el binario viejo, entero, y no uno a medio escribir.
if [ -f "$REMOTO/siger-api.nuevo" ]; then
    sudo mv "$REMOTO/siger-api.nuevo" "$REMOTO/siger-api"
fi
sudo chown -R siger:siger "$REMOTO"
# …pero el binario y el guion de siembra vuelven a root. `siger` tiene que poder
# ESCRIBIR la base y el almacén, no el ejecutable que corre ni el guion que root
# lanza cada noche: ser dueño de lo que ejecutas es poder reescribirlo. No cierra
# ningún ataque conocido —no hay primitiva de escritura que lleve ahí— pero
# quitarlo cuesta dos líneas. Señalado en la revisión de seguridad del 2026-07-30.
sudo chown root:root "$REMOTO/siger-api" "$REMOTO/sembrar_demo.py"
sudo chmod 755 "$REMOTO/siger-api" "$REMOTO/sembrar_demo.py"
sudo -u siger python3 "$REMOTO/sembrar_demo.py" \
     --ruta "$REMOTO" --binario "$REMOTO/siger-api" --limpiar

sudo systemctl daemon-reload
sudo systemctl enable --now siger-demo.service
sudo systemctl enable --now siger-demo-sembrar.timer
REMOTO_FIN

echo "==> 5/5  comprobar que responde"
# Contra el puerto local del servidor, que es lo único que existe hasta que haya
# DNS y certificado. Comprobar https://informes.xiliux.com aquí daría un fallo
# que no dice nada sobre el despliegue.
ssh "$DESTINO" 'sleep 2; curl -sS -o /dev/null -w "http=%{http_code} bytes=%{size_download}\n" --max-time 10 http://127.0.0.1:8003/'

# ¿Está publicado ya? Se MIDE, no se supone.
#
# Hasta el 2026-08-04 lo de abajo se imprimía siempre, y decía dos cosas que
# llevaban horas siendo falsas: que nada era alcanzable desde fuera, y que
# faltaba crear el registro DNS. Mandaba al panel de OVH a crear un registro que
# ya existía, y —peor— ocultaba que la demostración ESTABA en internet, que es
# justo lo que hay que saber: lo que se publica anuncia sus credenciales.
#
# El sitio habilitado no basta como señal: puede estar y no resolver el nombre.
# La verdad es la respuesta por el dominio, que es lo que ve el mundo.
DOMINIO=informes.xiliux.com
# SE MIDE DESDE AQUÍ, NO POR SSH, y es la corrección del hallazgo de la revisión
# de este PR. La primera versión preguntaba con `ssh "$DESTINO" "curl …"`, o sea
# desde DENTRO del servidor — justo lo contrario de lo que dice el comentario de
# arriba—. El guion hermano ya lo hacía bien y explicaba por qué
# (`publicar-demo.sh`): «desde FUERA del servidor es lo único que prueba que el
# DNS, el certificado, el proxy y el binario funcionan a la vez». Un curl desde
# el propio VPS pasa con el sitio sin publicar, y encima añade el `ssh` como un
# modo de fallo más.
#
# El `|| true` no es descuido: con `set -e`, un `curl` que no conecta abortaría
# el guion DESPUÉS de haber desplegado bien, y el despliegue se leería como
# fallido. Pero el fallo del curl NO es el mismo dato que un código HTTP, y ésa
# era la otra mitad del defecto: `000` —lo que curl imprime cuando no hubo
# respuesta— caía en la rama que afirma «todavía no es alcanzable desde fuera».
# Eso es concluir sobre una medición que no ocurrió.
PUBLICADO=$(curl -s -o /dev/null -w '%{http_code}' --max-time 15 "https://$DOMINIO/" 2>/dev/null || true)
[ -n "$PUBLICADO" ] || PUBLICADO="000"

if [ "$PUBLICADO" = "200" ]; then
    cat <<FIN

Desplegado y PUBLICADO: https://$DOMINIO/ responde 200.

Es una demostración pública a propósito, así que **las credenciales que anuncia
son alcanzables desde internet**. Lo que se sirve ahí, medido como lo mide el
mundo y no con un \`is-active\`:

  curl -s https://$DOMINIO/api/demostracion
  curl -s -o /dev/null -w '%{http_code}\\n' https://$DOMINIO/
FIN
elif [ "$PUBLICADO" = "000" ]; then
    # TERCER ESTADO, y es la razón de que exista este `elif`: la medición no se
    # pudo hacer. `000` es lo que curl imprime cuando no hubo respuesta, y
    # también lo que queda si el propio curl falló. NO se dice «no es alcanzable»
    # —eso sería concluir sobre algo que no se midió, y es como la versión
    # anterior de este guion ocultó durante horas que la demostración estaba en
    # internet anunciando sus credenciales—. Se dice que no se pudo comprobar y
    # se da el comando para comprobarlo a mano.
    cat <<FIN

Desplegado. **NO se pudo comprobar si está publicado**: la petición a
https://$DOMINIO/ no obtuvo respuesta desde esta máquina.

Eso NO significa que no sea alcanzable — significa que no se midió. Puede ser la
red de aquí, el resolutor, o que de verdad no esté publicado. Compruébalo antes
de dar nada por hecho, porque si SÍ está publicado, sus credenciales son
alcanzables desde internet:

  curl -s -o /dev/null -w '%{http_code}\\n' --max-time 15 https://$DOMINIO/
  dig +short $DOMINIO

FIN
else
    cat <<FIN

Desplegado. https://$DOMINIO/ contestó «$PUBLICADO», no 200.

El nombre RESUELVE y algo contesta, así que el sitio existe: no es el caso de
«falta el DNS». Un 502 o un 503 justo después del reinicio suele ser la ventana
en la que el servicio todavía está levantando — vuelve a medir en unos segundos:

  curl -s -o /dev/null -w '%{http_code}\\n' --max-time 15 https://$DOMINIO/

Si persiste, puede faltar el registro DNS o el sitio de nginx:

  Registro DNS   A   $DOMINIO  ->  192.99.42.108     (panel de OVH)

Y cuando resuelva, publicar es un solo comando que además lo comprueba:

  ./despliegue/publicar-demo.sh $DESTINO

(Habilitar el sitio y pedir el certificado a mano NO funciona en el orden obvio:
el sitio trae \`listen 443 ssl\` sin certificado —\`nginx -t\` falla— y certbot
necesita que alguien conteste por ese nombre en el puerto 80, cosa que el sitio
por defecto de este servidor no hace: responde 444. \`publicar-demo.sh\` rompe ese
círculo con un sitio temporal de solo-HTTP.)
FIN
fi
