from datetime import date, datetime, timedelta
import holidays
from dateutil.rrule import rrule, DAILY, WEEKLY, MONTHLY, YEARLY, MO, TU, WE, TH, FR, SA, SU

# Mapeo para convertir strings JSON a constantes de dateutil
FRECUENCIAS = {
    "DAILY": DAILY, "WEEKLY": WEEKLY, "MONTHLY": MONTHLY, "YEARLY": YEARLY
}
DIAS_SEMANA = {
    "MO": MO, "TU": TU, "WE": WE, "TH": TH, "FR": FR, "SA": SA, "SU": SU
}

class MotorReglasDinamicas:
    """Calcula fechas hábiles contra el calendario de festivos del país.

    Los festivos NO son una tabla: la librería `holidays` los deriva de la ley
    —incluida la Ley Emiliani, que corre varios al lunes siguiente— así que el
    motor funciona igual en 2027 que en 2099 sin tocar nada. Eso es lo que
    permite que una regla escrita una vez siga siendo correcta año tras año.

    Lo que sí caduca es la LEY. Si el Congreso añade o quita un festivo, hace
    falta una versión nueva de la librería: es exactamente el valor que paga la
    cuota anual de «mantenimiento y actualizaciones normativas», y no un detalle
    de implementación.
    """

    def __init__(self, pais_iso: str = None, sin_festivos: bool = False):
        from app.core.config import PAIS_ISO as CONFIG_PAIS_ISO
        effective_pais = pais_iso or CONFIG_PAIS_ISO

        if sin_festivos:
            # Camino permisivo, y hay que PEDIRLO. Existe para pruebas y para
            # una entidad que de verdad opere sin calendario de festivos.
            self.festivos = {}
            return

        try:
            self.festivos = holidays.country_holidays(effective_pais)
        except Exception as e:
            # Antes esto caía a un calendario SIN FESTIVOS y seguía adelante
            # imprimiendo un aviso. El resultado era un sistema que parecía
            # funcionar: cada festivo contaba como día hábil, así que ningún
            # vencimiento se ajustaba nunca y todas las fechas salían plausibles
            # y equivocadas. Un sistema de cumplimiento que calcula mal los
            # plazos y no lo dice es peor que uno que no arranca: la caída se
            # nota en un minuto, la fecha falsa no se nota nunca.
            #
            # Un `print` no es fallar. Se levanta la excepción.
            raise ValueError(
                f"No hay calendario de festivos para el país '{effective_pais}'. "
                f"Revise PAIS_ISO en la configuración: debe ser un código ISO de "
                f"dos letras soportado por la librería `holidays`. Si la entidad "
                f"opera sin festivos, constrúyalo con sin_festivos=True."
            ) from e

    def _es_festivo(self, fecha: date) -> bool:
        """Solo festivo. NO incluye fines de semana, y esa es la diferencia.

        `_es_habil` lleva dentro la regla «lunes a viernes», que es la jornada de
        una entidad pública colombiana — no una ley del calendario. Una empresa
        privada puede trabajar de lunes a sábado, y para ella el sábado es un día
        laboral con un festivo posible.

        Quién trabaja qué días lo declara la entidad al elegir los días de la
        semana. Lo que el calendario aporta es qué días son festivos.
        """
        return fecha in self.festivos

    def _es_habil(self, fecha: date) -> bool:
        """Verifica si es hábil (Lunes-Viernes y No Festivo)"""
        return (fecha.weekday() < 5) and (fecha not in self.festivos)

    def _ajustar_fecha(self, fecha: date, logica_ajuste: str) -> date:
        """
        Ajusta una fecha individual según la regla dinámica de conflicto.
        logica_ajuste: 'NEXT_WORKDAY' (>), 'PREV_WORKDAY' (<), 'KEEP' (=)
        """
        if self._es_habil(fecha) or logica_ajuste == 'KEEP':
            return fecha

        direccion = 1 if logica_ajuste == 'NEXT_WORKDAY' else -1
        fecha_temp = fecha
        
        # Buscamos el día hábil más cercano (Límite 30 iteraciones por seguridad)
        for _ in range(30):
            fecha_temp += timedelta(days=direccion)
            if self._es_habil(fecha_temp):
                return fecha_temp
        
        return fecha # Fallback

    # Aquí vivía `generar_calendario(regla_json, anio)`, una SEGUNDA
    # implementación del cálculo de vencimientos con su propio esquema de regla
    # (`freq`, `bymonthday`, `byweekday`) y un docstring que se declaraba «EL
    # CORAZÓN DEL SISTEMA». No lo era: el planificador nunca la llamó. De esta
    # clase solo se usan `_es_habil` y `_ajustar_fecha`.
    #
    # Su único cliente era la demo, y por eso la demo enseñaba fechas que el
    # producto no habría generado. Dos motores para lo mismo garantizan que uno
    # esté mal y que nadie sepa cuál.
    #
    # El cálculo vive en `GeneradorTareas.obtener_vencimientos_para_fecha`.
    # `tests/test_demo_es_real.py` vigila que no vuelva.
