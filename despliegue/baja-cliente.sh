#!/usr/bin/env bash
# Da de BAJA la instancia de un cliente, entregándole antes SUS datos (#606).
#
#   ./despliegue/baja-cliente.sh acme debian@192.99.42.108 ~/entrega-acme
#   ./despliegue/baja-cliente.sh acme debian@192.99.42.108 ~/entrega-acme --borrar
#
# ESTO ES ARGUMENTO DE VENTA, no mantenimiento. A un contratista que le reporta
# a un ente de control le importa poder irse con lo suyo: su binario, su base y
# sus documentos, en un formato que abre sin nosotros —SQLite es un archivo—.
# Es exactamente lo que un sistema multi-inquilino no puede ofrecer, porque los
# datos de todos viven en las mismas tablas.
#
# SIN `--borrar` NO BORRA NADA: se lleva la copia y deja la instancia en pie.
# Ése es el modo por defecto a propósito. El borrado es lo único irreversible de
# todo el despliegue, y separarlo en dos pasadas permite comprobar la entrega
# —abrirla, contarla, enseñársela al cliente— antes de que deje de existir el
# original.
set -euo pipefail

CLIENTE="${1:-}"
DESTINO="${2:-}"
ENTREGA="${3:-}"
BORRAR="${4:-}"
if [ -z "$CLIENTE" ] || [ -z "$DESTINO" ] || [ -z "$ENTREGA" ]; then
    echo "Uso: $0 <cliente> <usuario@servidor> <carpeta-de-entrega> [--borrar]" >&2
    exit 2
fi
case "$CLIENTE" in
    # `demo` se rechaza a propósito: su zona de nginx sería `siger_demo`, que es
    # LA MISMA que usa la demostración pública (`informes.xiliux.com.nginx`). Una
    # baja de ese cliente habría borrado el vhost de la demo y recargado nginx —
    # la misma clase de fallo que el comodín ya corregido, esta vez por colisión
    # de nombre en vez de por prefijo. Lo vio la revisión de seguridad.
    demo | *[!a-z0-9-]* | "" | -* | *-)
        echo "Nombre de cliente inaceptable: «$CLIENTE»" >&2; exit 2 ;;
esac

REMOTO="/opt/siger-$CLIENTE"     # el programa
DATOS="/var/lib/siger-$CLIENTE"  # los datos

echo "==> 1/4  respaldo final, verificado por el propio binario"
# No se copia `siger.db` en caliente con `scp`: un SQLite copiado mientras se
# escribe puede llegar íntegro y ser inservible. `respaldar` usa `VACUUM INTO`
# bajo transacción de lectura y VERIFICA la copia antes de darla por buena — si
# no cuadra, sale distinto de cero y este guion se para aquí, que es donde tiene
# que pararse.
MARCA="entrega-$(date +%F-%H%M%S)"
# EL ENTORNO NO PASA POR `argv`, y esta línea ya estuvo mal escrita una vez.
#
# La primera versión era `env $(sudo cat …/siger.env | xargs) siger-api …`. Como
# el `$(…)` iba escapado, lo expandía el shell REMOTO, y el resultado es que
# `SIGER_SECRETO_SESION`, `SIGER_CLAVE_REPOSO` y `QUIPU_SIGNING_KEY` acababan en
# la línea de órdenes de `sudo` y de `env` — o sea en `/proc/<pid>/cmdline`, que
# en Debian es legible por cualquier usuario de la máquina. El archivo está a
# 0600 a propósito; copiarlo a `argv` deshacía ese 0600 durante la ejecución. Y
# con el secreto de sesión se forja una cookie `id|version|caducidad|firma` de
# administrador sin saber ninguna contraseña.
#
# `systemd-run` lo lee él mismo, como root, y se lo entrega al proceso por el
# entorno: es el MISMO mecanismo (`EnvironmentFile=`) que ya usan las tres
# unidades del repositorio. `--wait` propaga el código de salida, que es lo que
# hace que un respaldo que no cuadra pare este guion.
ssh "$DESTINO" "sudo systemd-run --wait --collect --quiet \
    --unit=siger-entrega-$CLIENTE-$MARCA \
    --uid=siger-$CLIENTE --gid=siger-$CLIENTE \
    --working-directory=$DATOS \
    --property=EnvironmentFile=/etc/siger-$CLIENTE/siger.env \
    --property=UMask=0077 \
    $REMOTO/siger-api respaldar /var/respaldos/$CLIENTE/$MARCA"

echo "==> 2/4  traer la entrega a $ENTREGA"
mkdir -p "$ENTREGA"
# El binario va con los datos: sin él, «sus datos son suyos» obliga a pedirnos
# el programa. Con él, el cliente arranca su propio SIGER el día que quiera.
ssh "$DESTINO" "sudo tar -C /var/respaldos/$CLIENTE -cf - $MARCA" | tar -C "$ENTREGA" -xf -
ssh "$DESTINO" "sudo cat $REMOTO/siger-api" > "$ENTREGA/siger-api"
# Sin bit de ejecución: este binario se ENTREGA, no se corre aquí. Se lo pone
# el cliente en su máquina.
chmod 644 "$ENTREGA/siger-api"

echo "==> 3/4  comprobar la entrega ANTES de tocar nada en el servidor"
COPIA="$ENTREGA/$MARCA"
[ -d "$COPIA" ] || { echo "No llegó la carpeta de respaldo: $COPIA" >&2; exit 1; }
# Que el archivo exista no dice que sirva. Se comprueba el ARTEFACTO — y **NO
# ejecutándolo**, que es lo que hacía hasta el 2026-08-11 y era el último eslabón
# de una cadena que la revisión de seguridad encontró: cualquiera que lograra
# ejecutar código como `siger-<cliente>` en el VPS podía reescribir el binario, y
# la baja se lo traía y lo corría AQUÍ, en la máquina que tiene la clave SSH del
# servidor, /mnt/data y el origen de los 48 espejos. El momento lo elegía él, más
# o menos, porque la baja la pide el cliente.
#
# Se compara contra el binario que ACABAMOS de construir, que está en esta misma
# máquina. Eso comprueba algo más fuerte que ejecutarlo: que es EL NUESTRO, bit a
# bit. Correrlo nunca demostró eso.
#
# (El `--version` que había además no existe: `main.rs` no lo conoce, así que caía
# a `arrancar()` y solo «fallaba» porque a esta máquina le falta
# SIGER_SECRETO_SESION. Con las variables exportadas, el guion se habría quedado
# colgado sirviendo HTTP. Un verde por la razón equivocada.)
LOCAL="rust/target/release/siger-api"
[ -f "$LOCAL" ] || { echo "No hay binario local con que comparar ($LOCAL). Corra ./rust/construir.sh" >&2; exit 1; }
HUELLA_LOCAL=$(sha256sum "$LOCAL" | cut -d" " -f1)
HUELLA_TRAIDA=$(sha256sum "$ENTREGA/siger-api" | cut -d" " -f1)
if [ "$HUELLA_LOCAL" != "$HUELLA_TRAIDA" ]; then
    echo "El binario del servidor NO coincide con el nuestro. NO se borra nada." >&2
    echo "  aquí:     $HUELLA_LOCAL" >&2
    echo "  servidor: $HUELLA_TRAIDA" >&2
    echo "Puede ser una versión distinta a la construida... o que lo hayan cambiado." >&2
    exit 1
fi
echo "    binario verificado: sha256 $HUELLA_LOCAL"
BD=$(find "$COPIA" -maxdepth 1 -name '*.db' | head -1)
[ -n "$BD" ] || { echo "En la entrega no hay ninguna base. NO se borra nada." >&2; exit 1; }
if command -v sqlite3 >/dev/null 2>&1; then
    N=$(sqlite3 "$BD" "SELECT COUNT(*) FROM usuarios" 2>/dev/null || echo ERROR)
    [ "$N" != "ERROR" ] || { echo "La base entregada no se abre. NO se borra nada." >&2; exit 1; }
    echo "    base legible: $N usuarios"
else
    echo "    AVISO: sin sqlite3 aquí, NO se pudo comprobar que la base se abra."
    echo "    Se comprobó que existe y nada más. Compruébelo antes de usar --borrar."
fi
echo "    entrega en $COPIA (+ siger-api)"

if [ "$BORRAR" != "--borrar" ]; then
    cat <<FIN

Entrega hecha. NO se ha borrado nada: la instancia «$CLIENTE» sigue viva.

Compruebe la copia —ábrala, enséñesela al cliente— y cuando esté conforme:
    $0 $CLIENTE $DESTINO $ENTREGA --borrar
FIN
    exit 0
fi

echo "==> 4/4  BORRAR la instancia «$CLIENTE» del servidor"
ssh "$DESTINO" "sudo bash -s" <<REMOTO_FIN
set -euo pipefail
sudo systemctl disable --now "siger@$CLIENTE.service" || true
sudo systemctl disable --now "siger-respaldo@$CLIENTE.timer" || true
# El vhost primero, y recargando: si se borra el directorio con nginx todavía
# apuntando ahí, el cliente deja de ver su sitio con un 502 en vez de con un
# dominio que ya no existe.
#
# EL SITIO SE IDENTIFICA POR SU CONTENIDO, NO POR EL NOMBRE DEL ARCHIVO.
#
# La primera versión ponía \`rm -f …/sites-enabled/*"\$CLIENTE"*\`, comodín por los
# dos lados: dar de baja a «acme» habría desenlazado también el sitio de
# «acme-corp» y el de «grupo-acme», y el \`reload\` de la línea siguiente habría
# tirado la web de un cliente VIVO. El comodín contradecía al comentario de la
# directiva 22 que estaba cuatro líneas más abajo.
#
# Tampoco se puede nombrar el archivo: la baja recibe el CLIENTE, no el dominio,
# y el vhost se llama como el dominio. Se busca la zona de \`limit_req\` que el
# alta escribe, que sí lleva el cliente y es única. El ancla \`[: ]\` es lo que
# discrimina: \`zone=siger_acme:\` casa y \`zone=siger_acme-corp:\` no, porque tras
# «acme» viene un guion y no los dos caracteres que pueden seguir a una zona.
#
# LAS COMILLAS INVERTIDAS DE ESTOS COMENTARIOS VAN ESCAPADAS, y no es un detalle
# de estilo. Este \`heredoc\` NO lleva comillas —hacen falta para que \`\$CLIENTE\` se
# expanda aquí—, así que el shell LOCAL interpreta lo de dentro: sin escapar,
# \`reload\`, \`limit_req\` y \`[: ]\` se ejecutaron como órdenes en la máquina de
# desarrollo. Se vio corriendo la baja de verdad contra el VPS el 2026-08-10:
# «línea 105: reload: orden no encontrada». Esta vez no pasó nada porque eran
# palabras inofensivas; una comilla invertida con algo dentro habría corrido aquí.
#
# SE BORRA DE LOS DOS DIRECTORIOS. La primera versión miraba solo
# \`sites-enabled\`, y el alta instala el vhost en \`sites-available\`: tras la baja
# quedaba el archivo del cliente en el servidor. Medido el mismo día — el
# desmontaje se veía completo y no lo estaba.
HABILITADO=\$(grep -lE "zone=siger_$CLIENTE[: ]" /etc/nginx/sites-enabled/* 2>/dev/null || true)
DISPONIBLE=\$(grep -lE "zone=siger_$CLIENTE[: ]" /etc/nginx/sites-available/* 2>/dev/null || true)
if [ -n "\$HABILITADO" ]; then
    echo "    desenlazando: \$HABILITADO"
    sudo rm -f \$HABILITADO
    sudo nginx -t && sudo systemctl reload nginx
else
    echo "    ningún sitio HABILITADO declara la zona siger_$CLIENTE; no se recarga nginx"
fi
if [ -n "\$DISPONIBLE" ]; then
    echo "    borrando el vhost: \$DISPONIBLE"
    sudo rm -f \$DISPONIBLE
else
    echo "    ningún vhost DISPONIBLE declara la zona siger_$CLIENTE"
fi
# EL ALCANCE DEL BORRADO ES EXACTAMENTE LO QUE EL ALTA CREÓ (directiva 22), y se
# nombra pieza por pieza. Nada de comodines sobre /opt ni sobre /var/respaldos:
# ahí viven los demás clientes.
sudo rm -rf "/opt/siger-$CLIENTE" "/var/lib/siger-$CLIENTE" "/etc/siger-$CLIENTE" "/var/respaldos/$CLIENTE"
sudo userdel "siger-$CLIENTE" || true
# Las PLANTILLAS de systemd NO se borran: las comparten los demás clientes.
sudo systemctl daemon-reload
REMOTO_FIN

echo
echo "Instancia «$CLIENTE» borrada. Su copia está en $ENTREGA y no se ha tocado."
