"""Siembra una base de demostración para VER el sistema funcionando.

    python rust/sembrar_demo.py            # crea rust/demo/siger.db
    python rust/sembrar_demo.py --ruta X   # o donde se le diga

Es para mirar, no para instalar: los datos son ficticios y las contraseñas están
escritas aquí. Una instalación de verdad la hace el instalador, que genera el
secreto de sesión y pide las credenciales.

# Por qué las contraseñas las cifra el BINARIO y no este guion

`siger-api hashear` usa el mismo Argon2, con los mismos parámetros, que después
verifica el acceso. Un hash producido por otra herramienta con otra
configuración deja al usuario fuera del sistema, y el fallo aparece lejos de su
causa: el instalador termina bien, anuncia unas credenciales, y no se puede
entrar con ellas. Eso ya pasó una vez en la versión de Python.

# Por qué las entregas NO se escriben aquí

Antes este guion insertaba ocho entregas a mano. Tenía dos defectos y el segundo
era grave:

1. Los cuatro informes llevaban la MISMA recurrencia —día 15 de los doce meses—,
   así que el «trimestral» no era trimestral y el motor no se veía por ningún
   lado. Se enseñaba una tabla, no un sistema.
2. Al escribir las entregas a mano, la demostración salía igual de bonita
   **aunque el planificador estuviera roto**. Una demo que no depende del motor
   no demuestra el motor.

Ahora se siembran los INFORMES —uno por cada modo de recurrencia que el motor
sabe leer— y las entregas las genera el propio binario al arrancar, con su
planificador y su calendario de festivos. Después se comprueba que cada modo de
calendario produjo al menos una: si no, este guion FALLA en vez de dejar una
demostración vacía y silenciosa.

# Las fechas se calculan desde HOY

Una entrega vencida hace tres días y otra que vence mañana enseñan la columna de
urgencia haciendo su trabajo. Con fechas fijas, la demostración envejece: a las
dos semanas todo aparece vencido y la pantalla deja de contar lo que quería.
"""
from __future__ import annotations

import argparse
import json
import shutil
import sqlite3
import subprocess
import sys
import time
from datetime import date, datetime, timedelta
from pathlib import Path

RAIZ = Path(__file__).resolve().parent.parent
BINARIO_POR_DEFECTO = RAIZ / "rust" / "target" / "release" / "siger-api"

# Se fija en `main()`. Es global porque `hashear()` lo usa desde el fondo de la
# llamada; pasarlo por cinco funciones para que solo lo use una no lo aclararía.
BINARIO = BINARIO_POR_DEFECTO

CLAVES = {
    "admin@co.co": "admin",
    "operador@co.co": "operador",
    "tercero@co.co": "tercero",
}

# Cuánto se espera a que el planificador termine su primera pasada. Es generoso
# a propósito: en una máquina cargada la migración más el barrido tardan, y un
# tiempo corto convertiría «lento» en «roto».
ESPERA_PLANIFICADOR = 45.0

HOY = date.today()


def regla(**campos) -> str:
    """La recurrencia, en el MISMO formato compacto que guarda el sistema.

    Sin claves nulas de relleno: `ReglaJson` omite al serializar todo campo
    vacío, y una regla sembrada con veinte `null` sería una segunda forma de
    escribir lo mismo — justo lo que ese `skip_serializing_if` existe para
    evitar.
    """
    return json.dumps(campos, ensure_ascii=False, separators=(",", ":"))


def dias_atras(n: int) -> str:
    return (HOY - timedelta(days=n)).isoformat()


# Un informe por MODO, que es el punto: los seis que `compat.rs` sabe leer.
# La vigencia de cada uno se elige para que la ventana produzca un puñado de
# entregas, no cientos — el planificador barre hasta 730 días hacia atrás.
#
# (id, nombre, descripción, regla, modo_para_el_informe, espera_entregas)
INFORMES = [
    (
        1,
        "RIPS mensual",
        "Registro individual de prestación de servicios. Vence el 15 de cada mes; "
        "si cae en festivo, corre al siguiente hábil.",
        regla(modo="ESPECIFICO", fecha_inicio=dias_atras(150),
              meses=[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12], dia_corte=15,
              mover_habil=True),
        "ESPECIFICO (mensual)",
        True,
    ),
    (
        2,
        "Reporte de calidad trimestral",
        "Indicadores de calidad. Último día de marzo, junio, septiembre y "
        "diciembre, con entrega anticipada al hábil anterior.",
        regla(modo="ESPECIFICO", fecha_inicio=dias_atras(400),
              meses=[3, 6, 9, 12], es_ultimo_dia=True, mover_habil_anterior=True),
        "ESPECIFICO (trimestral)",
        True,
    ),
    (
        3,
        "Informe de cartera quincenal",
        "Estado de cartera por edades. Cada 15 días desde el inicio del contrato.",
        regla(modo="CICLICO", fecha_inicio=dias_atras(90), intervalo=15),
        "CICLICO",
        True,
    ),
    (
        4,
        "Novedades de afiliación",
        "Movimientos de afiliados. Lunes y jueves de cada semana.",
        regla(modo="SEMANAL", fecha_inicio=dias_atras(45), dias_semana=[1, 4]),
        "SEMANAL",
        True,
    ),
    (
        5,
        "Monitoreo diario de disponibilidad",
        "Estado del servicio, todos los días desde el inicio del contrato.",
        # Era el ejemplo del modo HORARIO, que se retiró: la tabla de entregas
        # guarda una FECHA y su clave única es (informe, fecha_nominal), así que
        # «cada cuatro horas» producía UNA entrega al día en silencio. Lo diario
        # es lo que aquel modo hacía de verdad, y así se llama.
        regla(modo="CICLICO", fecha_inicio=dias_atras(6), intervalo=1),
        "CICLICO (diario)",
        True,
    ),
    (
        6,
        "Circular 016 — anexo técnico",
        "Fechas fijadas por la Superintendencia en su circular, sin periodicidad.",
        regla(modo="FECHAS", fecha_inicio=dias_atras(60),
              fechas=[dias_atras(35), dias_atras(4),
                      (HOY + timedelta(days=12)).isoformat()]),
        "FECHAS",
        True,
    ),
    (
        7,
        "Requerimiento de la Superintendencia",
        "No tiene calendario: nace cuando la entidad lo pide y se cuenta desde "
        "ahí. Son los informes «INMEDIATO» del catálogo migrado.",
        regla(modo="A_SOLICITUD", fecha_inicio=dias_atras(60)),
        "A_SOLICITUD",
        # A propósito: este modo NO fija días, así que el planificador no le crea
        # ninguna. Que aquí se espere CERO es la afirmación, no un descuido.
        False,
    ),
]


def hashear(clave: str) -> str:
    salida = subprocess.run(
        [str(BINARIO), "hashear", clave], capture_output=True, text=True, check=True
    )
    return salida.stdout.strip()


def abrir(bd: Path) -> sqlite3.Connection:
    # `timeout`: el servidor puede tener la base tomada mientras migra o barre.
    # Sin esto, la primera lectura simultánea muere con «database is locked».
    return sqlite3.connect(bd, timeout=30)


def arrancar_binario(destino: Path, segundos: float) -> subprocess.Popen:
    """Arranca el servidor en `destino`. Migra y planifica al arrancar."""
    return subprocess.Popen(
        [str(BINARIO)],
        cwd=destino,
        env={
            "SIGER_SECRETO_SESION": "solo-para-sembrar-no-sirve-para-servir-x",
            "DATABASE_URL": "sqlite://siger.db",
            "PORT": "0",
            "PATH": "/usr/bin:/bin",
        },
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )


def esperar(proc: subprocess.Popen, condicion, segundos: float) -> bool:
    """Espera a que `condicion()` sea cierta, vigilando que el proceso siga vivo.

    Si el binario se muere, se deja de esperar de inmediato: aguardar 45 segundos
    a un proceso que ya terminó convierte un error legible en un tiempo agotado
    que no dice nada.
    """
    limite = time.monotonic() + segundos
    while time.monotonic() < limite:
        if condicion():
            return True
        if proc.poll() is not None:
            return False
        time.sleep(0.3)
    return condicion()


def parar(proc: subprocess.Popen) -> None:
    proc.terminate()
    try:
        proc.wait(timeout=10)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait(timeout=10)


def crear_esquema(destino: Path) -> Path:
    """Deja la base con su esquema aplicado. El binario migra al arrancar."""
    bd = destino / "siger.db"
    if bd.exists():
        return bd

    print("La base no existe: se arranca el binario un momento para que la cree…")
    proc = arrancar_binario(destino, 20)
    vivo = esperar(proc, lambda: bd.exists() and tablas(bd) >= {"usuarios", "informes"}, 20)
    parar(proc)

    # Y se COMPRUEBA que el esquema quedó. Dar por hecho que salió bien porque el
    # proceso arrancó es la clase de suposición que convierte un fallo de
    # migración en un error incomprensible tres pasos más adelante.
    if not vivo or not bd.exists():
        salida = proc.stderr.read() if proc.stderr else ""
        raise SystemExit(
            f"El binario no creó {bd}. Revise que ./rust/construir.sh terminó "
            f"bien.\n{salida}"
        )
    faltan = {"usuarios", "informes", "entregas", "participacion"} - tablas(bd)
    if faltan:
        raise SystemExit(f"El esquema quedó incompleto; faltan: {sorted(faltan)}")
    return bd


def tablas(bd: Path) -> set[str]:
    with abrir(bd) as c:
        return {f[0] for f in c.execute(
            "SELECT name FROM sqlite_master WHERE type='table'")}


def comprobar_que_es_de_demostracion(bd: Path, forzar: bool) -> None:
    """No se vacía una base que no sea la de demostración.

    Este guion empieza borrando seis tablas. Apuntarlo por descuido a la base de
    un cliente —un `--ruta` copiado de otra terminal— la dejaría sin datos y sin
    aviso. La señal de que una base es la de demostración son SUS usuarios: si
    hay alguno que no salga de este archivo, no es la de demostración.
    """
    if forzar:
        return
    with abrir(bd) as c:
        try:
            correos = {f[0] for f in c.execute("SELECT email FROM usuarios")}
        except sqlite3.OperationalError:
            return  # sin tabla `usuarios` todavía: base recién migrada
    ajenos = correos - set(CLAVES)
    if ajenos:
        raise SystemExit(
            f"{bd} tiene usuarios que NO son de la demostración: "
            f"{sorted(ajenos)[:5]}\n"
            "Este guion vacía las tablas antes de sembrar. Si de verdad quiere "
            "borrar esa base, pídalo con --forzar."
        )


def sembrar_catalogo(bd: Path) -> None:
    """Usuarios, terceros e informes. Las entregas las genera el binario."""
    c = abrir(bd)
    c.executescript(
        "DELETE FROM entregas; DELETE FROM participacion; DELETE FROM informes;"
        "DELETE FROM usuarios; DELETE FROM stakeholders; DELETE FROM auditoria;"
    )

    c.execute("INSERT INTO stakeholders (id, nombre) VALUES (1, 'Superintendencia de Salud')")
    c.execute("INSERT INTO stakeholders (id, nombre) VALUES (2, 'EPS Contratante')")

    usuarios = [
        (1, "Administradora", "admin@co.co", "ADMIN", None),
        (2, "Operadora", "operador@co.co", "OPERADOR", None),
        (3, "Tercero externo", "tercero@co.co", "TERCERO", 1),
    ]
    for uid, nombre, email, rol, stake in usuarios:
        c.execute(
            "INSERT INTO usuarios (id, nombre, email, clave_hash, rol, activo, stakeholder_id)"
            " VALUES (?,?,?,?,?,1,?)",
            (uid, nombre, email, hashear(CLAVES[email]), rol, stake),
        )

    for iid, nombre, desc, recurrencia, _modo, _espera in INFORMES:
        c.execute(
            "INSERT INTO informes (id, nombre, descripcion, recurrencia) VALUES (?,?,?,?)",
            (iid, nombre, desc, recurrencia),
        )

    # El tercero es DESTINO de dos informes: por eso ve esos y no los demás.
    c.execute("INSERT INTO participacion (informe_id, stakeholder_id, rol) VALUES (1,1,'DESTINO')")
    c.execute("INSERT INTO participacion (informe_id, stakeholder_id, rol) VALUES (6,1,'DESTINO')")
    c.execute("INSERT INTO participacion (informe_id, stakeholder_id, rol) VALUES (7,1,'ORIGEN')")
    # Quién ejecuta: sin esto las entregas nacen sin dueño (legítimo, pero la
    # demostración quiere enseñar la bandeja de alguien).
    for iid in (1, 2, 3, 4, 5, 6):
        c.execute(
            "INSERT INTO participacion (informe_id, usuario_id, rol) VALUES (?,2,'EJECUTA')",
            (iid,),
        )
    c.commit()
    c.close()


def generar_entregas(destino: Path, bd: Path) -> dict[int, int]:
    """Arranca el binario para que su PLANIFICADOR genere las entregas.

    Devuelve cuántas creó por informe. Si algún modo de calendario se queda en
    cero, esto no lo disimula: el llamante falla.
    """
    esperados = [iid for iid, _n, _d, _r, _m, espera in INFORMES if espera]

    def listo() -> bool:
        try:
            with abrir(bd) as c:
                hechos = {f[0] for f in c.execute(
                    "SELECT DISTINCT informe_id FROM entregas")}
        except sqlite3.OperationalError:
            return False
        return all(i in hechos for i in esperados)

    print("Se arranca el binario para que el planificador genere las entregas…")
    proc = arrancar_binario(destino, ESPERA_PLANIFICADOR)
    completo = esperar(proc, listo, ESPERA_PLANIFICADOR)
    salida = ""
    if proc.poll() is not None and proc.stderr:
        salida = proc.stderr.read()
    parar(proc)

    with abrir(bd) as c:
        cuenta = dict(c.execute(
            "SELECT informe_id, COUNT(*) FROM entregas GROUP BY informe_id"))

    if not completo:
        vacios = [
            f"#{iid} {nombre} [{modo}]"
            for iid, nombre, _d, _r, modo, espera in INFORMES
            if espera and cuenta.get(iid, 0) == 0
        ]
        raise SystemExit(
            "El planificador no generó entregas para:\n  "
            + "\n  ".join(vacios)
            + "\n\nLa demostración quedaría vacía justo en lo que quiere enseñar, "
            "así que se para aquí en vez de seguir.\n"
            + (f"\nSalida del binario:\n{salida}" if salida else "")
        )
    return cuenta


def entrega_a_solicitud(bd: Path) -> None:
    """La única entrega que NO nace del calendario, porque su modo no fija días.

    Se escribe a mano justo porque es lo que hace el modo `A_SOLICITUD`: la
    obligación aparece cuando la entidad la pide. Sin esta fila el informe #7
    saldría en el catálogo sin una sola entrega y parecería roto en vez de
    parecer lo que es.
    """
    pedido = HOY - timedelta(days=5)
    limite = pedido + timedelta(days=15)
    with abrir(bd) as c:
        c.execute(
            "INSERT OR IGNORE INTO entregas (informe_id, usuario_id, fecha_nominal,"
            " fecha_limite, tipo_ajuste, estado) VALUES (7, 2, ?, ?, 'NINGUNO', 'PENDIENTE')",
            (pedido.isoformat(), limite.isoformat()),
        )
        c.commit()


def pintar_estados(bd: Path, almacen: Path) -> dict[str, int]:
    """Da historia a lo ya vencido: cumplido, extemporáneo o incumplido.

    Determinista y no aleatorio: una demostración que cambia de aspecto en cada
    siembra es imposible de guionizar. Y el archivo se escribe DE VERDAD para
    cada entrega marcada como cumplida — dejar el nombre en la fila sin el
    archivo detrás produce una descarga que falla delante del cliente.
    """
    c = abrir(bd)
    vencidas = list(c.execute(
        "SELECT e.id, e.informe_id, e.fecha_limite, i.nombre FROM entregas e"
        " JOIN informes i ON i.id = e.informe_id"
        " WHERE e.fecha_limite < ? ORDER BY e.fecha_limite",
        (HOY.isoformat(),),
    ))

    conteo = {"CUMPLIDO": 0, "EXTEMPORANEO": 0, "INCUMPLIDO": 0, "PENDIENTE": 0}
    for n, (eid, iid, limite, nombre) in enumerate(vencidas):
        fl = date.fromisoformat(limite)
        if n % 7 == 2:
            # Se deja PENDIENTE con el límite ya pasado, a propósito. Es el
            # estado «vencida sin registrar», y es el que de verdad ocurre: nada
            # en el sistema marca INCUMPLIDO solo, lo marca una persona. Pintarlo
            # todo dejaba el contador de vencidas en CERO —justo la columna de
            # urgencia que la demostración existe para enseñar— y dejaba a la
            # alerta de incumplimiento sin nada que enviar, porque busca
            # exactamente esto: PENDIENTE con fecha_limite pasada.
            conteo["PENDIENTE"] += 1
            continue
        if n % 7 == 3:
            estado, real = "INCUMPLIDO", None
        elif n % 7 == 4:
            estado, real = "EXTEMPORANEO", fl + timedelta(days=2)
        else:
            estado, real = "CUMPLIDO", fl - timedelta(days=1)

        archivo = None
        if real is not None:
            archivo = f"{nombre.split(' ')[0].lower()}_{fl.isoformat()}.pdf"
            carpeta = almacen / str(eid)
            carpeta.mkdir(parents=True, exist_ok=True)
            (carpeta / archivo).write_text(
                f"Contenido de ejemplo de {archivo}\n"
                f"Informe: {nombre}\nLímite: {limite}\n",
                encoding="utf-8",
            )
        c.execute(
            "UPDATE entregas SET estado = ?, fecha_entrega_real = ?, archivo_nombre = ?"
            " WHERE id = ?",
            (estado, real.isoformat() if real else None, archivo, eid),
        )
        conteo[estado] += 1

    c.commit()
    c.close()
    return conteo


def sembrar(destino: Path, forzar: bool) -> None:
    almacen = destino / "storage" / "entregas"
    bd = crear_esquema(destino)
    comprobar_que_es_de_demostracion(bd, forzar)
    sembrar_catalogo(bd)
    cuenta = generar_entregas(destino, bd)
    entrega_a_solicitud(bd)
    conteo = pintar_estados(bd, almacen)

    modos = {m.split(" ")[0] for _i, _n, _d, _r, m, _e in INFORMES}
    print(f"\nSembrado en {bd}")
    print(f"  {len(INFORMES)} informes · los {len(modos)} modos que el motor lee, "
          f"con dos variantes de los dos más ricos\n")
    for iid, nombre, _d, _r, modo, espera in INFORMES:
        n = cuenta.get(iid, 0) if espera else 1
        nota = "" if espera else "  (creada a mano: su modo no fija días)"
        print(f"  [{modo:<24}] {nombre:<34} {n:>3} entregas{nota}")
    print(f"\n  {conteo['CUMPLIDO']} cumplidas · {conteo['EXTEMPORANEO']} extemporáneas"
          f" · {conteo['INCUMPLIDO']} incumplidas · {conteo['PENDIENTE']} vencidas"
          f" sin registrar · el resto, pendientes por vencer")
    print(f"  Generadas por el planificador del binario el {datetime.now():%Y-%m-%d %H:%M}\n")
    for email, clave in CLAVES.items():
        print(f"  {email:<18} {clave}")


def limpiar(destino: Path) -> None:
    """Borra SOLO lo que esta siembra vuelve a escribir.

    Antes esto era `rmtree(destino)`, y funcionaba mientras el destino fuera
    `rust/demo`, una carpeta que no contiene nada más. En el servidor la carpeta
    de la demostración es también donde viven el binario y este mismo guion: un
    `rmtree` se los llevaba por delante y el despliegue quedaba sin nada que
    arrancar. Una limpieza que borra más de lo que la corrida regenera no es
    limpieza, es pérdida de datos — y no se ve en la primera vuelta, solo en la
    segunda (directiva 22).
    """
    for wal in ("siger.db", "siger.db-wal", "siger.db-shm"):
        (destino / wal).unlink(missing_ok=True)
    shutil.rmtree(destino / "storage", ignore_errors=True)


def main() -> int:
    global BINARIO

    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--ruta", default=str(RAIZ / "rust" / "demo"),
                   help="carpeta donde vive la base de demostración")
    p.add_argument("--binario", default=str(BINARIO_POR_DEFECTO),
                   help="ruta del siger-api que migra, hashea y planifica "
                        "(en el servidor no está en rust/target/release)")
    p.add_argument("--limpiar", action="store_true",
                   help="borra la base y el almacén antes de sembrar "
                        "(no la carpeta: ahí puede vivir el propio binario)")
    p.add_argument("--forzar", action="store_true",
                   help="siembra aunque la base tenga usuarios ajenos a la demo "
                        "(la vacía: úsese sabiendo lo que hace)")
    args = p.parse_args()

    BINARIO = Path(args.binario).resolve()
    if not BINARIO.exists():
        print(f"Falta el binario: {BINARIO}\nConstrúyalo con ./rust/construir.sh",
              file=sys.stderr)
        return 1

    destino = Path(args.ruta)
    # La comprobación va ANTES de limpiar y no solo dentro de `sembrar`. Puesta
    # solo allí, `--limpiar` la anulaba: borraba la base del cliente y después
    # preguntaba, sobre una base recién creada, si era la de demostración —
    # siempre que sí, porque ya no quedaba nadie. Una salvaguarda que la bandera
    # más usada desactiva no es una salvaguarda.
    if (destino / "siger.db").exists():
        comprobar_que_es_de_demostracion(destino / "siger.db", args.forzar)
    if args.limpiar and destino.exists():
        limpiar(destino)
    destino.mkdir(parents=True, exist_ok=True)
    sembrar(destino, args.forzar)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
