#!/usr/bin/env python3
"""
Asistente de instalación de Informes — Windows, Linux y macOS.

Guía la instalación paso a paso, detecta lo que hay en la máquina, escribe el
`.env` con secretos generados de verdad y arranca el sistema. No requiere nada
más que Python: usa solo la biblioteca estándar.

    python instalar.py              asistente interactivo (lo normal)
    python instalar.py --kit        prepara un kit offline (lado proveedor)
    python instalar.py --revisar    solo diagnostica, no instala ni escribe nada

Instalación desatendida (para scripts de despliegue del cliente):

    python instalar.py --perfil standalone --modo docker --si

Diseño: si algo falta, el asistente lo dice en español y explica cómo
resolverlo. Nunca deja la instalación a medias en silencio.
"""

import argparse
import os
import platform
import secrets
import shutil
import subprocess
import sys
from pathlib import Path

RAIZ = Path(__file__).resolve().parent
ENV = RAIZ / ".env"
ENV_EJEMPLO = RAIZ / ".env.example"
PUERTO = 8003
PY_MINIMO = (3, 9)

# Un kit offline trae las ruedas de Python aquí y la imagen Docker exportada.
DIR_PAQUETES = RAIZ / "paquetes"
TAR_IMAGEN = RAIZ / "informes-offline.tar"

SO = platform.system()  # 'Windows' | 'Linux' | 'Darwin'
ES_WINDOWS = SO == "Windows"


# --- presentación -------------------------------------------------------------

def _color(codigo: str, texto: str) -> str:
    # En Windows sin terminal ANSI, los códigos ensucian la salida.
    if ES_WINDOWS and not os.environ.get("WT_SESSION"):
        return texto
    return f"\033[{codigo}m{texto}\033[0m"


def titulo(t: str) -> None:
    print()
    print(_color("1;36", f"  {t}"))
    print(_color("36", "  " + "─" * max(len(t), 40)))


def ok(t: str) -> None:
    print(f"  {_color('32', '✓')} {t}")


def aviso(t: str) -> None:
    print(f"  {_color('33', '!')} {t}")


def error(t: str) -> None:
    print(f"  {_color('31', '✗')} {t}")


def paso(t: str) -> None:
    print(f"  {_color('34', '→')} {t}")


def abortar(mensaje: str, ayuda: str = "") -> None:
    print()
    error(mensaje)
    if ayuda:
        print()
        for linea in ayuda.strip().splitlines():
            print(f"    {linea}")
    print()
    sys.exit(1)


# --- preguntas ----------------------------------------------------------------

def preguntar(texto: str, opciones: dict, defecto: str) -> str:
    """Menú numerado. `opciones` es {clave: descripción}."""
    print()
    print(f"  {_color('1', texto)}")
    print()
    claves = list(opciones)
    for i, clave in enumerate(claves, 1):
        marca = "  (por defecto)" if clave == defecto else ""
        print(f"    {i}. {opciones[clave]}{_color('2', marca)}")
    print()
    while True:
        try:
            r = input(f"  Elige [1-{len(claves)}]: ").strip()
        except (EOFError, KeyboardInterrupt):
            print()
            abortar("Instalación cancelada.")
        if not r:
            return defecto
        if r.isdigit() and 1 <= int(r) <= len(claves):
            return claves[int(r) - 1]
        aviso(f"Responde con un número entre 1 y {len(claves)}.")


def confirmar(texto: str, defecto: bool = True) -> bool:
    sufijo = "[S/n]" if defecto else "[s/N]"
    while True:
        try:
            r = input(f"  {texto} {sufijo}: ").strip().lower()
        except (EOFError, KeyboardInterrupt):
            print()
            abortar("Instalación cancelada.")
        if not r:
            return defecto
        if r in ("s", "si", "sí", "y", "yes"):
            return True
        if r in ("n", "no"):
            return False


def pedir_texto(texto: str, defecto: str = "") -> str:
    pista = f" [{defecto}]" if defecto else ""
    try:
        r = input(f"  {texto}{pista}: ").strip()
    except (EOFError, KeyboardInterrupt):
        print()
        abortar("Instalación cancelada.")
    return r or defecto


# --- diagnóstico --------------------------------------------------------------

def ejecutar(cmd: list, **kw) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, capture_output=True, text=True, **kw)


def hay_docker() -> tuple:
    """Devuelve (disponible, comando_compose, motivo_si_no)."""
    if not shutil.which("docker"):
        return False, None, "El comando `docker` no está en el PATH."
    r = ejecutar(["docker", "info"])
    if r.returncode != 0:
        return False, None, "Docker está instalado pero el servicio no responde."
    if ejecutar(["docker", "compose", "version"]).returncode == 0:
        return True, ["docker", "compose"], None
    if shutil.which("docker-compose"):
        return True, ["docker-compose"], None
    return False, None, "Docker existe pero falta Compose."


def hay_internet() -> bool:
    import socket
    for host in ("1.1.1.1", "8.8.8.8"):
        try:
            socket.create_connection((host, 53), timeout=3).close()
            return True
        except OSError:
            continue
    return False


def diagnosticar() -> dict:
    titulo("Revisando la máquina")

    d = {}
    print(f"  Sistema operativo   {SO} {platform.release()} ({platform.machine()})")

    v = sys.version_info
    d["python_ok"] = v >= PY_MINIMO
    linea = f"  Python              {v.major}.{v.minor}.{v.micro}"
    print(linea + ("" if d["python_ok"] else _color("31", f"  (se necesita {PY_MINIMO[0]}.{PY_MINIMO[1]}+)")))

    docker, compose, motivo = hay_docker()
    d["docker"] = docker
    d["compose"] = compose
    print(f"  Docker              {'disponible' if docker else 'no disponible'}"
          + ("" if docker else _color("2", f"  — {motivo}")))

    d["internet"] = hay_internet()
    print(f"  Internet            {'sí' if d['internet'] else 'no'}")

    d["kit_ruedas"] = DIR_PAQUETES.is_dir() and any(DIR_PAQUETES.iterdir())
    d["kit_imagen"] = TAR_IMAGEN.is_file()
    if d["kit_ruedas"] or d["kit_imagen"]:
        partes = []
        if d["kit_imagen"]:
            partes.append(f"imagen Docker ({TAR_IMAGEN.stat().st_size // 1_000_000} MB)")
        if d["kit_ruedas"]:
            partes.append(f"{len(list(DIR_PAQUETES.glob('*')))} paquetes Python")
        print(f"  Kit offline         {', '.join(partes)}")

    if ENV.exists():
        print(f"  Configuración       .env ya existe")
    d["env_existe"] = ENV.exists()

    return d


# --- configuración ------------------------------------------------------------

def escribir_env(perfil: str, valores: dict, comentar: set = frozenset()) -> None:
    """Escribe .env partiendo de .env.example y sustituyendo lo que toca.

    `comentar` son claves que deben quedar DESACTIVADAS aunque vengan en el
    ejemplo. Importa para `standalone`: `config.py` da prioridad a un
    DATABASE_URL explícito sobre el default por perfil, así que dejar el
    `postgresql://…` del ejemplo haría que SQLite no se usara nunca.
    """
    if ENV.exists():
        respaldo = RAIZ / ".env.respaldo"
        shutil.copy2(ENV, respaldo)
        aviso(f"El .env anterior se guardó como {respaldo.name}")

    base = ENV_EJEMPLO.read_text(encoding="utf-8") if ENV_EJEMPLO.exists() else ""
    lineas = []
    puestas = set()

    for linea in base.splitlines():
        limpia = linea.strip()
        if limpia and not limpia.startswith("#") and "=" in limpia:
            clave = limpia.split("=", 1)[0].strip()
            if clave in comentar:
                lineas.append(f"# (desactivado para el perfil {perfil}) {limpia}")
                continue
            if clave in valores:
                lineas.append(f"{clave}={valores[clave]}")
                puestas.add(clave)
                continue
        lineas.append(linea)

    faltantes = {k: v for k, v in valores.items() if k not in puestas}
    if faltantes:
        lineas += ["", "# --- Añadido por el asistente de instalación ---"]
        lineas += [f"{k}={v}" for k, v in faltantes.items()]

    ENV.write_text("\n".join(lineas) + "\n", encoding="utf-8")

    # El .env lleva secretos: en Unix, solo el dueño debe poder leerlo.
    if not ES_WINDOWS:
        os.chmod(ENV, 0o600)

    ok(f"Configuración escrita en .env  (perfil {perfil})")


def recoger_configuracion(perfil: str, interactivo: bool) -> tuple:
    """Devuelve (valores_a_escribir, claves_a_comentar)."""
    valores = {
        "DEPLOYMENT_PROFILE": perfil,
        # Se genera de verdad: es la clave que firma las sesiones.
        "STORAGE_SECRET": secrets.token_hex(32),
    }
    comentar = set()

    if perfil == "standalone":
        valores["OFFLINE"] = "true"
        # SQLite se resuelve solo, pero SOLO si no hay DATABASE_URL definido:
        # config.py le da prioridad al explícito. Hay que desactivar el del
        # ejemplo o la instalación más simple apuntaría a un PostgreSQL que no
        # existe. DB_PASSWORD tampoco pinta nada aquí.
        comentar.update({"DATABASE_URL", "DB_PASSWORD"})

    elif perfil == "onprem":
        valores["OFFLINE"] = "true"
        clave_db = secrets.token_urlsafe(24)
        if interactivo and confirmar(
            "¿Usar el PostgreSQL que trae el instalador (recomendado)?", True
        ):
            valores["DB_PASSWORD"] = clave_db
            valores["DATABASE_URL"] = f"postgresql://informes:{clave_db}@db/informes"
        elif interactivo:
            host = pedir_texto("Servidor PostgreSQL", "localhost")
            base = pedir_texto("Nombre de la base de datos", "informes")
            usuario = pedir_texto("Usuario", "informes")
            clave = pedir_texto("Contraseña")
            valores["DATABASE_URL"] = f"postgresql://{usuario}:{clave}@{host}/{base}"
        else:
            valores["DB_PASSWORD"] = clave_db
            valores["DATABASE_URL"] = f"postgresql://informes:{clave_db}@db/informes"

    else:  # cloud
        valores["OFFLINE"] = "false"
        if interactivo:
            url = pedir_texto("URL pública del sistema", f"http://localhost:{PUERTO}")
            valores["APP_BASE_URL"] = url
            if confirmar("¿Configurar el envío de correo ahora?", False):
                valores["SMTP_HOST"] = pedir_texto("Servidor SMTP", "smtp.gmail.com")
                valores["SMTP_PORT"] = pedir_texto("Puerto SMTP", "587")
                valores["SENDER_EMAIL"] = pedir_texto("Correo remitente")
                valores["SENDER_PASSWORD"] = pedir_texto("Contraseña de aplicación")

    return valores, comentar


# --- instalación --------------------------------------------------------------

def instalar_docker(perfil: str, compose: list, diag: dict, asumir_si: bool) -> None:
    titulo("Instalando con Docker")

    archivo = "docker-compose.standalone.yml" if perfil == "standalone" else "docker-compose.yml"
    if not (RAIZ / archivo).exists():
        abortar(f"No se encuentra {archivo} en {RAIZ}")

    if diag["kit_imagen"]:
        paso("Cargando la imagen desde el kit offline (puede tardar unos minutos)…")
        r = subprocess.run(["docker", "load", "-i", str(TAR_IMAGEN)])
        if r.returncode != 0:
            abortar("No se pudo cargar la imagen del kit offline.")
        ok("Imagen cargada")
    elif not diag["internet"]:
        abortar(
            "No hay internet y no se encontró el kit offline.",
            f"""
Esta máquina no puede descargar la imagen. Necesitas el kit offline:

  1. En una máquina CON internet, dentro de esta misma carpeta:
         python instalar.py --kit
  2. Copia la carpeta completa (con {TAR_IMAGEN.name}) por USB a esta máquina.
  3. Vuelve a ejecutar este instalador.
""",
        )

    paso("Levantando los servicios…")
    r = subprocess.run(compose + ["-f", archivo, "up", "-d"], cwd=RAIZ)
    if r.returncode != 0:
        abortar(
            "Docker Compose no pudo levantar los servicios.",
            f"Revisa el detalle con:  {' '.join(compose)} -f {archivo} logs",
        )
    ok("Servicios en marcha")

    if asumir_si or confirmar("¿Cargar los datos iniciales (usuarios de ejemplo)?", True):
        paso("Sembrando datos iniciales…")
        r = subprocess.run(compose + ["-f", archivo, "exec", "-T", "app",
                                      "python", "seed_data.py"], cwd=RAIZ)
        if r.returncode == 0:
            ok("Datos iniciales cargados")
        else:
            aviso("No se pudieron sembrar ahora. Hazlo después con:")
            print(f"      {' '.join(compose)} -f {archivo} exec app python seed_data.py")


def instalar_nativo(perfil: str, diag: dict) -> None:
    titulo("Instalando directamente con Python")

    if not diag["python_ok"]:
        abortar(
            f"Se necesita Python {PY_MINIMO[0]}.{PY_MINIMO[1]} o superior.",
            "Descárgalo en https://www.python.org/downloads/",
        )

    entorno = RAIZ / ".venv"
    if not entorno.exists():
        paso("Creando el entorno virtual…")
        r = subprocess.run([sys.executable, "-m", "venv", str(entorno)])
        if r.returncode != 0:
            abortar("No se pudo crear el entorno virtual (.venv).")
    ok("Entorno virtual listo")

    pip = entorno / ("Scripts" if ES_WINDOWS else "bin") / ("pip.exe" if ES_WINDOWS else "pip")
    python = entorno / ("Scripts" if ES_WINDOWS else "bin") / ("python.exe" if ES_WINDOWS else "python")

    cmd = [str(pip), "install", "-r", "requirements.txt"]
    if diag["kit_ruedas"]:
        paso("Instalando dependencias desde el kit offline…")
        cmd = [str(pip), "install", "--no-index", "--find-links", str(DIR_PAQUETES),
               "-r", "requirements.txt"]
    elif not diag["internet"]:
        abortar(
            "No hay internet y no se encontró la carpeta de paquetes offline.",
            f"""
Necesitas el kit offline:

  1. En una máquina CON internet, dentro de esta misma carpeta:
         python instalar.py --kit
  2. Copia la carpeta completa (con {DIR_PAQUETES.name}/) por USB a esta máquina.
  3. Vuelve a ejecutar este instalador.
""",
        )
    else:
        paso("Descargando e instalando dependencias (puede tardar unos minutos)…")

    if subprocess.run(cmd, cwd=RAIZ).returncode != 0:
        abortar("Falló la instalación de dependencias.")
    ok("Dependencias instaladas")

    if perfil != "standalone":
        aviso("Este perfil necesita un PostgreSQL en marcha y accesible.")
        aviso("Comprueba DATABASE_URL en .env antes de arrancar.")

    entorno_hijo = {**os.environ, "DEPLOYMENT_PROFILE": perfil, "PYTHONPATH": str(RAIZ)}
    preparar_base_de_datos(python, entorno_hijo)

    print()
    ok("Instalación terminada. Para arrancar el sistema:")
    print()
    if ES_WINDOWS:
        print(f"      .venv\\Scripts\\python.exe main.py")
    else:
        print(f"      .venv/bin/python main.py")


#: Los pasos de base de datos, EN ORDEN. El orden es el arreglo, no un detalle.
PASOS_BASE_DE_DATOS = [
    ("Creando el esquema de la base de datos…", ["-m", "app.init_db"],
     "No se pudo crear el esquema de la base de datos.",
     "Con el perfil `standalone` esto casi siempre es un problema de\n"
     "permisos de escritura en la carpeta `storage/`.\n"
     "Con `onprem` o `cloud`, revisa DATABASE_URL en el archivo .env."),
    ("Sembrando los datos iniciales…", ["seed_data.py"],
     "No se pudieron crear los usuarios iniciales.",
     "El sistema quedó instalado pero SIN NINGÚN USUARIO, así que no se\n"
     "puede entrar. Reintenta ejecutando `seed_data.py` a mano."),
]


def preparar_base_de_datos(python, entorno, ejecutar=None) -> None:
    """Crea el esquema y siembra los datos, en ese orden, y ABORTA si algo falla.

    Las dos cosas que arregla, y que estuvieron rotas a la vez:

    **El orden.** `seed_data.py` inserta filas; si las tablas no existen todavía,
    falla. En una instalación de verdad NUNCA existen —es la primera vez que se
    toca esa base—, así que el asistente fallaba SIEMPRE. Corriendo los comandos
    a mano, en cambio, funcionaba: quien lo probó creó el esquema antes sin darse
    cuenta de que el asistente no lo hacía.

    **Que fallar no fallaba.** El fallo se imprimía como un aviso de pasada, el
    asistente se declaraba «terminado», devolvía 0 y a continuación anunciaba
    «Usuario de ejemplo: admin@co.co / Contraseña: 123456». El cliente seguía el
    manual, leía que todo había ido bien, y no podía entrar. Un asistente que
    entrega un sistema sin usuarios tiene que decirlo y salir con error.

    `ejecutar` se inyecta para poder probar el orden y el aborto sin montar un
    entorno virtual de verdad.
    """
    ejecutar = ejecutar or (lambda cmd: subprocess.run(cmd, cwd=RAIZ, env=entorno))
    for descripcion, argumentos, fallo, ayuda in PASOS_BASE_DE_DATOS:
        paso(descripcion)
        if ejecutar([str(python), *argumentos]).returncode != 0:
            abortar(fallo, ayuda)
    ok("Base de datos lista")


# --- kit offline (lado proveedor) ---------------------------------------------

def preparar_kit() -> None:
    titulo("Preparando el kit de instalación offline")
    print("  Esto se ejecuta en una máquina CON internet. El resultado es esta")
    print("  misma carpeta, lista para copiarse por USB a un equipo aislado.")

    if not hay_internet():
        abortar("Esta máquina tampoco tiene internet. El kit se prepara donde sí hay.")

    paso("Descargando las dependencias de Python…")
    DIR_PAQUETES.mkdir(exist_ok=True)
    r = subprocess.run([sys.executable, "-m", "pip", "download",
                        "-r", "requirements.txt", "-d", str(DIR_PAQUETES)], cwd=RAIZ)
    if r.returncode != 0:
        aviso("No se pudieron descargar todas las ruedas de Python.")
        aviso("El kit servirá para Docker, pero no para instalación nativa.")
    else:
        ok(f"{len(list(DIR_PAQUETES.glob('*')))} paquetes descargados en {DIR_PAQUETES.name}/")

    docker, compose, motivo = hay_docker()
    if not docker:
        aviso(f"Sin Docker aquí ({motivo}); no se exporta la imagen.")
    else:
        paso("Construyendo la imagen Docker…")
        if subprocess.run(compose + ["build"], cwd=RAIZ).returncode != 0:
            abortar("Falló la construcción de la imagen.")

        paso("Exportando la imagen a un archivo (esto tarda y pesa)…")
        imagenes = ejecutar(["docker", "images", "--format", "{{.Repository}}:{{.Tag}}"])
        candidatas = [i for i in imagenes.stdout.split() if "informes" in i.lower()]
        candidatas.append("postgres:16-alpine")
        r = subprocess.run(["docker", "save", "-o", str(TAR_IMAGEN)] + candidatas, cwd=RAIZ)
        if r.returncode != 0:
            abortar("No se pudo exportar la imagen Docker.")
        ok(f"Imagen exportada: {TAR_IMAGEN.name} ({TAR_IMAGEN.stat().st_size // 1_000_000} MB)")

    print()
    ok("Kit listo. Copia esta carpeta completa al equipo de destino y allí ejecuta:")
    print()
    print("      python instalar.py")


# --- cierre -------------------------------------------------------------------

def resumen_final(perfil: str, modo: str) -> None:
    titulo("Listo")
    print(f"  Abre el sistema en:   {_color('1;32', f'http://localhost:{PUERTO}')}")
    print()
    print("  Usuario de ejemplo:   admin@co.co")
    print("  Contraseña:           123456")
    print()
    aviso("Cambia esa contraseña antes de usarlo con datos reales.")

    if modo == "docker":
        archivo = "docker-compose.standalone.yml" if perfil == "standalone" else "docker-compose.yml"
        print()
        print(_color("2", "  Comandos útiles:"))
        print(_color("2", f"    Ver registros    docker compose -f {archivo} logs -f"))
        print(_color("2", f"    Detener          docker compose -f {archivo} down"))
        print(_color("2", f"    Reiniciar        docker compose -f {archivo} restart"))
    print()


def main() -> None:
    p = argparse.ArgumentParser(
        description="Asistente de instalación de Informes.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument("--kit", action="store_true",
                   help="prepara un kit offline (se ejecuta donde SÍ hay internet)")
    p.add_argument("--revisar", action="store_true",
                   help="solo diagnostica la máquina; no instala ni escribe nada")
    p.add_argument("--perfil", choices=["standalone", "onprem", "cloud"],
                   help="perfil de despliegue (omite la pregunta)")
    p.add_argument("--modo", choices=["docker", "nativo"],
                   help="forma de instalar (omite la pregunta)")
    p.add_argument("--si", action="store_true",
                   help="acepta los valores por defecto sin preguntar")
    args = p.parse_args()

    print()
    print(_color("1;36", "  ╭────────────────────────────────────────────╮"))
    print(_color("1;36", "  │   Informes — asistente de instalación       │"))
    print(_color("1;36", "  ╰────────────────────────────────────────────╯"))

    if args.kit:
        preparar_kit()
        return

    diag = diagnosticar()

    if args.revisar:
        print()
        ok("Diagnóstico terminado. No se modificó nada.")
        print()
        return

    interactivo = not args.si

    # 1. Perfil
    perfil = args.perfil
    if not perfil:
        if not interactivo:
            perfil = "standalone"
        else:
            perfil = preguntar(
                "¿Cómo se va a usar el sistema?",
                {
                    "standalone": "Un solo equipo, sin base de datos que administrar (lo más simple)",
                    "onprem": "Servidor propio de la empresa o entidad, en su red",
                    "cloud": "Servidor con internet, accesible desde fuera",
                },
                "standalone",
            )

    # 2. Modo
    modo = args.modo
    if not modo:
        if diag["docker"] and not interactivo:
            modo = "docker"
        elif not diag["docker"]:
            modo = "nativo"
            if interactivo:
                aviso("Docker no está disponible; se instalará directamente con Python.")
        else:
            modo = preguntar(
                "¿Cómo prefieres instalarlo?",
                {
                    "docker": "Con Docker — aislado y fácil de actualizar (recomendado)",
                    "nativo": "Directamente con Python — sin Docker",
                },
                "docker",
            )

    if modo == "docker" and not diag["docker"]:
        abortar(
            "Se pidió instalar con Docker, pero Docker no está disponible.",
            """
Instálalo según tu sistema:

  Windows / macOS   https://www.docker.com/products/docker-desktop
  Linux (Debian)    sudo apt install docker.io docker-compose-plugin
  Linux (RHEL)      sudo dnf install docker docker-compose-plugin

En Linux, después:  sudo systemctl enable --now docker
Y añade tu usuario:  sudo usermod -aG docker $USER   (requiere volver a entrar)
""",
        )

    # 3. Configuración
    titulo("Configuración")
    if diag["env_existe"] and interactivo and not confirmar(
        "Ya existe un .env. ¿Rehacerlo? (se guardará una copia)", False
    ):
        ok("Se conserva el .env actual")
    else:
        valores, comentar = recoger_configuracion(perfil, interactivo)
        escribir_env(perfil, valores, comentar)

    # 4. Instalar
    if modo == "docker":
        instalar_docker(perfil, diag["compose"], diag, args.si)
    else:
        instalar_nativo(perfil, diag)

    resumen_final(perfil, modo)


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        print()
        print()
        abortar("Instalación cancelada por el usuario.")
