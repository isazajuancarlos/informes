#!/usr/bin/env bash
# Despliega la demostración pública en el VPS. Se ejecuta DESDE la máquina de
# desarrollo; hace ssh al servidor.
#
#   ./despliegue/desplegar-demo.sh usuario@192.99.42.108
#
# Qué hace, en orden, y todo idempotente:
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
    echo "Uso: $0 usuario@servidor" >&2
    exit 2
fi

RAIZ="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$RAIZ"
BIN=rust/target/release/siger-api
REMOTO=/opt/siger-demo

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
scp "$BIN" rust/sembrar_demo.py "$DESTINO:$REMOTO/"
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
$CLAVES
SIGER_CLAVE_REPOSO=$REPOSO
ENV
    sudo chmod 600 /etc/siger-demo/siger.env
    sudo chown siger:siger /etc/siger-demo/siger.env
    echo "  secreto de sesión generado"
else
    echo "  /etc/siger-demo/siger.env ya existe: se conserva"
fi

sudo install -m 644 /tmp/siger-demo.service /tmp/siger-demo-sembrar.service \
     /tmp/siger-demo-sembrar.timer /etc/systemd/system/
sudo install -m 644 /tmp/informes.xiliux.com.nginx \
     /etc/nginx/sites-available/informes.xiliux.com

sudo systemctl stop siger-demo.service 2>/dev/null || true
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

cat <<'FIN'

Desplegado. Lo que falta, y NO lo hace este guion:

  1. Registro DNS  A  informes.xiliux.com -> 192.99.42.108
  2. sudo ln -s /etc/nginx/sites-available/informes.xiliux.com \
                /etc/nginx/sites-enabled/
     sudo nginx -t && sudo systemctl reload nginx
  3. sudo certbot --nginx -d informes.xiliux.com     (exige el paso 1 hecho)
FIN
