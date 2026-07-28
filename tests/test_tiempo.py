"""Banco de la conversión UTC → hora local.

Existe porque el registro de auditoría de seguridad guardaba en UTC y se
mostraba como si fuera local: en Colombia, cada evento aparecía **cinco horas en
el futuro**. Un rastro de auditoría con las horas corridas no está incompleto:
miente, y en silencio.

Lo que se prueba no es que la función devuelva algo, sino que **la diferencia
sea la correcta** — una conversión que no convierte pasaría cualquier prueba que
solo mirase el tipo de dato.
"""
from __future__ import annotations

from datetime import datetime, timedelta, timezone

import pytest

from app.core import tiempo


def test_convierte_utc_a_bogota_restando_cinco_horas(monkeypatch):
    """El caso concreto que estaba roto. Colombia es UTC-5 todo el año: no tiene
    horario de verano, así que el desfase no depende de la fecha."""
    monkeypatch.setattr(tiempo, "TIMEZONE", "America/Bogota")
    utc = datetime(2026, 7, 21, 20, 17, 42)  # naive, como lo guarda la BD
    local = tiempo.utc_a_local(utc)
    assert (local.hour, local.minute) == (15, 17)
    assert local.utcoffset() == timedelta(hours=-5)


def test_el_formato_es_el_que_espera_la_tabla(monkeypatch):
    monkeypatch.setattr(tiempo, "TIMEZONE", "America/Bogota")
    assert tiempo.formatear_utc(datetime(2026, 7, 21, 20, 17, 42)) == "2026-07-21 15:17:42"


def test_none_no_revienta_y_da_cadena_vacia():
    """La columna es nullable y la tabla lo pintaba con un `if`; no se puede
    perder ese comportamiento al cambiar la función."""
    assert tiempo.utc_a_local(None) is None
    assert tiempo.formatear_utc(None) == ""


def test_respeta_un_momento_que_YA_trae_zona(monkeypatch):
    """Por si algún día se migra la columna a datetimes con zona: entonces la
    conversión debe seguir siendo correcta y no volver a restar cinco horas."""
    monkeypatch.setattr(tiempo, "TIMEZONE", "America/Bogota")
    con_zona = datetime(2026, 7, 21, 20, 17, 42, tzinfo=timezone.utc)
    assert tiempo.formatear_utc(con_zona) == "2026-07-21 15:17:42"


def test_cambiar_de_zona_cambia_el_resultado(monkeypatch):
    """Si la zona configurada NO afectara al resultado, la función sería
    decorativa y nadie lo notaría."""
    utc = datetime(2026, 7, 21, 20, 0, 0)
    monkeypatch.setattr(tiempo, "TIMEZONE", "America/Bogota")
    bogota = tiempo.formatear_utc(utc)
    monkeypatch.setattr(tiempo, "TIMEZONE", "Europe/Madrid")
    madrid = tiempo.formatear_utc(utc)
    assert bogota != madrid


def test_una_zona_inexistente_cae_a_UTC_y_no_a_la_del_servidor(monkeypatch):
    """Fallar a UTC es visible y alguien lo corrige. Fallar a la hora local del
    servidor parece correcto y no se entera nadie nunca (directiva 20)."""
    monkeypatch.setattr(tiempo, "TIMEZONE", "Marte/Olympus_Mons")
    utc = datetime(2026, 7, 21, 20, 17, 42)
    assert tiempo.formatear_utc(utc) == "2026-07-21 20:17:42"


def test_el_desfase_que_habia_era_de_cinco_horas(monkeypatch):
    """Prueba de regresión del defecto concreto: mostrar el UTC crudo y mostrar
    el convertido tienen que diferir exactamente en el offset de la zona."""
    monkeypatch.setattr(tiempo, "TIMEZONE", "America/Bogota")
    utc = datetime(2026, 7, 21, 20, 17, 42)
    crudo = utc.strftime("%Y-%m-%d %H:%M:%S")          # lo que hacía audit.py
    corregido = tiempo.formatear_utc(utc)               # lo que hace ahora
    assert crudo != corregido
    diferencia = utc - tiempo.utc_a_local(utc).replace(tzinfo=None)
    assert diferencia == timedelta(hours=5)


# --------------------------------------------------------------------------
# Las dos fechas de una entrega, en una línea
# --------------------------------------------------------------------------

class _Entrega:
    """Doble mínimo. Se usa uno propio en vez de un objeto de SQLAlchemy porque
    lo que se prueba es el TEXTO, y montar la base para eso ocultaría el fallo
    entre otros diez pasos."""

    def __init__(self, limite, nominal=None, ajuste=None):
        self.fecha_limite_teorica = limite
        self.fecha_vencimiento_nominal = nominal
        self.tipo_ajuste = ajuste


def test_la_gracia_y_la_anticipacion_NO_se_describen_igual():
    """Si el texto fuera el mismo, quien lo lee no puede saber si entregar el día
    que ve en pantalla es cumplir o llegar tarde."""
    from datetime import datetime

    from app.core.tiempo import describir_vencimiento

    gracia = describir_vencimiento(_Entrega(
        datetime(2026, 1, 2, 23, 59), datetime(2026, 1, 1, 23, 59), "GRACIA"))
    antic = describir_vencimiento(_Entrega(
        datetime(2025, 12, 31, 23, 59), datetime(2026, 1, 1, 23, 59), "ANTICIPADO"))

    assert "Vence 2026-01-01" in gracia and "plazo hasta 2026-01-02" in gracia, gracia
    assert "Vence 2026-01-01" in antic and "entregar antes de 2025-12-31" in antic, antic
    assert gracia != antic


def test_sin_ajuste_no_se_repite_la_fecha_dos_veces():
    """Decir «vence el 15 · límite el 15» es ruido."""
    from datetime import datetime

    from app.core.tiempo import describir_vencimiento

    txt = describir_vencimiento(_Entrega(
        datetime(2026, 1, 15, 23, 59), datetime(2026, 1, 15, 23, 59), "NINGUNO"))
    assert txt == "2026-01-15 23:59", txt


def test_una_entrega_ANTERIOR_a_la_distincion_no_se_inventa_el_dia_nominal():
    """Las filas viejas tienen el día nominal en NULL. Rellenarlo con el límite
    haría creer que la distinción se aplicó a esas entregas. No se aplicó."""
    from datetime import datetime

    from app.core.tiempo import describir_vencimiento

    txt = describir_vencimiento(_Entrega(datetime(2025, 6, 30, 23, 59)))
    assert txt == "2025-06-30 23:59", txt
    assert "Vence" not in txt


def test_sin_limite_lo_dice():
    from app.core.tiempo import describir_vencimiento

    assert describir_vencimiento(_Entrega(None)) == "Sin límite"


# ---------------------------------------------------------------------------
# Los dos relojes, como funciones con nombre
#
# `datetime.utcnow()` está deprecado desde Python 3.12 y marcado para
# eliminación. Sustituirlo NO era mecánico: la mitad de las columnas guardan UTC
# y una guarda hora local, así que un reemplazo uniforme habría corrido esa
# columna cinco horas. Estas pruebas fijan cuál es cuál.
# ---------------------------------------------------------------------------


def test_ahora_utc_es_naive_y_marca_utc():
    """Naive a propósito: las filas que ya existen lo son y las columnas no
    llevan `timezone=True`. Devolver un valor CON zona rompería cualquier
    comparación contra un valor leído de la base."""
    momento = tiempo.ahora_utc()
    assert momento.tzinfo is None, "con zona no se puede comparar con lo guardado"
    referencia = datetime.now(timezone.utc).replace(tzinfo=None)
    assert abs((referencia - momento).total_seconds()) < 5


def test_ahora_local_es_naive_y_marca_hora_local():
    momento = tiempo.ahora_local()
    assert momento.tzinfo is None
    assert abs((datetime.now() - momento).total_seconds()) < 5


def test_los_dos_relojes_no_son_el_mismo():
    """El corazón del asunto. Si alguien 'simplifica' haciendo que ambas
    devuelvan lo mismo, el desfase desaparece sin que falle nada más — y las
    fechas empiezan a mentir cinco horas."""
    import os
    # `datetime.now()` es naive y su `.utcoffset()` es None: hay que hacerlo
    # aware con `.astimezone()` para leer el offset local REAL. En un runner en
    # UTC (como el CI) da timedelta(0) y el test se salta, que es lo correcto:
    # ahí los dos relojes coinciden legítimamente y no hay desfase que medir.
    if datetime.now().astimezone().utcoffset() == timedelta(0) or os.environ.get("TZ") == "UTC":
        pytest.skip("la máquina está en UTC: aquí los dos relojes coinciden")
    diferencia = abs((tiempo.ahora_utc() - tiempo.ahora_local()).total_seconds())
    assert diferencia > 60, "UTC y hora local no pueden ser el mismo instante"


def test_ninguna_columna_del_modelo_usa_la_api_deprecada():
    """Registrar cerca de donde se violaría: esta prueba es la que impide que
    `datetime.utcnow` vuelva a entrar, no un comentario que hay que recordar."""
    import ast
    from pathlib import Path

    # Por AST y no buscando la cadena: los docstrings de `tiempo.py` EXPLICAN
    # esta migración y citan `datetime.utcnow()` a propósito. Un detector de
    # texto los marcaría, y la forma de callarlo sería borrar la explicación.
    raiz = Path(__file__).resolve().parent.parent
    culpables = []
    for py in raiz.rglob("*.py"):
        if any(p in py.parts for p in (".venv", "venv", "node_modules", "__pycache__")):
            continue
        try:
            arbol = ast.parse(py.read_text(encoding="utf-8"))
        except SyntaxError:
            continue
        for nodo in ast.walk(arbol):
            if isinstance(nodo, ast.Attribute) and nodo.attr == "utcnow":
                culpables.append(f"{py.relative_to(raiz)}:{nodo.lineno}")
    assert not culpables, (
        "datetime.utcnow() está deprecado y marcado para eliminación; "
        f"usa app.core.tiempo.ahora_utc(). Reintroducido en: {culpables}"
    )


def test_fecha_programada_usa_el_reloj_local_y_las_demas_el_utc():
    """La excepción del modelo, fijada por una prueba.

    `EntregaEsperada.fecha_programada` guarda hora LOCAL: todos los sitios que
    crean entregas la fijan a mano en local. Su default declaraba UTC, y quien
    creyera esa declaración y 'corrigiera' la columna al mostrarla la correría
    cinco horas en la dirección contraria.
    """
    from app.models import meta_tables as m

    def default_de(modelo, columna):
        # SQLAlchemy envuelve el callable para poder pasarle un contexto, así
        # que `.arg` NO es la función que se declaró: es su envoltorio, y
        # compararlo con `is` falla aunque todo esté bien. `functools.wraps`
        # deja la original en `__wrapped__`.
        arg = modelo.__table__.c[columna].default.arg
        return getattr(arg, "__wrapped__", arg)

    assert default_de(m.EntregaEsperada, "fecha_programada") is tiempo.ahora_local

    for modelo, columna in (
        (m.AuditLog, "timestamp"),
        (m.Comentario, "fecha"),
        (m.NotificacionApp, "fecha_creacion"),
        (m.SolicitudNuevoInforme, "fecha_solicitud"),
        (m.Informe, "fecha_creacion"),
        (m.ConjuntoDatos, "fecha_creacion"),
    ):
        assert default_de(modelo, columna) is tiempo.ahora_utc, (
            f"{modelo.__name__}.{columna} debe guardar UTC"
        )
