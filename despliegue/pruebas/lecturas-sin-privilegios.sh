#!/usr/bin/env bash
# Caza la clase de defecto que el 2026-08-12 llegó a PRODUCCIÓN: una lectura o
# una prueba de existencia **sin `sudo`**, dentro de un bloque remoto que corre
# **sin privilegios**, sobre una ruta que el propio despliegue ha ENDURECIDO.
#
# QUÉ PASÓ, para que se entienda qué se vigila. `desplegar-demo.sh` cerró
# `/etc/siger-demo` a `700 root:root` —arreglo bueno: ahí dentro están el secreto
# de sesión, las claves de firma de entregas y la de cifrado en reposo—. Pero su
# bloque remoto entra por `ssh … bash -s`, o sea como `debian`, y la comprobación
# de si el entorno ya existía era `[ ! -f … ]`, sin `sudo`. Sin travesía, esa
# prueba responde «no existe» sobre un archivo que sí está, y la rama de creación
# **regeneraba los tres secretos en cada despliegue**. No dio ningún error: el
# guion imprimía «secreto de sesión generado», que en un despliegue parece normal.
#
# LA REGLA, en una frase: dentro de un bloque sin privilegios, toda lectura de una
# ruta endurecida va con `sudo`. Lo demás no es que sea inseguro — es que MIENTE,
# y miente en la dirección que crea datos nuevos en vez de fallar.
#
#   bash despliegue/pruebas/lecturas-sin-privilegios.sh
#   bash despliegue/pruebas/lecturas-sin-privilegios.sh --de 92524b4   # el mutante
#
# La segunda forma es la pareja obligatoria (directiva 33), y el mutante no hay
# que fabricarlo: es la versión anterior de los guiones. Contra `92524b4` esto
# tiene que encontrar EXACTAMENTE el `[ ! -f /etc/siger-demo/siger.env ]`. Un
# detector que solo se prueba contra archivos limpios sale verde con la regla
# buena y con una regla ciega.
set -uo pipefail

RAIZ="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DE=""
[ "${1:-}" = "--de" ] && DE="${2:?--de necesita un commit}"
GUIONES=(desplegar-demo.sh publicar-demo.sh alta-cliente.sh baja-cliente.sh)

# Las rutas cuyo contenido no puede leer un usuario cualquiera del servidor: ahí
# viven los secretos, la configuración viva de nginx y las bases de los clientes.
ENDURECIDAS='/etc/siger-|/etc/nginx|/etc/letsencrypt|/var/lib/siger-|/var/respaldos|/opt/siger-'
# Lo que LEE. `echo` no está: nombrar una ruta en un mensaje no es leerla.
LECTORAS='(\[|\[\[|test|grep|cat|stat|ls|head|tail|source|\.|cmp|diff|md5sum|sha256sum|awk|sed)[[:space:]]'

hallazgos=0
revisados=0

for g in "${GUIONES[@]}"; do
    if [ -n "$DE" ]; then
        texto="$(git -C "$RAIZ" show "$DE:despliegue/$g" 2>/dev/null)" || continue
    else
        [ -f "$RAIZ/despliegue/$g" ] || continue
        texto="$(cat "$RAIZ/despliegue/$g")"
    fi

    # Un bloque remoto es SIN PRIVILEGIOS si su `ssh` no dice `sudo bash -s`.
    # Los de `alta-cliente.sh` y `baja-cliente.sh` sí lo dicen, y por eso no
    # pueden tener este defecto: corren enteros como root. Esa es la lección de
    # diseño, y aquí se comprueba en vez de suponerse.
    #
    # awk SOLO recorta el bloque —esa parte necesita estado— y el filtrado va con
    # `grep -E`, que no obliga a escapar corchetes dentro de una cadena de awk.
    # La primera versión metía el patrón en awk, la expresión salía inválida, awk
    # MORÍA, y el guion leía la salida vacía como «limpio»: un detector que falla
    # ABIERTO y CALLADO. Lo cazó su pareja, porque el mutante también salía en
    # verde. Por eso ahora el estado de salida de awk se mira y, si falla, esto
    # aborta a gritos en vez de dar por buenos los guiones.
    bloque="$(printf '%s\n' "$texto" | awk '
        # LOS COMENTARIOS SE SALTAN ANTES QUE NADA, y esto no es higiene: un
        # comentario que EXPLICA el patrón —«el bloque entra por `ssh … bash -s`»,
        # «alta-cliente entra por `ssh … "sudo bash -s"`»— casa la regla de abrir
        # y la de cerrar, y cerraba el bloque en seco. Se extraían 21 líneas de
        # ~350 y el detector salía verde. Lo destapó una aguja sintética: contra
        # el mutante acertaba, porque en aquella versión esos comentarios todavía
        # no estaban escritos. Un detector probado solo contra el caso histórico
        # reconoce el caso histórico, no la clase.
        /^[[:space:]]*#/ { next }
        /ssh .*bash -s/ {
            dentro = ($0 ~ /sudo bash -s/) ? 0 : 1
            if (match($0, /<<-?.?[A-Z_]+.?$/)) {
                fin = substr($0, RSTART, RLENGTH)
                gsub(/[<>'"'"'"-]/, "", fin)
            }
            next
        }
        dentro && fin != "" && $0 == fin { dentro = 0; next }
        dentro { printf "%d: %s\n", NR, $0 }
    ')"
    if [ "${PIPESTATUS[1]:-0}" != 0 ] || [ $? -gt 1 ]; then
        echo "el recortador de bloques falló sobre $g: no se puede concluir nada" >&2
        exit 2
    fi

    salida="$(printf '%s\n' "$bloque" \
        | grep -vE '^[0-9]+:[[:space:]]*#' \
        | grep -v 'sudo' \
        | grep -E "$ENDURECIDAS" \
        | grep -E "$LECTORAS" \
        | sed 's/^/    /')"

    revisados=$((revisados + 1))
    if [ -n "$salida" ]; then
        printf '  ROJO  %s\n%s\n' "$g" "$salida"
        hallazgos=$((hallazgos + 1))
    else
        printf '  ok    %s\n' "$g"
    fi
done

echo
echo "guiones revisados: $revisados"
if [ "$hallazgos" = 0 ]; then
    echo "ninguna lectura sin privilegios sobre ruta endurecida"
    exit 0
fi
cat >&2 <<'AVISO'

Cada línea de arriba lee una ruta endurecida SIN `sudo`, dentro de un bloque que
corre como el usuario de despliegue. Ahí la respuesta no es «no puedo leerlo»,
es «no existe» — y si detrás hay una rama de creación, ESCRIBE.

Arreglo: `sudo test -f …` en vez de `[ -f … ]`, `sudo grep`, `sudo cat`.
AVISO
exit 1
