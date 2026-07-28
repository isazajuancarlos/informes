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

# Las fechas se calculan desde HOY

Una entrega vencida hace tres días y otra que vence mañana enseñan la columna de
urgencia haciendo su trabajo. Con fechas fijas, la demostración envejece: a las
dos semanas todo aparece vencido y la pantalla deja de contar lo que quería.
"""
from __future__ import annotations

import argparse
import shutil
import sqlite3
import subprocess
import sys
from datetime import date, timedelta
from pathlib import Path

RAIZ = Path(__file__).resolve().parent.parent
BINARIO = RAIZ / "rust" / "target" / "release" / "siger-api"

CLAVES = {
    "admin@co.co": "admin",
    "operador@co.co": "operador",
    "tercero@co.co": "tercero",
}


def hashear(clave: str) -> str:
    salida = subprocess.run(
        [str(BINARIO), "hashear", clave], capture_output=True, text=True, check=True
    )
    return salida.stdout.strip()


def sembrar(destino: Path) -> None:
    bd = destino / "siger.db"
    almacen = destino / "storage" / "entregas"

    # El esquema lo crea el propio binario al arrancar; aquí solo se rellena. Si
    # la base no existe todavía, se arranca una vez para que migre.
    if not bd.exists():
        print("La base no existe: se arranca el binario un momento para que la cree…")
        # El binario migra ANTES de ponerse a escuchar, así que se le deja
        # arrancar y se corta. El tiempo de espera venciendo es el camino normal
        # aquí, no un error: un servidor que termina solo sería el raro.
        try:
            subprocess.run(
                [str(BINARIO)],
                cwd=destino,
                env={
                    "SIGER_SECRETO_SESION": "solo-para-sembrar-no-sirve-para-servir-x",
                    "DATABASE_URL": "sqlite://siger.db",
                    "PORT": "0",
                },
                capture_output=True,
                timeout=10,
            )
        except subprocess.TimeoutExpired:
            pass

        # Y se COMPRUEBA que el esquema quedó. Dar por hecho que salió bien
        # porque el proceso arrancó es la clase de suposición que convierte un
        # fallo de migración en un error incomprensible tres pasos más adelante.
        if not bd.exists():
            raise SystemExit(
                f"El binario no creó {bd}. Revise que ./rust/construir.sh terminó bien."
            )
        with sqlite3.connect(bd) as prueba:
            tablas = {f[0] for f in prueba.execute(
                "SELECT name FROM sqlite_master WHERE type='table'")}
        faltan = {"usuarios", "informes", "entregas", "participacion"} - tablas
        if faltan:
            raise SystemExit(f"El esquema quedó incompleto; faltan: {sorted(faltan)}")

    c = sqlite3.connect(bd)
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

    informes = [
        (1, "RIPS mensual", "Registro individual de prestación de servicios"),
        (2, "Informe de cartera", "Estado de cartera por edades"),
        (3, "Reporte de calidad trimestral", "Indicadores de calidad"),
        (4, "Circular 016 — anexo técnico", "Reporte a la Superintendencia"),
    ]
    for iid, nombre, desc in informes:
        c.execute(
            "INSERT INTO informes (id, nombre, descripcion, recurrencia) VALUES (?,?,?,?)",
            (iid, nombre, desc, '{"modo":"ESPECIFICO","meses":[1,2,3,4,5,6,7,8,9,10,11,12],'
                               '"dia_corte":15,"mover_habil":true}'),
        )

    # El tercero es DESTINO de dos informes: por eso ve esos y no los demás.
    c.execute("INSERT INTO participacion (informe_id, stakeholder_id, rol) VALUES (1,1,'DESTINO')")
    c.execute("INSERT INTO participacion (informe_id, stakeholder_id, rol) VALUES (4,1,'DESTINO')")
    c.execute("INSERT INTO participacion (informe_id, usuario_id, rol) VALUES (1,2,'EJECUTA')")

    hoy = date.today()
    # (informe, dueño, días desde hoy hasta el nominal, ajuste, estado, archivo)
    filas = [
        (1, 2, -20, "GRACIA", "CUMPLIDO", "rips_junio.zip"),
        (2, 2, -3, "NINGUNO", "INCUMPLIDO", None),
        (1, 2, 0, "NINGUNO", "PENDIENTE", None),
        (3, 2, 1, "ANTICIPADO", "PENDIENTE", None),
        (2, 2, 9, "GRACIA", "PENDIENTE", None),
        (3, 2, 25, "NINGUNO", "PENDIENTE", None),
        (4, None, -8, "GRACIA", "CUMPLIDO", "circular016_anexo.pdf"),
        (4, None, 12, "NINGUNO", "PENDIENTE", None),
    ]
    for i, (informe, dueno, desfase, ajuste, estado, archivo) in enumerate(filas, start=1):
        nominal = hoy + timedelta(days=desfase)
        # El límite se separa del nominal solo cuando hay ajuste, que es
        # exactamente lo que la pantalla debe dejar ver.
        limite = nominal + timedelta(days=1 if ajuste == "GRACIA" else -1 if ajuste == "ANTICIPADO" else 0)
        c.execute(
            "INSERT INTO entregas (id, informe_id, usuario_id, fecha_nominal, fecha_limite,"
            " tipo_ajuste, estado, archivo_nombre, firma) VALUES (?,?,?,?,?,?,?,?,?)",
            (i, informe, dueno, nominal.isoformat(), limite.isoformat(), ajuste, estado,
             archivo, "firma-de-ejemplo" if archivo else None),
        )
        if archivo:
            carpeta = almacen / str(i)
            carpeta.mkdir(parents=True, exist_ok=True)
            (carpeta / archivo).write_text(
                f"Contenido de ejemplo de {archivo}\n", encoding="utf-8"
            )

    c.commit()
    c.close()

    print(f"Sembrado en {bd}")
    print("  4 informes · 8 entregas · 3 usuarios")
    for email, clave in CLAVES.items():
        print(f"  {email:<18} {clave}")


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--ruta", default=str(RAIZ / "rust" / "demo"),
                   help="carpeta donde vive la base de demostración")
    p.add_argument("--limpiar", action="store_true",
                   help="borra la carpeta antes de sembrar")
    args = p.parse_args()

    if not BINARIO.exists():
        print(f"Falta el binario: {BINARIO}\nConstrúyalo con ./rust/construir.sh",
              file=sys.stderr)
        return 1

    destino = Path(args.ruta)
    if args.limpiar and destino.exists():
        shutil.rmtree(destino)
    destino.mkdir(parents=True, exist_ok=True)
    sembrar(destino)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
