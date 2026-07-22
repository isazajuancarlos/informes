"""Vista previa: qué vencimientos genera una regla, ANTES de guardarla.

# Por qué existe

Definir la obligación es el paso más importante del sistema: de ahí salen el
calendario, las tareas, las alertas y la trazabilidad. Y hasta ahora se definía
**a ciegas** — se elegía periodicidad, día de corte y ajuste a día hábil, se
guardaba, y las fechas resultantes solo se veían cuando el planificador ya había
creado las tareas.

Una regla mal entendida no falla ruidosamente: genera vencimientos distintos de
los que la persona creía, o ninguno. Con este módulo, quien crea la obligación
ve la lista de fechas antes de confirmar.

# Usa el MOTOR REAL, y eso no es negociable

Llama a `GeneradorTareas.obtener_deadlines_para_fecha`, la misma función que usa
el planificador en producción. Una vista previa con su propia copia de la lógica
sería peor que no tenerla: mostraría fechas que el sistema luego no genera, y
nadie sabría cuál de las dos está mal.

Este módulo NO calcula fechas. Solo barre el calendario y recoge lo que el motor
devuelve.

# Dos fechas, no una

Cada vencimiento trae su día NOMINAL —el que fija la regla— y su día LÍMITE —el
que resulta de ajustar a hábil—. La vista previa muestra los dos, porque son la
pregunta que de verdad se hace quien crea la obligación: «¿qué día vence?» y
«¿hasta cuándo tengo?». Antes solo existía la segunda fecha y este módulo tenía
que reconstruir la primera buscando ±6 días con la configuración desactivada.
Ese apaño desapareció con la distinción.
"""
from __future__ import annotations

from dataclasses import dataclass
from datetime import date, datetime, timedelta

from app.services.scheduler import GeneradorTareas

#: Tope de días a barrer. Un año y medio cubre cualquier periodicidad razonable
#: —incluida la anual, que necesita cruzar el año— sin volver lento el formulario.
DIAS_MAXIMOS = 550

#: Nombres de los días, para explicar el ajuste a hábil en la interfaz.
DIAS_ES = ("lunes", "martes", "miércoles", "jueves", "viernes", "sábado", "domingo")
MESES_ES = ("enero", "febrero", "marzo", "abril", "mayo", "junio", "julio",
            "agosto", "septiembre", "octubre", "noviembre", "diciembre")


def _dia_largo(d: date) -> str:
    return f"{DIAS_ES[d.weekday()]} {d.day} de {MESES_ES[d.month-1]} de {d.year}"


@dataclass(frozen=True)
class Vencimiento:
    """Lo que se enseña en el formulario, derivado del vencimiento del motor."""
    #: Día que fija la regla. No se mueve aunque sea festivo.
    nominal: date
    #: Día contra el que se mide el cumplimiento.
    limite: date
    #: 'NINGUNO' | 'ANTICIPADO' | 'GRACIA'
    ajuste: str = "NINGUNO"

    #: Compatibilidad con el código que preguntaba por una sola fecha. Es el
    #: LÍMITE, que es contra el que se mide.
    @property
    def fecha(self) -> date:
        return self.limite

    @property
    def se_ajusto(self) -> bool:
        return self.limite != self.nominal

    @property
    def sin_ajustar(self) -> date:
        return self.nominal

    def describir(self) -> str:
        """Una línea que dice las dos cosas sin obligar a deducir ninguna."""
        txt = f"Vence {_dia_largo(self.nominal)}"
        if not self.se_ajusto:
            return txt
        if self.ajuste == "GRACIA":
            # El plazo se amplía: entregar ese día NO es tarde.
            return txt + f" · plazo hasta {_dia_largo(self.limite)}"
        # Anticipado: el día nominal no es hábil, así que se entrega antes.
        return txt + f" · entregar antes de {_dia_largo(self.limite)}"


def proximos(config: dict, cuantos: int = 6, desde: date | None = None
             ) -> tuple[list[Vencimiento], str]:
    """Los próximos `cuantos` vencimientos que genera `config`.

    Devuelve (lista, advertencia). La advertencia es texto vacío si todo va
    bien; si no, explica por qué la lista salió corta o vacía — que es la
    información que de verdad hace falta cuando una regla no produce nada.
    """
    desde = desde or date.today()

    # La regla puede empezar en el futuro: barrer desde hoy no encontraría nada
    # y parecería que no genera vencimientos.
    inicio_cfg = config.get("fecha_inicio")
    if inicio_cfg:
        try:
            f = datetime.strptime(str(inicio_cfg), "%Y-%m-%d").date()
            if f > desde:
                desde = f
        except (ValueError, TypeError):
            return ([], "La fecha de inicio no se entiende: use el formato AAAA-MM-DD.")

    if config.get("modo") == "HOURLY":
        return ([], "La periodicidad por horas genera varios vencimientos cada "
                    "día; la vista previa por fechas no aplica.")

    encontrados: list[Vencimiento] = []
    dia = desde
    for _ in range(DIAS_MAXIMOS):
        if len(encontrados) >= cuantos:
            break
        try:
            hay = GeneradorTareas.obtener_vencimientos_para_fecha(dia, config)
        except Exception as e:  # noqa: BLE001
            # Se informa en vez de devolver una lista vacía: «no genera nada» y
            # «la regla revienta» son cosas distintas y confundirlas dejaría al
            # usuario creyendo que configuró algo inofensivo.
            return (encontrados, f"La regla no se pudo evaluar el {dia}: {e}")
        for v in hay:
            encontrados.append(Vencimiento(nominal=v.nominal, limite=v.limite,
                                           ajuste=v.ajuste))
        dia += timedelta(days=1)

    if not encontrados:
        return ([], "Esta regla no genera ningún vencimiento en los próximos "
                    f"{DIAS_MAXIMOS} días. Revise la periodicidad, los meses "
                    "seleccionados y la fecha de inicio.")
    if len(encontrados) < cuantos:
        return (encontrados, f"Solo se encontraron {len(encontrados)} "
                             f"vencimiento(s) en los próximos {DIAS_MAXIMOS} días.")
    return (encontrados, "")
