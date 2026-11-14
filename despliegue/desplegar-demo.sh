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
#   - habilitar el sitio de nginx la PRIMERA vez. Después ya está enlazado en
#     sites-enabled —lo está desde el 31 de julio—, así que este guion escribe
#     configuración VIVA: la compara antes, corre `nginx -t` después y solo
#     recarga si cambió.
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
DATOS=/var/lib/siger-demo   # lo que la demo ESCRIBE, separado del programa

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
# El panel es `maud` + htmx: el HTML lo compila `rustc` y los cuatro activos
# entran por `include_str!` desde `estaticos/`. Aquí se mira el ARTEFACTO —no el
# árbol—: que el htmx VENDORIZADO viaje dentro, que es la mitad-cliente del panel
# y el equivalente maud del viejo `index.html`. `htmx:load` es un evento interno
# de la librería, así que su presencia prueba que `htmx.min.js` se empotró de
# verdad; medido, aparece en el release y NO en un binario sin la interfaz.
# (Antes se buscaba `<!doctype html`, literal del `index.html` de Angular que
# `maud` ya no produce: daba falso negativo y abortaba el despliegue del panel.)
if ! grep -aq "htmx:load" "$BIN"; then
    echo "El binario NO lleva la interfaz empotrada (htmx no aparece). No se sube." >&2
    exit 1
fi

echo "==> 3/5  copiar a $DESTINO:$REMOTO"
ssh "$DESTINO" "sudo mkdir -p $REMOTO $DATOS /etc/siger-demo && sudo chown -R \$(id -un):\$(id -gn) $REMOTO && sudo chmod 700 /etc/siger-demo"
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
# LO QUE SE ESCENIFICA NO VA A `/tmp`, y el motivo no es limpieza. `/tmp` es
# `1777` —de todos, con sticky—, y los nombres que se usaban eran FIJOS y
# predecibles (`/tmp/siger-demo.service`, `/tmp/informes.xiliux.com.nginx`).
# Entre el `scp` y el `sudo install` hay una ventana en la que otro usuario local
# del servidor puede plantar ahí su archivo o su enlace, y lo que `install`
# instala como root es una UNIDAD DE SYSTEMD y un vhost del nginx compartido.
#
# Que hoy no sea explotable en esta máquina lo sostiene el NÚCLEO, no este guion:
# `fs.protected_symlinks=1`, `fs.protected_regular=2`, `fs.protected_hardlinks=1`
# (medidos en el VPS el 2026-08-12). Una salvaguarda que vive en un sysctl del
# servidor y no está declarada en ninguna parte es una que desaparece el día que
# esto se despliegue en otro host. Con un directorio del propio usuario a 700 no
# hay ventana que aprovechar, y cuesta una línea.
ssh "$DESTINO" 'mkdir -p ~/.siger-despliegue && chmod 700 ~/.siger-despliegue'

scp "$BIN" "$DESTINO:$REMOTO/siger-api.nuevo"
scp rust/sembrar_demo.py "$DESTINO:$REMOTO/"
scp despliegue/siger-demo.service despliegue/siger-demo-sembrar.service \
    despliegue/siger-demo-sembrar.timer "$DESTINO:.siger-despliegue/"
# El respaldo NO se instala aquí: la demostración se regenera cada noche, así que
# respaldarla sería guardar copias de datos ficticios. `siger-respaldo.service` y
# su timer son para una instalación de CLIENTE.
scp despliegue/informes.xiliux.com.nginx "$DESTINO:.siger-despliegue/"

echo "==> 4/5  configurar y arrancar"
ssh "$DESTINO" bash -s <<'REMOTO_FIN'
set -euo pipefail
REMOTO=/opt/siger-demo
DATOS=/var/lib/siger-demo
# Donde el paso 3/5 dejó lo que hay que instalar: directorio del propio usuario,
# a 700. No `/tmp`, que es de todos y con nombres predecibles (ver arriba).
ESC="$HOME/.siger-despliegue"

# Usuario de servicio sin shell: la demo es pública.
id siger >/dev/null 2>&1 || sudo useradd --system --no-create-home --shell /usr/sbin/nologin siger

# El secreto se genera UNA vez y no se rota en cada despliegue: rotarlo cierra
# la sesión de quien esté mirando la demostración en ese momento.
# `sudo test -f`, y NO `[ ! -f ]`. El bloque remoto entra por `ssh … bash -s`,
# o sea como `debian`, y desde que este directorio pasó a 700 root:root un
# `debian` no puede ni atravesarlo: la prueba sin privilegios decía «no existe»
# sobre un archivo que sí está, y esta rama REGENERABA los tres secretos en cada
# despliegue —sesión, firma de entregas y reposo— sin que nada lo delatara. Pasó
# de verdad, en el primer despliegue con el directorio ya endurecido: el guion
# imprimió «secreto de sesión generado» sobre una instalación que llevaba
# semanas con los suyos.
#
# En la demo el daño es acotado porque la base se resiembra entera en cada
# corrida; en una instalación de cliente, regenerar `SIGER_CLAVE_REPOSO` deja
# ilegibles TODOS los archivos de entrega cifrados y TODOS los secretos TOTP.
# `alta-cliente.sh` no tiene este defecto: sus bloques remotos entran por
# `ssh … "sudo bash -s"`, así que sus comprobaciones corren como root.
if ! sudo test -f /etc/siger-demo/siger.env; then
    SECRETO="$($REMOTO/siger-api generar-secreto)"
    # La firma de entregas y el cifrado en reposo se activan EN LA DEMOSTRACIÓN,
    # no solo en una instalación de pago. Son dos de las tres cosas que
    # diferencian esto de una hoja de cálculo, y con las claves ausentes la
    # entrega recién subida sale con `tiene_firma: false` — es decir, enseñando
    # en pantalla lo contrario de lo que uno acaba de decir.
    CLAVES="$($REMOTO/siger-api generar-claves-firma)"
    REPOSO="$($REMOTO/siger-api generar-secreto)"
    # NACE a 600, no se corrige a 600 tres líneas después. `tee` CREA el archivo
    # con el umask de sudo —0022 en este servidor, medido—, así que entre el
    # `tee` y el `chmod` había una ventana con modo 644 y los tres secretos ya
    # dentro: el de sesión (falsifica cualquier cookie), la clave de firma de las
    # entregas y la de reposo. Y `/etc/siger-demo` es atravesable por todos, así
    # que la ventana es alcanzable: un `inotify` sobre el directorio no necesita
    # acertar el instante —el evento es fijo y anunciado—, y con el descriptor ya
    # abierto el `chmod` posterior no le quita nada.
    #
    # El patrón bueno ya estaba escrito en este repositorio: `alta-cliente.sh`
    # crea el suyo con `umask 077`. La demostración —la instancia más expuesta
    # que tenemos— era la única que se había quedado con la variante insegura.
    #
    # Dueño `root:root` y no `siger`: systemd lee `EnvironmentFile=` como PID 1,
    # ANTES de bajar a `User=siger`, así que el servicio no necesita leerlo — y
    # que pudiera ESCRIBIRLO le regalaba a una demo comprometida un punto de
    # persistencia para el siguiente arranque.
    sudo install -m 600 -o root -g root /dev/null /etc/siger-demo/siger.env
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
    # Se REAFIRMAN, que no cuesta nada y cierra el caso de que el archivo ya
    # existiera con otro modo por una corrida vieja de este mismo guion.
    sudo chmod 600 /etc/siger-demo/siger.env
    sudo chown root:root /etc/siger-demo/siger.env
    echo "  secreto de sesión generado"
else
    # Se conserva el CONTENIDO —rotarlo cerraría la sesión de quien esté mirando
    # la demo—, pero no el modo ni el dueño: el archivo que hay hoy en el
    # servidor nació con la variante insegura de este mismo guion y quedó a
    # nombre de `siger`, o sea escribible por el servicio más expuesto de la
    # máquina. Corregirlo solo en el camino de creación dejaría la instalación
    # que YA existe —la única que hay— sin arreglar.
    sudo chmod 600 /etc/siger-demo/siger.env
    sudo chown root:root /etc/siger-demo/siger.env
    echo "  /etc/siger-demo/siger.env ya existe: se conserva (modo y dueño reafirmados)"
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

sudo install -m 644 "$ESC"/siger-demo.service "$ESC"/siger-demo-sembrar.service \
     "$ESC"/siger-demo-sembrar.timer /etc/systemd/system/
# Y se recoge lo escenificado. No es manía de limpieza: lo que se queda en el
# suelo es lo que alguien confunde mañana con lo instalado, y el vhost ya se
# recogía en sus tres caminos. Que estén en un directorio 700 lo hace inofensivo,
# no innecesario.
rm -f "$ESC"/siger-demo.service "$ESC"/siger-demo-sembrar.service "$ESC"/siger-demo-sembrar.timer
# EL VHOST SE COMPARA ANTES DE ESCRIBIRLO, Y SE COMPRUEBA DESPUÉS.
#
# La cabecera de este guion dice que el archivo «queda en sites-available» a la
# espera de que alguien lo habilite. Dejó de ser cierto el 31 de julio: hay un
# enlace en `sites-enabled`, así que esto escribe configuración VIVA de un
# servidor que también sirve datos clínicos de otro producto.
#
# Y el peligro no es escribirla: es escribirla MAL y que no se note. nginx no
# relee al instalar, así que una configuración rota se aplicaría en el próximo
# `reload` que dispare OTRO producto —medico, chasqui— o la renovación de
# certbot, y su `nginx -t` en rojo bloquearía el trabajo de ese otro. Un fallo
# que aparece lejos de su causa y en el turno de alguien más.
#
# Si no cambió, no se toca: recargar nginx sin motivo es mover una pieza viva
# por costumbre.
if sudo cmp -s "$ESC"/informes.xiliux.com.nginx /etc/nginx/sites-available/informes.xiliux.com; then
    echo "    vhost: sin cambios, no se toca"
    sudo rm -f "$ESC"/informes.xiliux.com.nginx
else
    echo "    vhost: CAMBIÓ — se instala, se comprueba y se recarga"
    VHOST=/etc/nginx/sites-available/informes.xiliux.com
    ENLACE=/etc/nginx/sites-enabled/informes.xiliux.com

    # 1. `nginx -t` ANTES de tocar nada, porque juzga TODA la configuración del
    #    servidor y no la nuestra. Sin esta medición, un vhost ajeno ya roto
    #    —medico, chasqui, inventario— hacía fallar la prueba de después y el
    #    guion acusaba al archivo equivocado: «el vhost nuevo NO pasa nginx -t»,
    #    y a arreglar lo que no era.
    #
    #    INFORMA, NO BLOQUEA, y la diferencia importa: la primera versión de esto
    #    abortaba al ver rojo, y con eso rechazaba el caso legítimo más obvio
    #    —que el vhost roto sea EL NUESTRO y este despliegue traiga justo el
    #    arreglo—. Lo cazó el caso F de `despliegue/pruebas/vhost.sh`
    #    (directiva 21: al diseñar una restricción, el sesgo es cerrar). Lo que
    #    se gana midiendo antes es el DIAGNÓSTICO: saber si lo rompimos nosotros
    #    o llegó roto.
    #
    #    ESTE BLOQUE TIENE BANCO, y se corre a mano:
    #        bash despliegue/pruebas/vhost.sh              # los 6 caminos
    #        bash despliegue/pruebas/vhost.sh --de <sha>   # contra un mutante
    #    Extrae el bloque de ESTE archivo con `sed`, así que no envejece por
    #    estar copiado. Si editas lo de abajo, córrelo.
    YA_ESTABA_ROJO=no
    if ! sudo nginx -t; then
        YA_ESTABA_ROJO=si
        echo "  AVISO: el nginx del servidor YA estaba en rojo antes de tocar nada." >&2
        echo "  Puede que este vhost sea la causa y este despliegue el arreglo." >&2
        echo "  Se sigue, y se vuelve a medir después." >&2
    fi

    # 2. El respaldo NO puede fallar callando. Antes iba con `2>/dev/null ||
    #    true`, y entonces la restauración de abajo imprimía «se restauró el
    #    anterior» hubiera respaldo o no: un mensaje que miente en el único
    #    momento en que alguien lo lee (directiva 20). Y hay un caso en que no
    #    hay nada que respaldar —host nuevo—, que es distinto de «falló el
    #    respaldo» y necesita otra vuelta atrás: RETIRAR el archivo nuevo.
    HAY_PREVIO=no
    if sudo test -f "$VHOST"; then
        sudo cp -a "$VHOST" "$VHOST.previo"
        HAY_PREVIO=si
    fi

    # 3. Sustitución ATÓMICA. `install` DESENLAZA el destino antes de escribir,
    #    así que dejaba una ventana en la que el vhost —que está ENLAZADO en
    #    sites-enabled desde el 31 de julio— no existía o estaba a medias. Si la
    #    corrida muere ahí (esto habla por ssh de Medellín a Montréal), se queda
    #    así y `nginx -t` no llega a correr nunca. Con `install` a un nombre
    #    temporal del MISMO directorio y un `mv`, el cambio de nombre es atómico:
    #    nginx ve el archivo viejo o el nuevo, jamás medio archivo.
    sudo install -m 644 -o root -g root "$ESC"/informes.xiliux.com.nginx "$VHOST.nuevo"
    sudo mv -f "$VHOST.nuevo" "$VHOST"

    # 4. `nginx -t` en su PROPIA línea, sin tubería. Con `| tail -2` quien decide
    #    el código de salida es `tail`, y que aquí no enmascarara el fallo lo
    #    sostenía un `set -o pipefail` declarado 175 líneas antes. Una salvaguarda
    #    de esta consecuencia no cuelga de una opción lejana.
    if ! sudo nginx -t; then
        # Se DESHACE en el acto. Dejar rota la configuración de un servidor
        # compartido es peor que no desplegar el vhost: `nginx.conf` incluye
        # `sites-enabled/*`, así que el error es GLOBAL — el siguiente `reload`
        # (que dispara el hook de certbot para CUALQUIER certificado renovado)
        # dejaría de aplicar, y los certificados de los demás sitios morirían a
        # los 90 días sin un solo error a la vista.
        # DE QUIÉN es el rojo. La medición del punto 1 existe para poder decirlo,
        # y decirlo mal es el defecto exacto que este bloque vino a arreglar: la
        # frase «el vhost nuevo NO pasa nginx -t» es FALSA cuando lo que está
        # roto es el sitio de otro producto, y manda a arreglar lo que no era.
        # `YA_ESTABA_ROJO` significa «algo estaba rojo», NO «lo rojo es ajeno»
        # —el vhost roto puede ser el nuestro—, así que aquí no se atribuye más
        # de lo que se midió.
        if [ "$YA_ESTABA_ROJO" = si ]; then
            echo "  'nginx -t' sigue en rojo con el vhost nuevo puesto, y YA lo estaba ANTES." >&2
            echo "  Puede ser este vhost o puede ser otro sitio: la medición no lo distingue." >&2
        else
            echo "  El vhost nuevo ROMPE 'nginx -t': antes del cambio el servidor estaba VERDE." >&2
        fi

        if [ "$HAY_PREVIO" = si ]; then
            # También por `.rest` + `mv`: la restauración se hace justo cuando ya
            # se sabe que la configuración puesta está rota, y un `cp` directo
            # sobre el archivo vivo tiene la misma ventana de medio archivo que
            # el camino de ida. Y va dentro de un `if` para que un fallo aquí no
            # mate el guion por `set -e` ANTES de la remedición de abajo: el
            # mensaje que importa es el que se imprime cuando esto sale mal.
            if sudo cp -a "$VHOST.previo" "$VHOST.rest" && sudo mv -f "$VHOST.rest" "$VHOST"; then
                echo "  Se restauró el vhost anterior." >&2
            else
                echo "  Y la RESTAURACIÓN FALLÓ: el vhost malo SIGUE PUESTO." >&2
            fi
        else
            sudo rm -f "$VHOST"
            echo "  No había vhost anterior: se RETIRA el nuevo." >&2
            # Y con él su ENLACE, si lo hay. `HAY_PREVIO` mide el ARCHIVO, y el
            # enlace de `sites-enabled` es independiente: aquí lleva puesto desde
            # el 31 de julio. Retirar solo el archivo dejaría un enlace COLGANTE
            # dentro de `include sites-enabled/*` —medido: `nginx: [emerg]
            # open() … failed (2)`—, o sea `nginx -t` en rojo GLOBAL y para
            # siempre: ningún producto del servidor podría volver a recargar, y
            # el hook de certbot dejaría de aplicar las renovaciones de todos.
            # La vuelta atrás que rompe el servidor no es vuelta atrás.
            if sudo test -L "$ENLACE"; then
                sudo rm -f "$ENLACE"
                echo "  y también su enlace en sites-enabled, que quedaría colgando." >&2
            fi
        fi

        # Y se COMPRUEBA que la vuelta atrás funcionó. «Restaurado» sin volver a
        # medir es un negativo que nunca se produjo (directiva 30): si esto sale
        # en rojo, el servidor está roto AHORA y hay que decirlo a gritos, no
        # despedirse con un mensaje tranquilizador.
        if sudo nginx -t; then
            echo "  Vuelta atrás verificada: nginx queda en VERDE. Se aborta." >&2
        else
            echo "  ATENCIÓN: tras la vuelta atrás, 'nginx -t' SIGUE en rojo." >&2
            if [ "$YA_ESTABA_ROJO" = si ]; then
                echo "  Ya lo estaba antes de este despliegue; puede ser este vhost o puede ser otro." >&2
            else
                echo "  Antes del cambio estaba en VERDE: la vuelta atrás NO ha bastado." >&2
            fi
            echo "  La configuración del servidor COMPARTIDO está rota ahora mismo." >&2
            echo "  Mírala a mano antes de cualquier reload: sudo nginx -t" >&2
        fi
        sudo rm -f "$ESC"/informes.xiliux.com.nginx
        exit 1
    fi
    # `reload` sale 0 en cuanto manda la señal —`ExecReload` es `nginx -s
    # reload`—, así que su éxito NO prueba que el vhost nuevo esté vivo. Eso solo
    # lo mide el `curl` contra el dominio del paso 5/5. Y aplica `conf.d/` y
    # `sites-enabled/` ENTEROS: una edición ajena guardada y todavía sin recargar
    # entra en producción en NUESTRO turno. No hay forma de acotar un `reload` a
    # un vhost; por eso el `nginx -t` del punto 1 es también un favor a los demás.
    sudo systemctl reload nginx
    sudo rm -f "$ESC"/informes.xiliux.com.nginx
fi

# RED PARA LO QUE VIENE. Entre este `stop` y el `enable --now` de más abajo hay
# cinco pasos con `set -e`, y el más frágil es la siembra —que en ESTE despliegue
# estrena destino—. Si aborta ahí, la demo se queda parada y el guion no la
# levanta: es la caída de 6 h del 2026-07-31, cuya lección `siger-demo-sembrar`
# sí aprendió (`ExecStopPost=+systemctl start`) y este guion no.
#
# El `daemon-reload` va DENTRO de la trampa a propósito: las unidades ya se
# instalaron más arriba, así que un arranque de emergencia sin recargar usaría la
# unidad CACHEADA —con el `WorkingDirectory` viejo— y la demo volvería sobre la
# base anterior sin que nada lo dijera.
# **La trampa NO levanta una demo VACÍA**, y ésa es la trampa de la trampa. El
# binario abre la base con `create_if_missing(true)`, así que si la siembra
# aborta —el paso más frágil, y el que en ESTE despliegue estrena destino—
# arrancar sin más crearía una base nueva, migrada y sin una sola cuenta: la
# demo respondería 200 en `/entrar` mientras el cartel anuncia tres credenciales
# que ahí no existen. Y la base vieja ya se borró: no hay a qué volver.
#
# Dejarla PARADA es peor de ver y mejor de entender: hoy un despliegue fallido
# deja la demo caída, que es evidente; lo que no puede es dejarla en pie y
# mintiendo.
#
# El `|| true` va en el `daemon-reload` y no solo en el `start`: dentro de la
# trampa sigue vigente `set -e`, así que si el primer comando falla el segundo
# —el único útil— no llega a correr. Estaba protegido el que puede fallar sin
# consecuencia y desprotegido el que no debe fallar. Medido con su pareja.
trap 'sudo systemctl daemon-reload || true
      if [ -f "$DATOS/siger.db" ]; then
          sudo systemctl start siger-demo.service || true
      else
          echo "  La demo queda PARADA a propósito: la siembra no terminó y no hay base." >&2
          echo "  Arrancarla ahora crearía una vacía y el cartel anunciaría cuentas que no existen." >&2
      fi' EXIT

sudo systemctl stop siger-demo.service 2>/dev/null || true
# Y se COMPRUEBA que paró. Era la única vía de verde falso del guion: con el
# servicio vivo, el `mv` renombra igual, el `rm -rf` desenlaza una base que el
# proceso mantiene abierta por su inodo, `enable --now` sobre una unidad ya
# activa no reinicia nada — y el guion terminaba anunciando «Desplegado y
# PUBLICADO» con el binario VIEJO sirviendo la base VIEJA.
if systemctl is-active --quiet siger-demo.service; then
    echo "No se pudo parar siger-demo: se aborta antes de tocar nada." >&2
    exit 1
fi

# Y AHORA el binario nuevo ocupa su sitio, con el servicio ya parado. `mv` sobre
# un ejecutable en uso sí está permitido —renombrar no es escribir dentro—, pero
# se hace igualmente con el servicio abajo: si el `mv` fallara, lo que quedaría
# corriendo sería el binario viejo, entero, y no uno a medio escribir.
if [ -f "$REMOTO/siger-api.nuevo" ]; then
    sudo mv "$REMOTO/siger-api.nuevo" "$REMOTO/siger-api"
fi
# EL PROGRAMA ES DE ROOT, Y EL DIRECTORIO TAMBIÉN.
#
# El 2026-07-30 una revisión puso el binario a nombre de root con el argumento
# correcto —«ser dueño de lo que ejecutas es poder reescribirlo»— y cerró solo la
# mitad: `chown -R siger:siger "$REMOTO"` dejaba el DIRECTORIO de `siger`, y
# **borrar un archivo pide permiso sobre el directorio, no sobre él**. La demo
# podía reemplazar su propio ejecutable, y es la instancia más expuesta que
# tenemos: sus credenciales están anunciadas en la pantalla de acceso.
#
# Cerrado el 2026-08-11 igual que en las instancias por cliente: el programa en
# `/opt/siger-demo` (root, y nadie más escribe ahí) y todo lo que la demo escribe
# —base, almacén, respaldos— en `/var/lib/siger-demo`.
# LO QUE ESTA CORRIDA DEJA DE REGENERAR HAY QUE BORRARLO, y es la directiva 22
# leída del revés. Hasta hoy la base y el almacén de la demo vivían aquí y los
# limpiaba `sembrar_demo.py --limpiar` cada noche. Al mudar los datos a $DATOS,
# el nocturno apunta a otro sitio y **esto deja de limpiarse para siempre**: la
# instantánea del día del despliegue se queda congelada, y esa base incluye
# `intento_acceso` (correo y dirección de cada visitante) y `auditoria`. Sigue
# siendo legible por cualquier usuario local de la máquina —comprobado en el
# servidor— porque `chown` no cambia modos y el directorio queda atravesable.
#
# Se enumera lo que se borra, sin comodines sobre el directorio: los `.bak-*` de
# binarios viejos sí llevan uno, y va acotado a ese prefijo.
# El `rm -rf` no puede depender de que tres líneas más arriba haya un literal:
# `set -u` cubre «no definida», no «vacía». Esto lo convierte en mecanismo.
: "${REMOTO:?la raíz del programa no puede estar vacía}"
: "${DATOS:?la raíz de datos no puede estar vacía}"
sudo rm -rf "$REMOTO/siger.db" "$REMOTO/siger.db-wal" "$REMOTO/siger.db-shm" \
            "$REMOTO/storage" "$REMOTO/respaldos" "$REMOTO"/*.bak-*

sudo chown -R root:root "$REMOTO"
sudo chmod 755 "$REMOTO" "$REMOTO/siger-api" "$REMOTO/sembrar_demo.py"
sudo chown -R siger:siger "$DATOS"
sudo chmod 750 "$DATOS"
sudo -u siger python3 "$REMOTO/sembrar_demo.py" \
     --ruta "$DATOS" --binario "$REMOTO/siger-api" --limpiar

sudo systemctl daemon-reload
sudo systemctl enable --now siger-demo.service
trap - EXIT   # desarmada DESPUÉS de que la demo esté arriba, no antes
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
#
# Y SE GUARDA EL CUERPO, no solo el código. Un 200 dice que ALGO contestó por ese
# nombre; no dice que sea SIGER. Un vhost por defecto, un comodín de DNS o una
# página de aparcamiento devuelven 200 igual de bien, y con solo el código este
# guion anunciaba «PUBLICADO, las credenciales son alcanzables» sobre cualquiera
# de los tres. El hermano `publicar-demo.sh` ya exigía además un `<title>` no
# vacío (l.130 y 134); esto es traer aquí esa misma comprobación.
CUERPO=$(mktemp)
trap 'rm -f "$CUERPO"' EXIT
# El `|| true` sigue haciendo falta por `set -e`, pero ahora se conserva ADEMÁS
# el código de salida de curl, que es lo que separa dos cosas que `000` mezclaba:
# «no hubo red» y «el certificado no vale». Un certificado caducado deja el sitio
# ROTO PARA TODO VISITANTE, y decir de eso «puede ser la red de aquí» es el mismo
# error que este guion vino a arreglar, un escalón más abajo.
# Se pone a cero ANTES: si no, un `CURL_ESTADO` exportado en el entorno
# sobreviviría a `${CURL_ESTADO:-0}` y se tomaría la rama del certificado sobre
# un sitio sano.
CURL_ESTADO=0
# `-L`: la raíz del panel `maud` **redirige** —`/` → `/entregas` → (sin sesión)
# → `/entrar`—, así que sin seguir el redirect se mediría el 303 y un cuerpo
# vacío, sin título. Con `-L`, `%{http_code}` reporta el código FINAL (200 en
# `/entrar`) y `$CUERPO` trae su HTML, que es donde vive el `<title>`. El destino
# es siempre el mismo origen sobre HTTPS; el saneado del título de abajo protege
# igual sea cual sea el cuerpo que vuelva. (Antes no llevaba `-L`: el Angular
# viejo servía 200 en `/`, y el 303 de `maud` caía al último `else` como si fuera
# un 502 o un comodín de DNS.)
PUBLICADO=$(curl -sL -o "$CUERPO" -w '%{http_code}' --max-time 15 "https://$DOMINIO/" 2>/dev/null) || CURL_ESTADO=$?
CURL_ESTADO=${CURL_ESTADO:-0}
[ -n "$PUBLICADO" ] || PUBLICADO="000"
# El TEXTO del título, no la etiqueta, y saneado. Tres cosas que la primera
# versión de esto hacía mal y que cazó la revisión de seguridad de este mismo
# cambio — o sea, el arreglo repetía el defecto que venía a arreglar:
#
#   1. `[ -z "$TITULO" ]` sobre `<title></title>` es FALSO: la cadena con las
#      etiquetas nunca está vacía. Un título vacío pasaba por SIGER.
#   2. Cualquier título pasaba. `<title>Welcome to nginx!</title>` y
#      `<title>This domain is parked</title>` —los dos ejemplos que el comentario
#      de arriba nombra como aquello de lo que protege— entraban por la rama de
#      éxito. Se medía «hay etiqueta title», no «contesta SIGER».
#   3. El cuerpo lo escribe un SERVIDOR REMOTO y salía crudo al terminal. Con
#      `\033[5A\033[2K\033[J` dentro, quien conteste por ese nombre BORRA las
#      líneas anteriores del informe y escribe lo que quiera en su lugar: la
#      inversión exacta de lo que este guion existe para decir. Y con OSC 52
#      (`\033]52;c;`) se escribe en el portapapeles, justo cuando el mensaje
#      termina con comandos para copiar.
#
# EL SANEADO ES UNA LISTA BLANCA, y la primera versión no lo era: `tr -d` quitaba
# los controles de 7 bits (C0) y dejaba pasar los **C1** —U+0080..U+009F—, que en
# UTF-8 son dos bytes y que un terminal interpreta igual. `\u009b` ES el CSI de 8
# bits: `\u009b4A\u009b2K` borra líneas exactamente como `\033[4A\033[2K`.
# Comprobado en GNU screen y en pyte, con control negativo. También pasaban OSC 52
# de 8 bits (portapapeles), DCS y DEL (0x7f).
#
# Enumerar lo que se PROHÍBE deja fuera lo que no se te ocurrió; enumerar lo
# permitido manda el resto a quitarse, que es la dirección segura (directiva 23).
# El título real es ASCII imprimible puro, así que `\040-\176` no pierde nada.
#
# Y la comprobación va ANCLADA al SUFIJO (`SIGER$`), no por subcadena suelta: el
# panel `maud` titula cada página con la forma «<página> · SIGER» —la raíz aterriza
# en `Entrar · SIGER`—, y el punto medio `·` (U+00B7, dos bytes en UTF-8) lo quita
# el saneado de lista blanca ASCII de arriba, dejando `Entrar  SIGER`. Anclar al
# final reconoce esa convención y sigue rechazando `Welcome to nginx!` y
# `This domain is parked`. Si alguien cambia el título de la aplicación, este guion
# dirá «no es SIGER» — falla CERRADO y se ve en el primer despliegue, que es mejor
# que dar por publicado lo que no se reconoce. (Antes era `^SIGER$`, atado al
# `angular/src/index.html:5` que se fue con Angular; la fuente hoy es el layout de
# `vista.rs`.)
TITULO=$(grep -oE '<title>[^<]*</title>' "$CUERPO" 2>/dev/null | head -1 \
         | sed -E 's|</?title>||g' | LC_ALL=C tr -cd '\040-\176' | cut -c1-80 || true)

if [ "$CURL_ESTADO" = "60" ]; then
    # 60 es el código de curl para «no pude verificar el certificado». Va ANTES
    # que el resto porque es un diagnóstico, no una incógnita: el sitio está roto
    # para cualquiera que lo visite, y merece decirse con su nombre en vez de
    # caer en «no se pudo comprobar».
    cat <<FIN

Desplegado. **El CERTIFICADO de https://$DOMINIO/ no valida** (curl 60).

Esto NO es «no se pudo medir»: la medición ocurrió y su resultado es que **desde
esta máquina** el certificado no valida. Suele ser el certificado vencido o
emitido para otro nombre —y entonces está roto para todo el mundo—, pero también
lo produce un problema de ESTE lado: un almacén de CA incompleto, un proxy que
inspecciona TLS o el reloj desfasado. Los dos comandos de abajo lo separan antes
de tocar nada en producción.

  echo | openssl s_client -connect $DOMINIO:443 -servername $DOMINIO 2>/dev/null \\
    | openssl x509 -noout -dates -subject
  ssh $DESTINO 'sudo certbot certificates'
FIN
elif [ "$PUBLICADO" = "200" ] && ! printf '%s' "$TITULO" | grep -qE 'SIGER$'; then
    # 200 SIN título: contesta algo que no es la aplicación. Se dice tal cual en
    # vez de anunciar que las credenciales están publicadas, que es lo que haría
    # falso el aviso justo cuando más importa acertarlo.
    cat <<FIN

Desplegado. https://$DOMINIO/ responde 200 **pero lo que contesta no es
SIGER**: su título es «$TITULO» y debería decir SIGER.

Un 200 dice que algo contesta por ese nombre, no que sea esta aplicación. Suele
ser el vhost por defecto de nginx, un comodín de DNS apuntando a otro sitio, o
una página de aparcamiento. NO se da por publicada: mira qué está contestando.

  curl -s https://$DOMINIO/ | head -40 | LC_ALL=C tr -cd '\\040-\\176\\n'
  ssh $DESTINO 'sudo nginx -T | grep -n "server_name"'
FIN
elif [ "$PUBLICADO" = "200" ]; then
    cat <<FIN

Desplegado y PUBLICADO: https://$DOMINIO/ responde 200 y sirve $TITULO.

El título va ahí como EVIDENCIA, no como adorno: es lo que separa «algo contesta
por ese nombre» de «contesta esta aplicación», y sin él este mensaje se emitía
igual ante un vhost por defecto o un comodín de DNS.

Es una demostración pública a propósito, así que **las credenciales que anuncia
son alcanzables desde internet**. Lo que se sirve ahí, medido como lo mide el
mundo y no con un \`is-active\`:

  curl -s https://$DOMINIO/api/demostracion | LC_ALL=C tr -cd '\\040-\\176\\n'
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

Lo único que eso demuestra es que ALGO contestó por ese nombre. No demuestra que
sea este sitio: un 404 de un comodín de DNS entra por aquí igual que un 502 de
nuestro nginx, y decir «el sitio existe, no es el caso de falta el DNS» —como
decía este guion hasta el 2026-08-05— es concluir de más sobre lo medido, que es
el mismo defecto que arriba, un escalón más abajo.

Un 502 o un 503 justo después del reinicio suele ser la ventana en la que el
servicio todavía está levantando — vuelve a medir en unos segundos:

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

# LOS CINCO CAMINOS SALEN CON 0, Y ES DELIBERADO.
#
# La revisión de seguridad del PR #28 lo señaló: para un envoltorio o un CI,
# «medido y publicado» y «no se pudo medir» son indistinguibles desde el código
# de salida. Es cierto, y aun así no se cambia. La razón:
#
#   el código de salida de este guion responde «¿se DESPLEGÓ bien?»,
#   no «¿está publicado?».
#
# Son dos preguntas distintas y la segunda ocurre DESPUÉS de la primera. Hacer
# que «no se pudo comprobar» saliera con un código de error convertiría un
# despliegue correcto en un fallo aparente, y la reacción natural ante eso es
# volver a lanzar el despliegue. Sobre un VPS con SWAP = 0 —el nuestro—, dos
# despliegues encadenados son un OOM-kill, o sea que la «mejora» tendría un modo
# de fallo peor que el defecto que arregla.
#
# Lo que sí hace falta si algún día esto se automatiza: sacar la MEDICIÓN a su
# propio guion, que entonces sí puede tener un código de salida por estado, y
# dejar aquí solo el despliegue. Mientras lo lea una persona, el mensaje es el
# canal y por eso los cinco dicen exactamente qué se midió y qué no.
