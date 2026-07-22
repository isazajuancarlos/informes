from dataclasses import dataclass
from datetime import datetime, timedelta, date
import calendar
from app.core.database import get_db_session
from app.models.meta_tables import Informe, EntregaEsperada
from sqlalchemy import desc

_motor_calendario = None

#: Última hora del día. El vencimiento agota el día, no lo abre.
FIN_DIA = datetime.max.time().replace(microsecond=0)

#: Cuántos días NOMINALES por delante se miran al generar tareas.
#:
#: Con ajuste anticipado el día límite llega antes que el nominal, así que la
#: tarea tiene que existir antes de que su fecha nominal aparezca en el barrido.
#: Un puente largo mueve la fecha cuatro o cinco días; 31 cubre cualquier
#: calendario con margen y no crea nada por adelantado —una tarea solo nace
#: cuando llega la primera de sus dos fechas—.
VENTANA_ANTICIPACION = 31

#: Qué hacer cuando el día nominal no es hábil. NO son tres formas de mover una
#: fecha: son tres cosas distintas.
AJUSTE_NINGUNO = 'NINGUNO'
#: Se revisa y se entrega el hábil ANTERIOR. El vencimiento sigue siendo el día
#: nominal; lo que se adelanta es la entrega, porque el día nominal nadie
#: trabaja. Es una regla de control interno, y es la segura: adelantar nunca
#: llega tarde.
AJUSTE_ANTICIPADO = 'ANTICIPADO'
#: Período de gracia: se admite entregar hasta el hábil SIGUIENTE. El
#: vencimiento sigue siendo el día nominal; lo que se amplía es el plazo.
AJUSTE_GRACIA = 'GRACIA'


@dataclass(frozen=True)
class Vencimiento:
    """Un vencimiento tiene DOS fechas, y confundirlas cambia el resultado.

    **`nominal`** es el día que fija la norma o el contrato — el 1 de enero de un
    informe anual. No se mueve nunca, aunque sea festivo. Identifica la
    obligación: el «informe anual de 2026» vence el 1 de enero, y sigue siendo el
    de 2026 se entregue cuando se entregue.

    **`limite`** es el día contra el que se mide el cumplimiento, una vez
    resuelto que el nominal no era hábil. Es un DERIVADO del nominal, no un
    sustituto.

    Tratarlas como una sola fecha —que es lo que hacía este motor— tiene tres
    consecuencias que se pagaron una por una:

      1. La obligación **cambiaba de identidad** al ajustarse. Un informe anual
         con vencimiento el 1 de enero y ajuste al hábil anterior «pertenecía» al
         31 de diciembre, y por tanto al año anterior.
      2. Con vigencia desde el 1 de enero, ese vencimiento caía fuera y
         **desaparecía**: la obligación existía y el sistema no la mostraba.
      3. Para no perder los que el ajuste sacaba de su mes, el motor tenía que
         **barrer los meses vecinos** y la vista previa **reconstruir el origen
         a posteriori** buscando ±6 días. Dos mecanismos cuya única razón de ser
         era contener la confusión.

    Emitiendo por el día NOMINAL las tres desaparecen a la vez.
    """
    nominal: date
    limite: date
    ajuste: str = AJUSTE_NINGUNO
    #: Solo en periodicidad por horas: la hora concreta del día. `None` en el
    #: resto, donde el vencimiento agota el día.
    hora: int | None = None

    @property
    def se_movio(self) -> bool:
        return self.limite != self.nominal

    @property
    def es_gracia(self) -> bool:
        """El plazo se amplió: entregar el día límite NO es tarde."""
        return self.ajuste == AJUSTE_GRACIA and self.se_movio

    @property
    def es_anticipado(self) -> bool:
        """La entrega se adelantó: el día nominal ya nadie trabaja."""
        return self.ajuste == AJUSTE_ANTICIPADO and self.se_movio

    @property
    def limite_dt(self) -> datetime:
        if self.hora is not None:
            return datetime.combine(self.limite, datetime.min.time()).replace(hour=self.hora)
        return datetime.combine(self.limite, FIN_DIA)

    @property
    def nominal_dt(self) -> datetime:
        return datetime.combine(self.nominal, FIN_DIA)


def _get_motor():
    global _motor_calendario
    if _motor_calendario is None:
        from app.services.calendar_engine import MotorReglasDinamicas
        _motor_calendario = MotorReglasDinamicas()
    return _motor_calendario


class GeneradorTareas:

    @staticmethod
    def mover_a_habil_siguiente(fecha):
        return _get_motor()._ajustar_fecha(fecha, 'NEXT_WORKDAY')

    @staticmethod
    def mover_a_habil_anterior(fecha):
        return _get_motor()._ajustar_fecha(fecha, 'PREV_WORKDAY')

    @staticmethod
    def _n_esimo_habil(year, month, n):
        """El n-ésimo día HÁBIL del mes. No es «el día n ajustado a hábil».

        «Dentro de los primeros cinco días hábiles del mes siguiente» es una de
        las formas más frecuentes de fijar un plazo en la norma colombiana, y no
        se puede expresar moviendo una fecha: si el mes empieza con un festivo,
        el 5º día hábil es el 8, no el 5 corrido al 8.

        Medido sobre la base de 399 informes reales de una entidad que opera:
        41 de los 318 con periodicidad codificada —uno de cada ocho— usan esta
        forma. Era el único hueco grande del motor.

        Si el mes tiene menos días hábiles que `n`, se usa el último hábil, por
        el mismo criterio que el día de corte 31 en un mes de 30: descartar el
        vencimiento lo haría desaparecer sin avisar. El rango real es de 18 a 23
        días hábiles al mes (comprobado sobre 2020-2040), y el validador avisa
        por encima de 18.
        """
        ultimo_habil = None
        vistos = 0
        for dia in range(1, calendar.monthrange(year, month)[1] + 1):
            fecha = datetime(year, month, dia).date()
            if not _get_motor()._es_habil(fecha):
                continue
            vistos += 1
            ultimo_habil = fecha
            if vistos == n:
                return fecha
        return ultimo_habil

    @staticmethod
    def _nominales_del_mes(year, month, config):
        """TODOS los días nominales que la regla fija en ese mes.

        `dia_corte` era un escalar, así que una obligación de los días 7 y 20 —o
        una quincenal, que es de las más comunes que existen— no se podía
        expresar. No es un caso exótico: era un límite arbitrario de lo que el
        sistema admitía que una obligación fuera.

        Se acepta `dias_corte` como lista y se conserva `dia_corte` escalar, que
        es lo que hay guardado en las reglas ya creadas.
        """
        dias = config.get('dias_corte')
        if not dias:
            return [GeneradorTareas._nominal_del_mes(year, month, config)]

        salida = []
        for dia in dias:
            sencilla = {**config, 'dia_corte': dia,
                        'es_ultimo_dia': False, 'dia_habil': None}
            nominal = GeneradorTareas._nominal_del_mes(year, month, sencilla)
            if nominal and nominal not in salida:
                salida.append(nominal)
        return sorted(salida)

    @staticmethod
    def _nominal_del_mes(year, month, config):
        """Día NOMINAL que corresponde a ese mes. Sin ajustar a hábil.

        Deliberadamente no aplica el ajuste: el nominal es el día que fija la
        regla, y el ajuste se calcula aparte en `_con_ajuste`. Cuando esta
        función devolvía la fecha ya ajustada, el mes al que «pertenecía» un
        vencimiento dependía del calendario de festivos.
        """
        # El n-ésimo día hábil YA es hábil por construcción, así que el ajuste
        # no tiene nada que hacer. Se resuelve primero por eso.
        if config.get('dia_habil'):
            try:
                n = int(config['dia_habil'])
            except (ValueError, TypeError):
                n = 1
            return GeneradorTareas._n_esimo_habil(year, month, max(1, n))

        if config.get('es_ultimo_dia'):
            return datetime(year, month, calendar.monthrange(year, month)[1]).date()
        try:
            return datetime(year, month, int(config.get('dia_corte', 1))).date()
        except (ValueError, TypeError):
            # Día 31 en un mes de 30: se usa el último día real del mes en vez de
            # descartar el vencimiento. Descartarlo lo haría desaparecer sin
            # avisar, y una obligación que nadie ve es peor que una mal fechada.
            return datetime(year, month, calendar.monthrange(year, month)[1]).date()

    @staticmethod
    def _con_ajuste(nominal, config):
        """Envuelve un día nominal con su día límite y el tipo de ajuste.

        Lee las dos parejas de banderas —`ajuste_habil*` y `mover_habil*`—
        porque el ESPECÍFICO usaba una y el CÍCLICO y el SEMANAL la otra. Esa
        divergencia ya costó que el ajuste al hábil anterior no se aplicara nunca
        en modo semanal: el bloque solo miraba una de las dos.
        """
        if config.get('ajuste_habil') or config.get('mover_habil'):
            return Vencimiento(nominal,
                               GeneradorTareas.mover_a_habil_siguiente(nominal),
                               AJUSTE_GRACIA)
        if config.get('ajuste_habil_anterior') or config.get('mover_habil_anterior'):
            return Vencimiento(nominal,
                               GeneradorTareas.mover_a_habil_anterior(nominal),
                               AJUSTE_ANTICIPADO)
        return Vencimiento(nominal, nominal, AJUSTE_NINGUNO)

    @staticmethod
    def obtener_vencimientos_para_fecha(fecha_evaluar, config):
        """Vencimientos cuyo día NOMINAL es `fecha_evaluar`.

        La pregunta que responde esta función es «¿la regla fija un vencimiento
        para hoy?», no «¿cae hoy algún vencimiento ya ajustado?». La segunda es
        la que hacía antes, y por eso necesitaba barrer meses y semanas vecinas:
        el ajuste puede mover el día límite a otro mes, pero el día NOMINAL
        siempre está donde la regla dice.

        Devuelve objetos `Vencimiento`, cada uno con su día nominal, su día
        límite y qué tipo de ajuste los separa.
        """
        str_inicio = config.get('fecha_inicio')
        inicio_dt = None
        if str_inicio:
            try:
                inicio_dt = datetime.strptime(str_inicio, '%Y-%m-%d').date()
            except (ValueError, TypeError):
                inicio_dt = None

        # La vigencia se comprueba contra el día NOMINAL. Contra el límite
        # borraba obligaciones reales: un informe anual del 1 de enero con
        # ajuste anticipado tiene su límite el 31 de diciembre anterior, y con
        # vigencia desde el 1 de enero desaparecía entero. La obligación nace el
        # 1 de enero; que haya que entregarla antes no la mueve de sitio.
        if inicio_dt and fecha_evaluar < inicio_dt:
            return []

        modo = config.get('modo', 'CICLICO')
        vencimientos = []

        # CÍCLICO
        if modo == 'CICLICO':
            if inicio_dt:
                intervalo = int(config.get('intervalo', 0))
                diff = (fecha_evaluar - inicio_dt).days
                toca = (intervalo == 0 and diff == 0) or (intervalo > 0 and diff % intervalo == 0)
                if toca:
                    vencimientos.append(GeneradorTareas._con_ajuste(fecha_evaluar, config))

        # ESPECÍFICO
        elif modo == 'ESPECIFICO':
            if fecha_evaluar.month in config.get('meses', []):
                for nominal in GeneradorTareas._nominales_del_mes(
                        fecha_evaluar.year, fecha_evaluar.month, config):
                    if nominal == fecha_evaluar:
                        vencimientos.append(
                            GeneradorTareas._con_ajuste(nominal, config))
                        break  # un vencimiento por día, aunque dos días coincidan

        # FECHAS EXPLÍCITAS
        elif modo == 'FECHAS':
            # La salida de emergencia del diseño: una obligación que no sigue
            # ningún patrón se enumera. Sin esto, «el sistema puede crear
            # cualquier informe» sería cierto solo para las formas que se nos
            # ocurrieron, y quien tuviera una distinta se quedaría fuera sin
            # alternativa.
            for texto in config.get('fechas', []):
                try:
                    nominal = datetime.strptime(str(texto).strip(), '%Y-%m-%d').date()
                except (ValueError, TypeError):
                    continue  # una fecha ilegible no invalida las demás
                if nominal == fecha_evaluar:
                    vencimientos.append(GeneradorTareas._con_ajuste(nominal, config))
                    break

        # SEMANAL
        elif modo == 'SEMANAL':
            if fecha_evaluar.weekday() in config.get('dias_semana', []):
                vencimientos.append(GeneradorTareas._con_ajuste(fecha_evaluar, config))

        # HORAS
        elif modo == 'HOURLY':
            # Varios vencimientos dentro del mismo día: aquí el ajuste a día
            # hábil no aplica —no se traslada una franja horaria a otro día— y
            # nominal y límite coinciden por definición.
            hora_ini = int(config.get('hora_inicio', 8))
            hora_fin = int(config.get('hora_fin', 18))
            intervalo = int(config.get('intervalo_horas', 1))
            curr = hora_ini
            while curr <= hora_fin and intervalo > 0:
                vencimientos.append(
                    Vencimiento(fecha_evaluar, fecha_evaluar, AJUSTE_NINGUNO, hora=curr))
                curr += intervalo

        return vencimientos

    @staticmethod
    def obtener_deadlines_para_fecha(fecha_evaluar, config):
        """Compatibilidad: solo los días LÍMITE, como `datetime`.

        Se conserva para el código que aún no distingue las dos fechas. Lo que
        devuelve ya no coincide día a día con `obtener_vencimientos_para_fecha`
        —esta emite por el nominal— así que **no se debe usar para barrer el
        calendario**: la lista de un día concreto puede venir vacía y aun así
        haber un vencimiento cuyo límite cae ahí.
        """
        return [v.limite_dt for v in
                GeneradorTareas.obtener_vencimientos_para_fecha(fecha_evaluar, config)]

    @staticmethod
    def generar_pendientes_del_dia(hoy=None):
        """Crea las entregas pendientes que ya deban existir.

        `hoy` se puede fijar para poder simular el paso del tiempo. Se expone
        como parámetro en vez de parchear el reloj desde fuera: el planificador
        es incremental —cada corrida arranca donde acabó la anterior— y sus
        defectos solo aparecen al encadenar cientos de días, algo que no se puede
        comprobar sin controlar la fecha.
        """
        from app.models.meta_tables import Delegacion
        from app.core.config import MAX_RETROACTIVE_DAYS

        creados = 0
        hoy = hoy or datetime.now().date()
        pendientes_notif = []  # (uid, nombre_informe, venc, es_delegada)

        with get_db_session() as db:
            informes = db.query(Informe).filter(Informe.activo == True).all()
            for inf in informes:
                config = inf.recurrencia_json or {}
                str_inicio = config.get('fecha_inicio')
                if not str_inicio: continue

                try:
                    fecha_inicio_config = datetime.strptime(str_inicio, '%Y-%m-%d').date()
                except Exception:
                    continue

                ultima_tarea = db.query(EntregaEsperada).filter(
                    EntregaEsperada.informe_id == inf.id
                ).order_by(desc(EntregaEsperada.fecha_programada)).first()

                start_scan = fecha_inicio_config
                if ultima_tarea:
                    start_scan = ultima_tarea.fecha_programada.date() + timedelta(days=1)

                if start_scan > hoy:
                    continue

                if (hoy - start_scan).days > MAX_RETROACTIVE_DAYS:
                    start_scan = hoy - timedelta(days=MAX_RETROACTIVE_DAYS)

                # Se miran también días NOMINALES futuros, porque con ajuste
                # anticipado el día límite llega ANTES que el nominal: un informe
                # que vence el 1 de enero y se entrega el hábil anterior tiene que
                # aparecer en diciembre. Barrer solo hasta hoy lo haría nacer ya
                # vencido.
                delta_dias = (hoy + timedelta(days=VENTANA_ANTICIPACION) - start_scan).days
                for i in range(delta_dias + 1):
                    dia_evaluar = start_scan + timedelta(days=i)
                    vencimientos = GeneradorTareas.obtener_vencimientos_para_fecha(dia_evaluar, config)

                    for venc in vencimientos:
                        # La tarea aparece cuando llega la primera de sus dos
                        # fechas. Con gracia y sin ajuste esa es la nominal, así
                        # que nada cambia; con anticipación es la límite.
                        dia_aparicion = min(venc.nominal, venc.limite)
                        if dia_aparicion > hoy:
                            continue

                        # La identidad de la entrega es su día NOMINAL, no el
                        # límite. Dos días marcados en la misma semana pueden
                        # compartir límite tras el ajuste, y con la clave puesta
                        # en el límite el segundo se perdía por «ya existe».
                        exists = db.query(EntregaEsperada).filter(
                            EntregaEsperada.informe_id == inf.id,
                            EntregaEsperada.fecha_vencimiento_nominal == venc.nominal_dt
                        ).first()
                        if not exists:
                            for resp in inf.responsables:
                                # Check for active delegation on this day
                                dia_dt = datetime.combine(dia_aparicion, datetime.min.time())
                                delegacion = db.query(Delegacion).filter(
                                    Delegacion.delegante_id == resp.id,
                                    Delegacion.activo == True,
                                    Delegacion.fecha_inicio <= dia_dt,
                                    Delegacion.fecha_fin >= datetime.combine(dia_aparicion, datetime.max.time().replace(microsecond=0))
                                ).first()

                                uid_asignado = delegacion.delegado_id if delegacion else resp.id
                                es_delegada = delegacion is not None

                                db.add(EntregaEsperada(
                                    informe_id=inf.id,
                                    usuario_id=uid_asignado,
                                    fecha_programada=dia_dt + timedelta(hours=8),
                                    fecha_vencimiento_nominal=venc.nominal_dt,
                                    fecha_limite_teorica=venc.limite_dt,
                                    tipo_ajuste=venc.ajuste,
                                    estado='PENDIENTE'
                                ))
                                creados += 1
                                pendientes_notif.append(
                                    (uid_asignado, inf.nombre, venc.limite_dt, es_delegada))

            db.commit()

        # Create in-app notifications AFTER the main session closes
        from app.services.notification_service import NotificationService
        for uid, nombre, venc, es_delegada in pendientes_notif:
            titulo = f"{'Tarea delegada' if es_delegada else 'Nueva tarea'}: {nombre}"
            NotificationService.crear(
                uid, 'INFO', titulo,
                f"Vencimiento: {venc.strftime('%d/%m/%Y')}",
                '/mis-pendientes'
            )

        return creados