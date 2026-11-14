#!/usr/bin/env bash
# Banco del bloque del vhost de `desplegar-demo.sh`.
#
# POR QUÉ EXISTE. Ese bloque escribe configuración VIVA del nginx que comparten
# medico (datos clínicos), chasqui, inventario y xiliux.com, y ya lleva cuatro
# rondas de revisión encima — dos de ellas encontraron defectos DENTRO de los
# arreglos de la anterior. Hasta el 2026-08-12 toda esa lógica se sostenía en
# lectura: los comentarios del guion citaban «su propio banco» y el banco vivía
# en el directorio temporal de una sesión. Un banco que se muere con la sesión
# no es un banco, es un recuerdo.
#
# LO QUE MIDE, y es la propiedad que lo hace valer: **extrae el bloque del guion
# REAL** con `sed`, no una copia a mano. Una copia envejece en cuanto alguien
# edita el original, y entonces el banco sale verde midiendo código que ya nadie
# ejecuta (directiva 31: el veredicto se ancla al ARTEFACTO).
#
#   bash despliegue/pruebas/vhost.sh                  # el bloque de HOY
#   bash despliegue/pruebas/vhost.sh --de 9479429     # el de un commit: el MUTANTE
#
# La segunda forma es la pareja de la directiva 33, y no hay que fabricarla: el
# mutante ya existe, es la versión anterior de la regla. Contra `9479429` este
# banco tiene que ponerse ROJO en el caso D.
#
# Corre en cualquier máquina, no toca nada de verdad y no necesita `sudo`: el
# `sudo`, `nginx`, `install` y `systemctl` de dentro son funciones del arnés, y
# el árbol de nginx es un directorio de usar y tirar.
set -uo pipefail

RAIZ="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
GUION="$RAIZ/despliegue/desplegar-demo.sh"
DE=""
[ "${1:-}" = "--de" ] && DE="${2:?--de necesita un commit}"

BANCO="$(mktemp -d)"
trap 'rm -rf "$BANCO"' EXIT
DISPONIBLE="$BANCO/etc/nginx/sites-available"
HABILITADO="$BANCO/etc/nginx/sites-enabled"
mkdir -p "$DISPONIBLE" "$HABILITADO" "$BANCO/tmp"
VHOST="$DISPONIBLE/informes.xiliux.com"
ENLACE="$HABILITADO/informes.xiliux.com"
ESCENARIO="$BANCO/tmp/informes.xiliux.com.nginx"

# El bloque, sacado del guion vivo (o del commit que se pida) y con las rutas
# absolutas apuntadas al árbol de mentira.
if [ -n "$DE" ]; then
    git -C "$RAIZ" show "$DE:despliegue/desplegar-demo.sh"
else
    cat "$GUION"
fi | sed -n '/^if sudo cmp -s /,/^fi$/p' \
   | sed -e "s#/etc/nginx#$BANCO/etc/nginx#g" \
         -e "s#/tmp/informes.xiliux.com.nginx#$ESCENARIO#g" > "$BANCO/bloque.sh"
# `$ESC` es el directorio de escenario del guion real —`~/.siger-despliegue` en el
# servidor, ya no `/tmp`—; aquí apunta al del banco. Se declara en el arnés y no
# se sustituye con `sed`: si mañana el guion escenifica otra cosa ahí, el banco la
# ve sin tocar nada. Cuando la ruta cambió de `/tmp` a `$ESC`, este banco se puso
# en rojo 4 de 6 — que es justamente para lo que existe.

[ -s "$BANCO/bloque.sh" ] || { echo "no se pudo extraer el bloque del vhost" >&2; exit 2; }

# `nginx -t` de mentira, y juzga lo mismo que el de verdad: recorre
# `sites-enabled/*` y se pone en rojo si una entrada CUELGA —el modo de fallo que
# costó la cuarta ronda— o si su contenido dice ROTA. Sin `nullglob` el propio
# arnés daría rojo con el directorio vacío, que fue el primer falso positivo.
cat > "$BANCO/nginx" <<'NGINX'
#!/usr/bin/env bash
shopt -s nullglob
for e in "$SITIOS"/*; do
    [ -e "$e" ] || { echo "nginx: [emerg] open() \"$e\" failed (2)" >&2; exit 1; }
    grep -q ROTA "$e" && { echo "nginx: [emerg] configuration file test failed" >&2; exit 1; }
done
exit 0
NGINX
chmod +x "$BANCO/nginx"
export SITIOS="$HABILITADO"

cat > "$BANCO/arnes.sh" <<ARNES
set -euo pipefail
sudo() { "\$@"; }
ESC="$BANCO/tmp"
nginx() { "$BANCO/nginx"; }
# el -o/-g de root no se puede honrar sin privilegios; el resto sí
install() {
    local args=() saltar=0 a
    for a in "\$@"; do
        case "\$a" in
            -o|-g) saltar=1 ;;
            *) if [ "\$saltar" = 1 ]; then saltar=0; else args+=("\$a"); fi ;;
        esac
    done
    command install "\${args[@]}"
}
systemctl() { echo "RECARGA"; }
source "$BANCO/bloque.sh"
ARNES

FALLOS=0
sembrar() {  # $1 = contenido del vhost puesto (o AUSENTE)  $2 = del que se despliega
    rm -f "$VHOST" "$VHOST".previo "$VHOST".nuevo "$VHOST".rest "$ENLACE"
    [ "$1" = AUSENTE ] || echo "$1" > "$VHOST"
    ln -s "$VHOST" "$ENLACE"          # el enlace SIEMPRE está: así está el servidor desde el 31 de julio
    echo "$2" > "$ESCENARIO"
}
caso() {  # nombre | vhost esperado | enlace sí/no | nginx VERDE/ROJO | recargas | salida
    local nombre="$1" e_vhost="$2" e_enlace="$3" e_nginx="$4" e_recargas="$5" e_salida="$6"
    local salida recargas nginx enlace vhost
    salida=0; bash "$BANCO/arnes.sh" > "$BANCO/salida.txt" 2>&1 || salida=$?
    recargas=$(grep -c '^RECARGA$' "$BANCO/salida.txt" || true)
    "$BANCO/nginx" >/dev/null 2>&1 && nginx=VERDE || nginx=ROJO
    [ -L "$ENLACE" ] && enlace=si || enlace=no
    vhost=$([ -f "$VHOST" ] && cat "$VHOST" || echo AUSENTE)
    local mal=""
    [ "$vhost"    = "$e_vhost"    ] || mal="$mal vhost=$vhost(esperado $e_vhost)"
    [ "$enlace"   = "$e_enlace"   ] || mal="$mal enlace=$enlace(esperado $e_enlace)"
    [ "$nginx"    = "$e_nginx"    ] || mal="$mal nginx=$nginx(esperado $e_nginx)"
    [ "$recargas" = "$e_recargas" ] || mal="$mal recargas=$recargas(esperado $e_recargas)"
    [ "$salida"   = "$e_salida"   ] || mal="$mal salida=$salida(esperado $e_salida)"
    if [ -z "$mal" ]; then
        printf '  ok    %s\n' "$nombre"
    else
        printf '  ROJO  %s —%s\n' "$nombre" "$mal"
        sed 's/^/          /' "$BANCO/salida.txt"
        FALLOS=$((FALLOS + 1))
    fi
}

echo "banco del vhost — bloque de ${DE:-el árbol de trabajo}"

# A · el archivo no cambió: no se toca nada y NO se recarga. Recargar nginx sin
#     motivo es mover una pieza viva por costumbre, y arrastra la configuración
#     que otros hayan dejado a medio poner.
sembrar "config-vieja" "config-vieja"
caso "A · sin cambios, no toca ni recarga" "config-vieja" si VERDE 0 0

# B · el camino feliz.
sembrar "config-vieja" "config-NUEVA"
caso "B · cambió y es bueno: instala y recarga" "config-NUEVA" si VERDE 1 0

# C · el nuevo rompe nginx: se deshace y NO se recarga.
sembrar "config-vieja" "config-ROTA"
caso "C · cambió y está roto: restaura, no recarga" "config-vieja" si VERDE 0 1

# D · EL CASO QUE COSTÓ LA CUARTA RONDA. No hay archivo previo pero el ENLACE
#     está puesto, que es justo lo que deja la ventana del `install`. Retirar
#     solo el archivo dejaba un enlace colgante: `nginx -t` en rojo GLOBAL y
#     permanente, o sea ningún producto del servidor podría recargar nunca más y
#     el hook de certbot dejaría de aplicar renovaciones. La vuelta atrás que
#     rompe el servidor no es vuelta atrás.
sembrar AUSENTE "config-ROTA"
caso "D · sin archivo previo: retira archivo Y enlace" AUSENTE no VERDE 0 1

# E · el roto es el vhost de OTRO producto. El nuestro es bueno y aun así se
#     retira —no hay forma de recargar solo lo nuestro—, pero no se recarga y no
#     se acusa a nadie: `nginx -t` no dice de quién es el rojo.
sembrar "config-vieja" "config-NUEVA"
echo "config-ROTA-de-otro" > "$DISPONIBLE/ajeno"; ln -sf "$DISPONIBLE/ajeno" "$HABILITADO/ajeno"
caso "E · el roto es de otro sitio: restaura y no recarga" "config-vieja" si ROJO 0 1
rm -f "$HABILITADO/ajeno" "$DISPONIBLE/ajeno"

# F · el vhost PUESTO estaba roto y el nuevo lo arregla. Es el caso legítimo que
#     una versión de esto rechazó por abortar al ver rojo en la medición previa
#     (directiva 21: al diseñar una restricción, el sesgo es cerrar).
sembrar "config-ROTA-vieja" "config-NUEVA"
caso "F · el puesto estaba roto y el nuevo lo arregla" "config-NUEVA" si VERDE 1 0

echo
if [ "$FALLOS" = 0 ]; then
    echo "6/6 en verde"
    exit 0
fi
echo "$FALLOS de 6 en ROJO"
exit 1
