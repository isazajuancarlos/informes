"""El producto se llama de UNA manera, y en un solo sitio.

Estaba escrito en cinco, y decían cinco cosas distintas:

    pantalla de entrada ......... «SIGER»
    cabecera de las demás ....... «SISTEMA INTEGRAL»
    pestaña del navegador ....... «Sistema Gestión»
    correos ..................... «Sistema de Gestión»
    clausulado .................. «Sistema de Gestión de Informes»

Nadie lo había visto porque nadie los había puesto en la misma frase. Se ve en
cuanto alguien enseña el producto: entra viendo un nombre, la cabecera dice
otro y la pestaña del navegador un tercero.

Este banco existe para que no vuelvan a separarse. No comprueba que el nombre
sea «SIGER» —eso lo decide quien instala, con `NOMBRE_PRODUCTO`— sino que **haya
uno solo**.
"""
from __future__ import annotations

import os
import pathlib
import re

import pytest

RAIZ = pathlib.Path(__file__).resolve().parent.parent

#: Nombres que alguna vez estuvieron escritos a mano en la interfaz. Si vuelve a
#: aparecer cualquiera de ellos como literal, es que alguien escribió el nombre
#: en vez de leerlo de la configuración.
NOMBRES_HUERFANOS = [
    "Sistema Gestión",
    "Sistema de Gestión",
    "SISTEMA INTEGRAL",
    "Sistema Integral de Gestión",
]

#: Dónde se permite que aparezcan: el sitio que los define y el que los explica.
PERMITIDOS = {"app/core/config.py", "tests/test_nombre_del_producto.py"}


def _fuentes():
    for ruta in sorted(RAIZ.glob("app/**/*.py")):
        rel = ruta.relative_to(RAIZ).as_posix()
        if "__pycache__" in rel or rel in PERMITIDOS:
            continue
        yield rel, ruta.read_text(encoding="utf-8")
    for suelto in ("main.py",):
        yield suelto, (RAIZ / suelto).read_text(encoding="utf-8")


@pytest.mark.parametrize("huerfano", NOMBRES_HUERFANOS)
def test_ningun_nombre_del_producto_esta_escrito_a_mano(huerfano):
    """Escribirlo a mano es exactamente cómo se separaron los cinco."""
    culpables = [rel for rel, texto in _fuentes() if huerfano in texto]
    assert not culpables, (
        f"«{huerfano}» aparece escrito a mano en {culpables}. "
        f"Use NOMBRE_PRODUCTO / NOMBRE_LARGO de app.core.config.")


def test_el_nombre_LITERAL_tampoco_se_escribe_a_mano():
    """Ni siquiera el correcto: hoy es «SIGER» y mañana el cliente lo cambia.

    Si está escrito a mano en la pantalla de entrada y leído de la
    configuración en la cabecera, un cliente que lo renombre se encuentra con
    los dos — que es cómo empezó este problema.
    """
    from app.core.config import NOMBRE_PRODUCTO

    culpables = [rel for rel, texto in _fuentes()
                 if re.search(rf"""['"]{re.escape(NOMBRE_PRODUCTO)}['"]""", texto)]
    assert not culpables, (
        f"«{NOMBRE_PRODUCTO}» está escrito a mano en {culpables}: léalo de "
        f"app.core.config.")


def test_el_nombre_se_puede_CAMBIAR_sin_tocar_el_codigo():
    """Un cliente que lo instala en su casa puede querer el suyo. Obligarle a
    modificar el programa convertiría una decisión de negocio en un parche."""
    import importlib

    from app.core import config

    previo = os.environ.get("NOMBRE_PRODUCTO")
    try:
        os.environ["NOMBRE_PRODUCTO"] = "Contraloría Digital"
        importlib.reload(config)
        assert config.NOMBRE_PRODUCTO == "Contraloría Digital"
    finally:
        if previo is None:
            os.environ.pop("NOMBRE_PRODUCTO", None)
        else:
            os.environ["NOMBRE_PRODUCTO"] = previo
        importlib.reload(config)
    assert config.NOMBRE_PRODUCTO == "SIGER", "se restauró mal el entorno"


def test_un_nombre_VACIO_no_deja_el_producto_sin_nombre():
    """`NOMBRE_PRODUCTO=` en un `.env` es un dedazo frecuente, y dejaría la
    pestaña del navegador y la cabecera en blanco sin decir nada."""
    import importlib

    from app.core import config

    previo = os.environ.get("NOMBRE_PRODUCTO")
    try:
        os.environ["NOMBRE_PRODUCTO"] = "   "
        importlib.reload(config)
        assert config.NOMBRE_PRODUCTO == "SIGER"
    finally:
        if previo is None:
            os.environ.pop("NOMBRE_PRODUCTO", None)
        else:
            os.environ["NOMBRE_PRODUCTO"] = previo
        importlib.reload(config)
