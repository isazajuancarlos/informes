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

scp -q "$SITIO" "$DESTINO:/tmp/$DOMINIO.nginx"

ssh "$DESTINO" DOMINIO="$DOMINIO" CORREO="$CORREO" bash -s <<'REMOTO_FIN'
set -euo pipefail

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

echo "==> 4/6  instalar el sitio definitivo con las líneas de TLS activas"
# Se descomentan las cuatro que certbot habría escrito. `sed` sobre estas líneas
# concretas y no sobre «cualquier comentario»: el archivo tiene prosa que explica
# por qué está cada cosa, y descomentarla lo rompería.
sudo sed -E 's|^(\s*)# (ssl_certificate |ssl_certificate_key |include  |ssl_dhparam )|\1\2|' \
     "/tmp/$DOMINIO.nginx" | sudo tee "/etc/nginx/sites-available/$DOMINIO" >/dev/null
if ! sudo grep -qE '^\s*ssl_certificate\s' "/etc/nginx/sites-available/$DOMINIO"; then
    echo "no se descomentó ssl_certificate: el sitio quedaría sin TLS. Se aborta." >&2
    exit 5
fi

echo "==> 5/6  comprobar y recargar"
sudo nginx -t
sudo systemctl reload nginx
REMOTO_FIN

echo "==> 6/6  comprobar contra el dominio real, desde aquí"
# Desde FUERA del servidor: es lo único que prueba que el DNS, el certificado, el
# proxy y el binario funcionan a la vez. Un `curl` a 127.0.0.1 en el servidor
# habría pasado con el sitio sin publicar.
CODIGO="$(curl -s -o /dev/null -w '%{http_code}' --max-time 20 "https://$DOMINIO/")"
TITULO="$(curl -s --max-time 20 "https://$DOMINIO/" | grep -oE '<title>[^<]*</title>' || true)"
REDIR="$(curl -s -o /dev/null -w '%{http_code}' --max-time 20 "http://$DOMINIO/")"
echo "    https://$DOMINIO/  ->  $CODIGO  $TITULO"
echo "    http (debe redirigir) ->  $REDIR"
if [ "$CODIGO" != "200" ] || [ -z "$TITULO" ]; then
    echo "La demostración NO está respondiendo como debe. Mire:" >&2
    echo "    ssh $DESTINO 'sudo journalctl -u siger-demo -n 40 --no-pager'" >&2
    exit 6
fi
echo
echo "Publicada: https://$DOMINIO/"
