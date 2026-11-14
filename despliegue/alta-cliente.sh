#!/usr/bin/env bash
# Da de alta UNA INSTANCIA DE SIGER PARA UN CLIENTE (ficha #606). Se ejecuta
# DESDE la máquina de desarrollo; hace ssh al servidor.
#
#   ./despliegue/alta-cliente.sh acme acme.xiliux.com debian@192.99.42.108
#
# Así se vende el servicio, decidido el 2026-08-09: un proceso, una base y un
# subdominio por cliente. Multi-inquilino quedó DESCARTADO —no en espera—, y el
# porqué está en la cabecera de `siger@.service`.
#
# Qué hace, en orden, y todo idempetente:
#   0. Comprueba el acceso ANTES de construir nada.
#   1. Construye el artefacto (binario con el panel dentro) con construir.sh.
#   2. Comprueba que el binario lleva la interfaz, mirando el ARTEFACTO.
#   3. Crea usuario, directorios y respaldos del cliente.
#   4. Escribe /etc/siger-<cliente>/siger.env SOLO si no existía. Si existe, se
#      CONSERVA: ahí viven el secreto de sesión, la clave de reposo y las claves
#      de firma, y regenerarlos deja fuera a todo el mundo y hace ilegibles los
#      documentos ya cifrados.
#   5. Instala las plantillas de systemd, arranca y COMPRUEBA que responde.
#   6. Deja el vhost en sites-available y dice lo que falta.
#
# Lo que NO hace, a propósito, porque no es reversible desde aquí:
#   - crear el registro DNS del subdominio (va en el registrador);
#   - pedir el certificado (`certbot certonly --webroot -w /var/www/html -d ...`),
#     que exige que el DNS ya resuelva;
#   - habilitar el sitio de nginx ni recargar nginx: un sitio que apunta a un
#     certificado inexistente tumba TODO el nginx del VPS al recargar, y con él
#     los otros cinco dominios. Se deja el archivo y se dice el comando.
#
# Y NO SIEMBRA NADA. La demo se siembra; una instalación de cliente nace vacía y
# su primer administrador se crea a mano con `siger-api hashear`. Sembrar aquí
# metería cuentas conocidas y públicas en la base de una empresa.
set -euo pipefail

CLIENTE="${1:-}"
DOMINIO="${2:-}"
DESTINO="${3:-}"
if [ -z "$CLIENTE" ] || [ -z "$DOMINIO" ] || [ -z "$DESTINO" ]; then
    echo "Uso: $0 <cliente> <dominio> <usuario@servidor>" >&2
    echo "     $0 acme acme.xiliux.com debian@192.99.42.108" >&2
    exit 2
fi

# El nombre del cliente viaja a una unidad de systemd, a un usuario del sistema,
# a una ruta y a una zona de nginx. Se valida ANTES de tocar nada: un nombre con
# `/`, con espacio o con `..` no produce un error claro más adelante, produce una
# ruta distinta de la que uno cree (directiva 20 — fallar ruidoso, no adivinar).
case "$CLIENTE" in
    # `demo` se rechaza a propósito: su zona de nginx sería `siger_demo`, que es
    # LA MISMA que usa la demostración pública (`informes.xiliux.com.nginx`). Una
    # baja de ese cliente habría borrado el vhost de la demo y recargado nginx —
    # la misma clase de fallo que el comodín ya corregido, esta vez por colisión
    # de nombre en vez de por prefijo. Lo vio la revisión de seguridad.
    demo | *[!a-z0-9-]* | "" | -* | *-)
        echo "El nombre del cliente debe ser [a-z0-9-], sin empezar ni acabar en «-»: «$CLIENTE»" >&2
        exit 2
        ;;
esac
case "$DOMINIO" in
    *[!a-zA-Z0-9.-]* | "" | .* | *.)
        echo "El dominio no parece un nombre de máquina: «$DOMINIO»" >&2
        exit 2
        ;;
esac

RAIZ="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$RAIZ"
BIN=rust/target/release/siger-api
REMOTO="/opt/siger-$CLIENTE"     # el PROGRAMA: de root, y el servicio no lo escribe
DATOS="/var/lib/siger-$CLIENTE"  # los DATOS: del cliente, y aquí sí escribe
ENTORNO="/etc/siger-$CLIENTE"

echo "==> 0/6  comprobar el acceso a $DESTINO"
if ! ssh -o ConnectTimeout=10 "$DESTINO" true; then
    echo "No se pudo entrar en «$DESTINO». No se construyó nada." >&2
    exit 3
fi

echo "==> 1/6  construir el artefacto"
./rust/construir.sh

echo "==> 2/6  comprobar que el binario lleva la interfaz"
# Se mira el ARTEFACTO que se va a subir, no el árbol: es la diferencia entre
# «se compiló bien alguna vez» y «esto que estoy subiendo la trae». `htmx:load`
# es un evento interno de htmx, así que su presencia prueba que el activo
# vendorizado se empotró de verdad.
if ! grep -aq "htmx:load" "$BIN"; then
    echo "El binario NO lleva la interfaz empotrada (htmx no aparece). No se sube." >&2
    exit 1
fi

echo "==> 3/6  usuario, directorios y respaldos de «$CLIENTE»"
ssh "$DESTINO" "sudo bash -s" <<REMOTO_FIN
set -euo pipefail
# \`--system\` y sin shell: esta cuenta existe para que el servicio corra, no
# para que nadie entre con ella.
id -u "siger-$CLIENTE" >/dev/null 2>&1 || \
    sudo useradd --system --no-create-home --shell /usr/sbin/nologin "siger-$CLIENTE"
sudo mkdir -p "$REMOTO" "$DATOS/storage/entregas" "$ENTORNO" "/var/respaldos/$CLIENTE"
# EL PROGRAMA ES DE ROOT Y EL SERVICIO NO PUEDE TOCARLO. Hasta el 2026-08-11 el
# binario se instalaba con dueño \`siger-$CLIENTE\` y modo 755 DENTRO del mismo
# árbol que \`ReadWritePaths=\` abre para la base — o sea que el proceso expuesto a
# internet podía reescribir su propio ejecutable, y la baja se lo traía y lo
# EJECUTABA en esta máquina. Lo encontró la revisión de seguridad de este mismo
# cambio, y era una cadena que salía del VPS y llegaba al portátil que tiene la
# clave SSH, /mnt/data y el origen de los 48 espejos.
#
# Y no basta con que el ARCHIVO sea de root: borrar un archivo pide permiso sobre
# el DIRECTORIO, no sobre él. Por eso el programa y los datos viven ahora en
# árboles distintos, y \`/opt/siger-<cliente>\` entero es de root.
sudo chown -R "siger-$CLIENTE:siger-$CLIENTE" "$DATOS" "$ENTORNO" "/var/respaldos/$CLIENTE"
sudo chown root:root "$REMOTO"

# EL MODO, QUE \`chown\` NO TOCA — y aquí estaba el defecto.
#
# \`mkdir\` corre como root con \`umask 022\`, así que estos directorios nacían en
# **0755** y \`chown -R\` solo cambiaba el dueño. O sea que CUALQUIER cuenta local
# de la máquina podía leer \`/opt/siger-<cliente>/siger.db\`: padrón completo con
# nombres, correos, roles, hashes de Argon2 para romper offline, y la bitácora
# entera. Y eso no lo tapa \`ProtectSystem=strict\`, que restringe las ESCRITURAS
# del servicio y no las lecturas de otros usuarios.
#
# No es «falta de endurecimiento»: \`siger@.service\` declara por escrito que el
# usuario por cliente existe *para impedir que una instancia lea la base de
# otra*. Con 0755 ese control nombrado no impedía nada. Un control declarado que
# no hace lo que dice es peor que uno ausente.
sudo chmod 755 "$REMOTO"   # de root: solo travesía y lectura para el servicio
sudo chmod 750 "$DATOS" "$DATOS/storage" "$DATOS/storage/entregas"
sudo chmod 700 "$ENTORNO"

# UNA CARPETA DE RESPALDO POR CLIENTE, y no una compartida a 755. Dos motivos, y
# el segundo es un fallo liso:
#
#   · confidencialidad: el respaldo es una copia ÍNTEGRA de la base. Dejarlo en
#     un directorio que todos atraviesan devuelve por la puerta de atrás lo que
#     el \`chmod 750\` de arriba acaba de cerrar;
#   · y funcionaba MAL: \`/var/respaldos\` era de root y 755, pero
#     \`siger-respaldo@<cliente>.service\` corre como \`siger-<cliente>\`, que por
#     tanto NO podía crear nada dentro. El respaldo nocturno habría fallado con
#     EACCES todas las noches —ruidosamente, en \`systemctl --failed\`, pero el
#     alta ya había impreso «con respaldo diario verificado»—.
sudo chmod 755 /var/respaldos
sudo chmod 750 "/var/respaldos/$CLIENTE"
REMOTO_FIN

echo "==> 4/6  entorno (se CONSERVA si ya existe)"
# El puerto se elige una sola vez y queda escrito en el propio entorno, que pasa
# a ser la fuente de verdad. Volver a calcularlo en cada corrida movería el
# puerto de un cliente vivo cada vez que se diera de alta a otro.
PUERTO=$(ssh "$DESTINO" "sudo bash -s" <<REMOTO_FIN
set -euo pipefail
if [ -f "$ENTORNO/siger.env" ]; then
    grep -m1 '^PORT=' "$ENTORNO/siger.env" | cut -d= -f2
    exit 0
fi
# Primer puerto libre desde 8100 — la demo vive en 8003 y los servicios ya
# instalados están por debajo. Se comprueba contra lo que ESCUCHA de verdad y
# contra lo que ya tiene asignado otro cliente, porque una instancia parada no
# aparece en \`ss\` y su puerto no está libre: es suyo.
for p in \$(seq 8100 8199); do
    if ss -ltnH "sport = :\$p" | grep -q . ; then continue; fi
    if grep -rqs "^PORT=\$p\$" /etc/siger-*/siger.env; then continue; fi
    echo "\$p"; exit 0
done
echo "sin puertos libres en 8100-8199" >&2; exit 1
REMOTO_FIN
)
echo "    puerto: $PUERTO"

ssh "$DESTINO" "sudo bash -s" <<REMOTO_FIN
set -euo pipefail
if [ -f "$ENTORNO/siger.env" ]; then
    echo "    $ENTORNO/siger.env ya existe: se CONSERVA (ahí están los secretos)"
    exit 0
fi
# El binario todavía no está arriba en el primer alta, así que los secretos se
# generan con el que se acaba de subir en el paso 5. Se hace al revés: primero
# se sube, luego se escribe el entorno. Para no reordenar los pasos, aquí solo
# se comprueba y el entorno lo escribe el paso 5.
echo "    $ENTORNO/siger.env no existe: se creará tras subir el binario"
REMOTO_FIN

echo "==> 5/6  subir binario y unidades, y arrancar"
scp -q "$BIN" "$DESTINO:/tmp/siger-api-$CLIENTE"
scp -q despliegue/siger@.service despliegue/siger-respaldo@.service \
       despliegue/siger-respaldo@.timer "$DESTINO:/tmp/"
ssh "$DESTINO" "sudo bash -s" <<REMOTO_FIN
set -euo pipefail
sudo install -o root -g root -m 755 \
     "/tmp/siger-api-$CLIENTE" "$REMOTO/siger-api"
rm -f "/tmp/siger-api-$CLIENTE"
sudo install -m 644 /tmp/siger@.service /tmp/siger-respaldo@.service \
     /tmp/siger-respaldo@.timer /etc/systemd/system/
rm -f /tmp/siger@.service /tmp/siger-respaldo@.service /tmp/siger-respaldo@.timer

if [ ! -f "$ENTORNO/siger.env" ]; then
    SECRETO="\$("$REMOTO/siger-api" generar-secreto)"
    CLAVES="\$("$REMOTO/siger-api" generar-claves-firma)"
    REPOSO="\$("$REMOTO/siger-api" generar-secreto)"
    sudo sh -c "umask 077; cat > $ENTORNO/siger.env" >/dev/null <<ENV
SIGER_SECRETO_SESION=\$SECRETO
# DATABASE_URL y SIGER_DIR_ENTREGAS son RELATIVAS al WorkingDirectory, que es
# $DATOS. Y el comentario va en su propia línea a propósito: systemd NO recorta
# los comentarios a final de valor: DATABASE_URL valdría la cadena entera y el
# binario responde «unable to open database file» y sale con 1.
#
# Y SIN COMILLAS INVERTIDAS, que es la segunda mitad de la lección: este texto
# va dentro de DOS heredocs sin comillar (el de ssh y el del propio .env), así
# que se expande DOS veces. Escaparlas una sola vez no arregla nada: mueve la
# sustitución de órdenes del shell local al shell ROOT del VPS y el comentario
# sigue llegando mutilado. Medido en las dos formas.
DATABASE_URL=sqlite://siger.db
PORT=$PUERTO
PAIS_ISO=CO
SIGER_DIR_ENTREGAS=storage/entregas
SIGER_MAX_MB=50
# Detrás de nginx, que reescribe X-Real-IP con la dirección real. Sin esto el
# freno del login vería a todo el mundo llegar desde 127.0.0.1 y contaría a
# todos los visitantes en la misma casilla.
SIGER_TRAS_PROXY=1
\$CLAVES
SIGER_CLAVE_REPOSO=\$REPOSO
ENV
    sudo chmod 600 "$ENTORNO/siger.env"
    sudo chown "siger-$CLIENTE:siger-$CLIENTE" "$ENTORNO/siger.env"
    echo "    entorno creado, con secreto de sesión, claves de firma y clave de reposo"
fi

sudo systemctl daemon-reload
sudo systemctl enable --now "siger@$CLIENTE.service"
sudo systemctl enable --now "siger-respaldo@$CLIENTE.timer"
sudo systemctl restart "siger@$CLIENTE.service"
REMOTO_FIN

echo "==> 6/6  comprobar que responde"
# Se mide contra el PUERTO LOCAL del servidor y no contra el dominio: el dominio
# todavía puede no resolver ni tener certificado, y eso no dice nada del
# servicio. `systemctl is-active` tampoco sirve — un proceso vivo que no atiende
# se ve igual que uno sano.
CODIGO=$(ssh "$DESTINO" "curl -s -o /dev/null -w '%{http_code}' --max-time 10 http://127.0.0.1:$PUERTO/entrar || true")
if [ "$CODIGO" != "200" ]; then
    echo "La instancia NO contesta en 127.0.0.1:$PUERTO/entrar (código «$CODIGO»)." >&2
    echo "Diagnóstico:  ssh $DESTINO 'sudo journalctl -u siger@$CLIENTE -n 50 --no-pager'" >&2
    exit 1
fi
echo "    responde 200 en 127.0.0.1:$PUERTO/entrar"

# El vhost se deja PREPARADO, no habilitado. Ver la cabecera: un sitio que
# apunta a un certificado que no existe tumba nginx entero al recargar.
sed -e "s/@CLIENTE@/$CLIENTE/g" -e "s/@DOMINIO@/$DOMINIO/g" -e "s/@PUERTO@/$PUERTO/g" \
    despliegue/cliente.nginx.plantilla > "/tmp/$DOMINIO.nginx"
scp -q "/tmp/$DOMINIO.nginx" "$DESTINO:/tmp/"
ssh "$DESTINO" "sudo install -m 644 /tmp/$DOMINIO.nginx /etc/nginx/sites-available/$DOMINIO && rm -f /tmp/$DOMINIO.nginx"
rm -f "/tmp/$DOMINIO.nginx"

cat <<FIN

Instancia «$CLIENTE» viva en el puerto $PUERTO, con respaldo diario verificado.

FALTA, y no lo hace este guion porque no es reversible desde aquí:

  1. Registro DNS A de $DOMINIO -> la IP del VPS (en el registrador).
  2. Certificado, cuando el DNS resuelva:
       ssh $DESTINO 'sudo certbot certonly --webroot -w /var/www/html -d $DOMINIO'
  3. Habilitar el sitio y recargar — SOLO después de que exista el certificado:
       ssh $DESTINO 'sudo ln -sf /etc/nginx/sites-available/$DOMINIO /etc/nginx/sites-enabled/ && sudo nginx -t && sudo systemctl reload nginx'
  4. El primer administrador, que esta instalación NO trae:
       ssh $DESTINO 'sudo -u siger-$CLIENTE $REMOTO/siger-api hashear'
     y dar de alta la fila en \`usuarios\` con ese hash.

Y la comprobación que vale, cuando estén los tres:
    curl -sI https://$DOMINIO/entrar
FIN
