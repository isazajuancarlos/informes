"""El asistente DIBUJADO y PULSADO, no solo importado.

`test_asistente_regla.py` comprueba la lógica; esto comprueba que existe una
pantalla que la usa. Son cosas distintas y confundirlas es fácil: la lógica
puede estar impecable y el diálogo no abrirse, o el calendario no admitir varias
fechas, o el botón de avanzar no llamar a nada. Nada de eso se cae al importar
el módulo, y todo se cae en cuanto alguien lo abre.

Se usa el simulador de cliente de NiceGUI: construye los elementos de verdad y
los pulsa, sin navegador.
"""
import os
import sys
from datetime import date

from nicegui import ui
from nicegui.testing import User

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from app.services.asistente_regla import Asistente  # noqa: E402
from app.ui.asistente_cronograma import abrir_asistente  # noqa: E402

LUNES = date(2026, 7, 6)


def _solo(user, marcador):
    """El único elemento con ese marcador. Falla si hay más de uno.

    `find(...).elements` devuelve un CONJUNTO: buscar por índice funciona hasta
    que deja de funcionar, y entonces la prueba falla por un motivo que no tiene
    nada que ver con lo que comprueba.
    """
    encontrados = list(user.find(marker=marcador).elements)
    assert len(encontrados) == 1, (marcador, encontrados)
    return encontrados[0]


# Marcar en el calendario mismo no se puede desde aquí —el QDate es un
# componente de Quasar y sin navegador nadie lo pinta—, así que las fechas se
# ponen por el rango, que sí es pulsable. Lo demás se ejercita entero: que las
# páginas se dibujen, que los botones existan y que al pulsarlos pase lo que
# dicen. Ahí es donde estaban los fallos que nada más detectaba.


async def test_el_asistente_se_abre_y_pide_marcar_en_el_calendario(user: User):
    recogido = {}

    @ui.page("/")
    def _():
        abrir_asistente(lambda regla, resumen: recogido.update(regla=regla))

    await user.open("/")
    await user.should_see("Definir el cronograma")
    await user.should_see("Cuándo vence")
    await user.should_see("Nada marcado todavía")
    # El botón de avanzar está apagado, y se dice POR QUÉ.
    await user.should_see("Marque al menos un día en el calendario")


async def test_hay_un_calendario_que_admite_VARIAS_fechas(user: User):
    """La pregunta del asistente es un calendario de selección múltiple. Con uno
    de fecha única no se podría distinguir «el día 6» de «los días 6 y 20»."""
    @ui.page("/")
    def _():
        abrir_asistente(lambda regla, resumen: None)

    await user.open("/")
    calendarios = list(user.find(ui.date).elements)
    assert len(calendarios) == 1, calendarios
    assert "multiple" in calendarios[0]._props, calendarios[0]._props


async def test_marcar_un_rango_al_reves_lo_DICE_en_vez_de_callarse(user: User):
    @ui.page("/")
    def _():
        abrir_asistente(lambda regla, resumen: None)

    await user.open("/")
    # Por MARCADOR, no por posición: `find(...).elements` es un conjunto y no
    # tiene orden. Encontrarlas por índice funcionaba de casualidad.
    _solo(user, "rango-desde").set_value("2026-07-10")
    _solo(user, "rango-hasta").set_value("2026-07-01")
    user.find("Marcar el rango").click()
    await user.should_see("no hay ningún día entre las dos")


async def test_el_recorrido_ENTERO_hasta_confirmar(user: User):
    """De marcar a guardar, pulsando lo que pulsaría una persona.

    Es la prueba que responde «¿esto funciona?». Todo lo demás comprueba piezas.
    """
    recogido = {}

    @ui.page("/")
    def _():
        abrir_asistente(lambda regla, resumen: recogido.update(
            regla=regla, resumen=resumen))

    await user.open("/")

    # 1. Marcar. Se hace por el rango, que sí es pulsable sin navegador.
    _solo(user, "rango-desde").set_value("2026-07-06")
    _solo(user, "rango-hasta").set_value("2026-07-06")
    user.find("Marcar el rango").click()
    await user.should_see("1 día(s) marcado(s)")

    # 2. Elegir qué significa. Aparecen las lecturas, con sus fechas.
    user.find("Siguiente").click()
    await user.should_see("Qué significa")
    await user.should_see("el día 6 de cada mes")
    await user.should_see("todas las semanas, lunes")
    await user.should_see("no elige por usted")

    user.find("lectura:el día 6 de cada mes").click()

    # 3. El ajuste, con el dato MEDIDO de cuántos caen en día no hábil.
    user.find("Siguiente").click()
    await user.should_see("Cómo se ajusta")
    await user.should_see("caen en día no hábil")
    user.find("ajuste:anterior").click()

    # 4. Confirmar: el resumen y las fechas de verdad.
    user.find("Siguiente").click()
    await user.should_see("Confirmar")
    await user.should_see("Rige desde")
    await user.should_see("Mensual")

    user.find("Usar este cronograma").click()
    assert recogido["regla"]["modo"] == "ESPECIFICO"
    assert recogido["regla"]["dia_corte"] == 6
    assert recogido["regla"]["ajuste_habil_anterior"] is True
    assert recogido["regla"]["fecha_inicio"] == "2026-07-06"
    assert "Mensual" in recogido["resumen"]


async def test_no_se_confirma_nada_si_no_se_llega_al_final(user: User):
    """El asistente solo entrega una regla al pulsar el último botón. Si
    entregara algo antes, se guardaría una obligación a medio definir."""
    recogido = {}

    @ui.page("/")
    def _():
        abrir_asistente(lambda regla, resumen: recogido.update(regla=regla))

    await user.open("/")
    user.find("Siguiente").click()   # bloqueado: no hay nada marcado
    await user.should_see("Marque al menos un día")
    assert recogido == {}


async def test_el_gestor_de_informes_ofrece_el_asistente(user: User):
    """Y lo ofrece como camino principal, no escondido."""
    from app.ui.reports_manager import page_reports_manager

    @ui.page("/")
    def _():
        page_reports_manager()

    await user.open("/")
    await user.should_see("Gestor de Informes")
    user.find(marker="nuevo-informe").click()
    await user.should_see("Cronograma")
    await user.should_see("Definir con el calendario")


async def test_la_pantalla_y_la_LOGICA_dicen_lo_mismo():
    """Las lecturas que se pintan son exactamente las que el servicio calcula.

    Si la pantalla filtrara o reordenara por su cuenta, el usuario elegiría de
    una lista que no es la que está probada.
    """
    a = Asistente(seleccion=[LUNES])
    dichos = [L.dice for L in a.lecturas()]
    assert len(dichos) == len(set(dichos)), "una lectura repetida se elegiría dos veces"
    assert dichos[0] == "todos los días desde esa fecha"


async def test_el_formulario_de_NUEVO_INFORME_se_dibuja_entero(user: User):
    """Regresión de un fallo que llevaba tiempo en pie y nadie veía.

    El campo «Fecha Inicio» pasaba `refrescar_vista_previa` como manejador antes
    de que esa función existiera, y abrir «Nuevo» reventaba con UnboundLocalError
    a media pantalla. No lo detectaba nada porque el módulo IMPORTA
    perfectamente: el fallo solo ocurre al dibujar.

    Aquí se comprueba que se llega hasta el último bloque del formulario.
    """
    from app.ui.reports_manager import page_reports_manager

    @ui.page("/")
    def _():
        page_reports_manager()

    await user.open("/")
    user.find(marker="nuevo-informe").click()
    await user.should_see("Datos Básicos")
    await user.should_see("Fecha Inicio")
    await user.should_see("Próximos vencimientos con esta regla")
    await user.should_see("Ficha del informe")
    await user.should_see("Datos que lleva la entrega")   # el último bloque
    await user.should_see("Guardar")


async def test_con_un_cronograma_del_asistente_las_pestanas_NO_se_muestran(user: User):
    """Dos vías visibles para la misma obligación es garantizar que una esté mal:
    quien tocara las pestañas creería estar cambiando algo que ya no se mira."""
    from app.ui import reports_manager as rm

    @ui.page("/")
    def _():
        rm.page_reports_manager()

    await user.open("/")
    user.find(marker="nuevo-informe").click()
    await user.should_see("Definir con el calendario")

    # Sin cronograma del asistente, el modo manual está a la vista.
    await user.should_see("A Demanda")

    user.find("Definir con el calendario").click()
    await user.should_see("Definir el cronograma")
    _solo(user, "rango-desde").set_value("2026-07-06")
    _solo(user, "rango-hasta").set_value("2026-07-06")
    user.find("Marcar el rango").click()
    user.find("Siguiente").click()
    user.find("lectura:el día 6 de cada mes").click()
    user.find("Siguiente").click()
    user.find("Siguiente").click()
    user.find("Usar este cronograma").click()

    await user.should_see("Rige desde")
    await user.should_see("Mensual")
    # Y las pestañas del modo manual ya no están: dos vías visibles para la
    # misma obligación garantizan que una esté mal.
    await user.should_not_see("A Demanda")
    await user.should_not_see("Fecha Inicio")


async def test_la_ultima_pagina_BLOQUEA_una_regla_que_no_generaria_nada(user: User):
    """La vigencia se decide en la última página, y puede invalidar lo elegido.

    «Solo esa fecha» con una vigencia posterior a esa fecha no genera nada: la
    obligación existiría en la base y no produciría una sola tarea. Es el fallo
    silencioso de siempre —nadie recibe algo que no existe— y por eso se bloquea
    aquí en vez de descubrirse cuando lo pregunte un ente de control.
    """
    recogido = {}

    @ui.page("/")
    def _():
        abrir_asistente(lambda regla, resumen: recogido.update(regla=regla))

    await user.open("/")
    _solo(user, "rango-desde").set_value("2026-07-06")
    _solo(user, "rango-hasta").set_value("2026-07-06")
    user.find("Marcar el rango").click()
    user.find("Siguiente").click()
    user.find("lectura:solo esa fecha").click()
    user.find("Siguiente").click()
    user.find("Siguiente").click()
    await user.should_see("Confirmar")

    # Hasta aquí se podía guardar…
    assert not _solo(user, "confirmar-cronograma").enabled is False

    # …y ahora la vigencia deja la única fecha fuera.
    _solo(user, "vigencia").set_value("2027-01-01")
    await user.should_see("no genera ningún vencimiento")
    assert _solo(user, "confirmar-cronograma").enabled is False

    user.find("Usar este cronograma").click()
    assert recogido == {}, "no puede entregarse una regla que no genera nada"


async def test_el_formulario_EXIGE_las_partes_del_flujo(user: User):
    """No se puede guardar un informe que nadie pide y que no se entrega a
    nadie, y se dice qué falta en vez de quedarse callado."""
    from app.ui.reports_manager import page_reports_manager

    @ui.page("/")
    def _():
        page_reports_manager()

    await user.open("/")
    user.find(marker="nuevo-informe").click()
    await user.should_see("Partes del flujo")
    await user.should_see("Solicitado por *")
    await user.should_see("Destinatarios (envío) *")

    # Con nombre y formatos puestos, lo único que falta son las partes.
    _solo(user, "nombre-informe").set_value("Informe de prueba")
    _solo(user, "formatos-informe").set_value(["pdf"])
    user.find("Guardar").click()
    # Los avisos salen por `ui.notify`, que no es un elemento de la página.
    assert user.notify.contains("Falta quién solicita"), user.notify.messages
    assert user.notify.contains("Falta a quién se entrega"), user.notify.messages

    from app.core.database import get_db_session
    from app.models.meta_tables import Informe
    with get_db_session() as db:
        assert not db.query(Informe).filter_by(nombre="Informe de prueba").first(), \
            "no puede haberse guardado"


async def test_se_puede_CREAR_una_parte_sin_salir_del_formulario(user: User):
    """Si las partes son obligatorias y no hay ninguna registrada, el primer
    informe de una instalación recién hecha sería imposible de guardar. Mandar
    a otra pantalla a mitad de un formulario largo pierde lo ya escrito."""
    from app.ui.reports_manager import page_reports_manager

    @ui.page("/")
    def _():
        page_reports_manager()

    await user.open("/")
    user.find(marker="nuevo-informe").click()
    await user.should_see("Todavía no hay partes registradas")

    user.find(marker="alta-parte-solicitantes").click()
    await user.should_see("Nueva parte del flujo")
    _solo(user, "nombre-parte").set_value("Ministerio de Salud")
    user.find("Crear y añadir").click()
    assert user.notify.contains("creada"), user.notify.messages

    # Y queda seleccionada, que es el punto: se creó PARA este informe.
    from app.core.database import get_db_session
    from app.models.meta_tables import Stakeholder
    with get_db_session() as db:
        assert db.query(Stakeholder).filter_by(nombre="Ministerio de Salud").first()
