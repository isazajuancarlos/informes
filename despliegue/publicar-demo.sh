#!/usr/bin/env bash
# Publica la demo YA DESPLEGADA en https://informes.xiliux.com.
#
#   ./despliegue/publicar-demo.sh debian@192.99.42.108
#
# Se ejecuta DESPUÉS de `desplegar-demo.sh` y DESPUÉS de que exista el registro
# DNS. Hace las tres cosas que aquel deja fuera —habilitar el sitio, pedir el
# certificado y recargar nginx— y comprueba el resultado contra el dominio real.
#
# === POR QUÉ ESTO ES UN GUION Y NO TRES LÍNEAS EN UN README ===
#
# Las tres líneas que se documentaban antes NO FUNCIONAN en ese orden, y el fallo
# aparece con el sitio a medio publicar:
#
#   1. `informes.xiliux.com.nginx` trae `listen 443 ssl` con los `ssl_certificate`
#      comentados —los escribe certbot—. Habilitarlo antes de tener certificado
#      hace que `nginx -t` FALLE: «no ssl_certificate is defined for the listen
#      ... ssl directive».
#   2. Pero certbot necesita que alguien conteste en el puerto 80 por ese nombre
#      para resolver el reto HTTP-01, y en este servidor el sitio por defecto
#      responde `444` (cierra sin responder) a todo host desconocido. Sin un
#      sitio habilitado para el dominio, el reto no llega.
#
# O sea: no se puede habilitar sin certificado, y no se puede obtener certificado
# sin habilitar. Se rompe el círculo con un sitio TEMPORAL de solo-HTTP que
# únicamente sirve el reto; se pide el certificado; y entonces sí entra el sitio
# definitivo. Eso son seis pasos con dos recargas de nginx: exactamente la clase
# de secuencia que un humano ejecuta mal a las once de la noche.
set -euo pipefail

DESTINO="${1:-}"
DOMINIO="${DOMINIO:-informes.xiliux.com}"
CORREO="${CORREO:-isazajuancarlos@gmail.com}"
if [ -z "$DESTINO" ]; then
    echo "Uso: $0 usuario@servidor      (por ejemplo: debian@192.99.42.108)" >&2
    exit 2
fi

RAIZ="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SITIO="$RAIZ/despliegue/$DOMINIO.nginx"
[ -f "$SITIO" ] || { echo "no encuentro $SITIO" >&2; exit 2; }

echo "==> 0/6  comprobar el acceso a $DESTINO"
ssh -o ConnectTimeout=10 "$DESTINO" true || {
    echo "No se pudo entrar. Vea el aviso de desplegar-demo.sh." >&2
    exit 3
}

# El DNS se comprueba AQUÍ y no se descubre a medio camino: sin él, certbot falla
# tras haber dejado un sitio temporal habilitado, que es el peor sitio donde
# parar. Y se compara con la dirección del servidor, no solo «resuelve algo»:
# apuntar a otra máquina daría un reto que nunca llega.
echo "==> 1/6  comprobar que $DOMINIO resuelve a este servidor"
IP_SERVIDOR="$(ssh "$DESTINO" 'curl -sf --max-time 10 https://api.ipify.org || hostname -I | awk "{print \$1}"')"
IP_DNS="$(dig +short "$DOMINIO" A | tail -1)"
if [ -z "$IP_DNS" ]; then
    echo "«$DOMINIO» no resuelve todavía. Falta el registro DNS:" >&2
    echo "    A  $DOMINIO  ->  $IP_SERVIDOR      (panel de OVH)" >&2
    echo "No se tocó nada." >&2
    exit 4
fi
if [ "$IP_DNS" != "$IP_SERVIDOR" ]; then
    echo "«$DOMINIO» resuelve a $IP_DNS, pero el servidor es $IP_SERVIDOR." >&2
    echo "El reto de certbot llegaría a otra máquina. No se tocó nada." >&2
    exit 4
fi
echo "    $DOMINIO -> $IP_DNS  ✔"

# El escenario NO va a `/tmp`, por lo mismo que en `desplegar-demo.sh`: `/tmp`
# es 1777 y el nombre es predecible, así que entre el `scp` y el `sudo install`
# hay una ventana en la que otro usuario local puede plantar ahí su archivo — y
# lo que se instala como root es un vhost del nginx COMPARTIDO. Que hoy no sea
# explotable lo sostiene el núcleo del VPS (`fs.protected_regular=2`,
# `fs.protected_symlinks=1`), no este guion.
ssh "$DESTINO" 'mkdir -p ~/.siger-despliegue && chmod 700 ~/.siger-despliegue'
scp -q "$SITIO" "$DESTINO:.siger-despliegue/$DOMINIO.nginx"

ssh "$DESTINO" DOMINIO="$DOMINIO" CORREO="$CORREO" bash -s <<'REMOTO_FIN'
set -euo pipefail
ESC="$HOME/.siger-despliegue"

echo "==> 2/6  sitio TEMPORAL de solo-HTTP, para que el reto de certbot llegue"
# Mínimo a propósito: solo el reto. No redirige a https —todavía no hay https— y
# a lo demás responde 404, para que en la ventana entre este paso y el 5 nadie vea
# una demostración a medio montar.
sudo tee "/etc/nginx/sites-available/$DOMINIO" >/dev/null <<TEMP
server {
    listen 80;
    listen [::]:80;
    server_name $DOMINIO;
    location /.well-known/acme-challenge/ { root /var/www/html; }
    location / { return 404; }
}
TEMP
sudo ln -sfn "/etc/nginx/sites-available/$DOMINIO" "/etc/nginx/sites-enabled/$DOMINIO"
sudo nginx -t
sudo systemctl reload nginx

echo "==> 3/6  pedir el certificado"
# `certonly --webroot` y no `--nginx`: el segundo REESCRIBE el archivo del sitio,
# y el nuestro lleva cabeceras, limit_req y el proxy afinados a mano. Aquí certbot
# solo consigue el certificado; la configuración la ponemos nosotros.
sudo certbot certonly --webroot -w /var/www/html -d "$DOMINIO" \
     --non-interactive --agree-tos -m "$CORREO" --keep-until-expiring

echo "==> 4/6  instalar el sitio definitivo"
# Se comprueba que EXISTE lo que el sitio va a exigir, antes de ponerlo. Si
# faltara, `nginx -t` fallaría con el sitio ya instalado y el temporal ya
# sustituido: el peor sitio donde parar.
for necesario in \
    "/etc/letsencrypt/live/$DOMINIO/fullchain.pem" \
    "/etc/letsencrypt/live/$DOMINIO/privkey.pem" \
    /etc/letsencrypt/options-ssl-nginx.conf \
    /etc/letsencrypt/ssl-dhparams.pem
do
    sudo test -f "$necesario" || { echo "falta $necesario; no se instala el sitio" >&2; exit 5; }
done
# `install` de un archivo a otro, y NUNCA una tubería hacia el archivo del sitio.
# La versión anterior hacía `sed … | tee /etc/nginx/sites-available/…`: el `sed`
# falló, no escribió nada, y `tee` dejó la configuración del sitio VACÍA. Una
# tubería trunca el destino ANTES de saber si el productor va a producir algo.
sudo install -m 644 "$ESC/$DOMINIO.nginx" "/etc/nginx/sites-available/$DOMINIO"
sudo grep -qE '^\s*ssl_certificate\s' "/etc/nginx/sites-available/$DOMINIO" || {
    echo "el sitio instalado no declara ssl_certificate: quedaría sin TLS. Se aborta." >&2
    exit 5
}

echo "==> 5/6  comprobar y recargar"
sudo nginx -t
sudo systemctl reload nginx
REMOTO_FIN

echo "==> 6/6  comprobar contra el dominio real, desde aquí"
# Desde FUERA del servidor: es lo único que prueba que el DNS, el certificado, el
# proxy y el binario funcionan a la vez. Un `curl` a 127.0.0.1 en el servidor
# habría pasado con el sitio sin publicar.
CODIGO="$(curl -s -o /dev/null -w '%{http_code}' --max-time 20 "https://$DOMINIO/")"
# Mismo saneado y misma exigencia que `desplegar-demo.sh`, y por el mismo
# hallazgo: `[ -z "$TITULO" ]` sobre `<title></title>` es falso —la cadena con
# etiquetas nunca está vacía— y cualquier título pasaba, incluido el de la
# página por defecto de nginx. Además el cuerpo lo escribe un servidor remoto:
# sin quitar los caracteres de control, puede reescribir lo que este guion
# parece haber dicho en el terminal.
TITULO="$(curl -s --max-time 20 "https://$DOMINIO/" | grep -oE '<title>[^<]*</title>' \
          | head -1 | sed -E 's|</?title>||g' | LC_ALL=C tr -cd '\040-\176' | cut -c1-80 || true)"
REDIR="$(curl -s -o /dev/null -w '%{http_code}' --max-time 20 "http://$DOMINIO/")"
echo "    https://$DOMINIO/  ->  $CODIGO  $TITULO"
echo "    http (debe redirigir) ->  $REDIR"
# `SIGER$` y NO `^SIGER$`. El ancla de principio es de la era Angular, cuando la
# pantalla se titulaba exactamente «SIGER»; con el panel en `maud` cada página
# pone su nombre delante —hoy `/entrar` es «Entrar · SIGER»—, así que este guion
# habría RECHAZADO una publicación correcta, después de haber instalado el sitio
# y recargado nginx. Un falso negativo que manda a leer el journal de un servicio
# que está bien.
#
# Lo que sigue exigiendo, y por eso no se relaja a `grep SIGER` a secas: que el
# título TERMINE en SIGER. Un `<title>Welcome to nginx!</title>` o el de un
# dominio aparcado no pasan. Es la misma expresión que usa `desplegar-demo.sh`,
# y ahora son la misma en los dos: dos guiones que comprueban lo mismo con dos
# reglas distintas acaban discrepando el día que importa.
if [ "$CODIGO" != "200" ] || ! printf '%s' "$TITULO" | grep -qE 'SIGER$'; then
    echo "La demostración NO está respondiendo como debe. Mire:" >&2
    echo "    ssh $DESTINO 'sudo journalctl -u siger-demo -n 40 --no-pager'" >&2
    exit 6
fi
echo
echo "Publicada: https://$DOMINIO/"
