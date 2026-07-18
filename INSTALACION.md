# Manual de instalación — Informes

Este manual sirve para **Windows, Linux y macOS**, con o sin conexión a
internet. Si solo quieres que funcione, lee la sección 1 y para de leer.

> **Documento para el cliente.** Los detalles de arquitectura y los perfiles de
> despliegue están en [`DESPLIEGUE.md`](DESPLIEGUE.md).

---

## 1. Instalación rápida (con el asistente)

El asistente detecta tu equipo, hace tres preguntas y deja el sistema andando.

### Windows

1. Descomprime la carpeta de instalación donde quieras (por ejemplo, el
   Escritorio).
2. Haz **doble clic en `instalar.bat`**.
3. Responde las preguntas.

### macOS

1. Abre la **Terminal** (⌘+Espacio, escribe «Terminal»).
2. Arrastra la carpeta a la ventana después de escribir `cd ` y pulsa Enter.
3. Ejecuta:

   ```bash
   chmod +x instalar.sh
   ./instalar.sh
   ```

### Linux

```bash
cd ruta/a/la/carpeta
chmod +x instalar.sh
./instalar.sh
```

---

Al terminar verás:

```
  ✓ Instalación terminada
  Abre el sistema en:   http://localhost:8003

  Usuario de ejemplo:   admin@co.co
  Contraseña:           123456
```

> ⚠️ **Cambia esa contraseña antes de cargar datos reales.** Es una cuenta de
> ejemplo, conocida y pública.

### Las tres preguntas del asistente

**«¿Cómo se va a usar el sistema?»**

| Respuesta | Cuándo elegirla |
|---|---|
| **Un solo equipo** | Un computador o servidor pequeño. No hay base de datos que administrar. **Es la opción correcta para la mayoría.** |
| **Servidor propio** | La empresa o entidad lo corre en su red, con su PostgreSQL. Varios usuarios simultáneos. |
| **Servidor con internet** | Accesible desde fuera de la red, con correo y enlaces públicos. |

**«¿Cómo prefieres instalarlo?»** — Con **Docker** si está disponible
(recomendado: queda aislado y se actualiza sin tocar el resto del equipo), o
**directamente con Python** si no lo está. El asistente lo detecta y no pregunta
si solo hay una opción viable.

**«¿Cargar los datos iniciales?»** — Di que sí la primera vez: crea el usuario
administrador con el que vas a entrar.

---

## 2. Requisitos

| | Mínimo | Recomendado |
|---|---|---|
| Memoria RAM | 2 GB | 4 GB |
| Disco libre | 5 GB | 20 GB |
| Sistema | Windows 10 · macOS 12 · Linux con kernel 5.x | Cualquiera más reciente |

Y **una** de estas dos:

- **Docker** — [Docker Desktop](https://www.docker.com/products/docker-desktop)
  en Windows y macOS; en Linux, `docker.io` + `docker-compose-plugin`.
- **Python 3.9 o superior** — [python.org/downloads](https://www.python.org/downloads/).

> **Windows:** al instalar Python, marca **«Add python.exe to PATH»** en la
> primera pantalla. Si no lo haces, el instalador no lo encontrará.
>
> **Linux:** después de instalar Docker, ejecuta
> `sudo usermod -aG docker $USER` y **vuelve a iniciar sesión**. Si no, Docker
> pedirá `sudo` en cada comando.

### Comprobar antes de instalar

Para ver qué detecta el asistente sin que toque nada:

```bash
python instalar.py --revisar
```

Sale un resumen del sistema operativo, la versión de Python, si hay Docker y si
hay internet. No escribe ni instala nada.

---

## 3. Instalación sin internet (air-gapped)

Muchas entidades no permiten que sus servidores salgan a la red. La instalación
se hace en dos tiempos: se prepara un **kit** donde sí hay internet y se traslada
en un USB.

### Paso 1 — En un equipo CON internet

Desde la carpeta de instalación:

```bash
python instalar.py --kit
```

Esto descarga todas las dependencias y exporta la imagen Docker a un archivo.
Tarda varios minutos y el resultado pesa **entre 1 y 2 GB**. Al terminar, la
carpeta contiene:

```
  paquetes/                 dependencias de Python
  informes-offline.tar      imagen Docker exportada
```

> El kit debe prepararse en **el mismo sistema operativo y arquitectura** que el
> equipo de destino. Un kit hecho en un Mac con chip Apple no sirve para un
> servidor Linux x86.

### Paso 2 — Copiar

Copia **la carpeta completa** (con `paquetes/` y `informes-offline.tar`) al
equipo aislado. USB, disco externo o lo que permita la política de la entidad.

### Paso 3 — En el equipo aislado

Igual que una instalación normal:

```bash
./instalar.sh          # Linux / macOS
instalar.bat           # Windows (doble clic)
```

El asistente detecta el kit, avisa de que lo va a usar e instala sin tocar la
red. Si no encuentra el kit y tampoco hay internet, **se detiene y explica qué
falta** en vez de fallar a mitad de camino.

---

## 4. Instalación desatendida

Para incluirla en un script de despliegue de la entidad:

```bash
python instalar.py --perfil standalone --modo docker --si
```

| Opción | Valores |
|---|---|
| `--perfil` | `standalone` · `onprem` · `cloud` |
| `--modo` | `docker` · `nativo` |
| `--si` | Acepta los valores por defecto sin preguntar |

Genera un `STORAGE_SECRET` aleatorio y —en `onprem`— una contraseña de base de
datos aleatoria. Quedan en `.env`, con permisos `600` en Linux y macOS.

---

## 5. Uso diario

### Con Docker

Desde la carpeta de instalación:

```bash
# ver qué está pasando
docker compose -f docker-compose.standalone.yml logs -f

# detener
docker compose -f docker-compose.standalone.yml down

# volver a arrancar
docker compose -f docker-compose.standalone.yml up -d
```

En el perfil **servidor propio**, cambia `docker-compose.standalone.yml` por
`docker-compose.yml`.

El sistema **arranca solo al encender el equipo** (`restart: unless-stopped`).
No hay que hacer nada tras un reinicio o un corte de luz.

### Sin Docker

```bash
.venv/bin/python main.py          # Linux / macOS
.venv\Scripts\python.exe main.py  # Windows
```

---

## 6. Respaldos

**Esto no es opcional.** Un sistema de cumplimiento sin respaldos es un riesgo,
no una herramienta.

| Perfil | Qué respaldar |
|---|---|
| Un solo equipo (SQLite) | El volumen `data` completo: contiene la base y los archivos subidos |
| Servidor propio (PostgreSQL) | `pg_dump` de la base + el volumen `uploads` |

Con Docker, en el perfil de un solo equipo:

```bash
docker compose -f docker-compose.standalone.yml stop
docker run --rm -v informes_data:/datos -v "$PWD":/respaldo alpine \
    tar czf /respaldo/respaldo-$(date +%F).tar.gz -C /datos .
docker compose -f docker-compose.standalone.yml start
```

Guarda el archivo resultante **fuera del servidor**. Un respaldo en el mismo
disco que la base no es un respaldo.

---

## 7. Si algo sale mal

| Síntoma | Causa habitual | Solución |
|---|---|---|
| «No se encontró Python» en Windows | No se marcó «Add to PATH» | Reinstalar Python marcando la casilla |
| «Docker está instalado pero el servicio no responde» | Docker Desktop está cerrado | Abrirlo y esperar a que el icono deje de moverse |
| `permission denied` con Docker en Linux | El usuario no está en el grupo `docker` | `sudo usermod -aG docker $USER` y volver a entrar |
| La página no abre en `localhost:8003` | Otro programa usa el puerto | Cambiar `8003:8003` por `8080:8003` en el `docker-compose` |
| «no se pudieron sembrar los datos» | La base aún no aceptaba conexiones | Esperar 30 s y repetir el comando que indica el asistente |
| Instalación offline se detiene | Falta el kit | Ver la sección 3 |

### Empezar de cero

Si la instalación quedó a medias y prefieres reiniciar:

```bash
docker compose -f docker-compose.standalone.yml down -v   # ⚠️ borra los datos
rm .env
./instalar.sh
```

> El `-v` **borra la base de datos y los archivos subidos**. No lo uses en un
> sistema en producción sin haber respaldado antes.

---

## 8. Actualizar a una versión nueva

1. Respalda (sección 6). Siempre, sin excepción.
2. Reemplaza los archivos por los de la versión nueva, **conservando tu `.env`**.
3. Con Docker:

   ```bash
   docker compose -f docker-compose.standalone.yml up -d --build
   ```

Las migraciones de base de datos son **idempotentes**: correrlas dos veces no
rompe nada. El esquema se actualiza solo al arrancar.

En equipos aislados hay que traer un kit nuevo (sección 3); no hay actualización
remota por diseño, porque el servidor no sale a la red.

---

## 9. Soporte

Antes de escribir, ten a mano la salida de:

```bash
python instalar.py --revisar
```

Con eso se resuelve la mayoría de los casos en el primer correo, sin ida y
vuelta.
