from datetime import datetime, timedelta, date
import calendar
from app.core.database import get_db_session
from app.models.meta_tables import Informe, EntregaEsperada
from sqlalchemy import desc

_motor_calendario = None


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
    def _mes_desplazado(fecha, desplazamiento):
        """(año, mes) tras sumar `desplazamiento` meses, cruzando el año bien."""
        indice = fecha.year * 12 + (fecha.month - 1) + desplazamiento
        return indice // 12, indice % 12 + 1

    @staticmethod
    def _objetivo_del_mes(year, month, config):
        """Fecha de vencimiento que corresponde a ese mes, YA ajustada a hábil.

        Se extrajo del bloque ESPECÍFICO para poder evaluarla sobre meses
        vecinos: el ajuste puede sacar el objetivo del mes que lo origina.
        """
        if config.get('es_ultimo_dia'):
            target = datetime(year, month, calendar.monthrange(year, month)[1]).date()
        else:
            try:
                target = datetime(year, month, int(config.get('dia_corte', 1))).date()
            except (ValueError, TypeError):
                # Día 31 en un mes de 30: se usa el último día real del mes en
                # vez de descartar el vencimiento. Descartarlo lo haría
                # desaparecer sin avisar, que es el fallo que este bloque corrige.
                target = datetime(year, month, calendar.monthrange(year, month)[1]).date()

        if config.get('ajuste_habil'):
            target = GeneradorTareas.mover_a_habil_siguiente(target)
        elif config.get('ajuste_habil_anterior'):
            target = GeneradorTareas.mover_a_habil_anterior(target)
        return target

    @staticmethod
    def obtener_deadlines_para_fecha(fecha_evaluar, config):
        str_inicio = config.get('fecha_inicio')
        if str_inicio:
            try:
                inicio_dt = datetime.strptime(str_inicio, '%Y-%m-%d').date()
                if fecha_evaluar < inicio_dt: return [] # Antes de vigencia
            except: pass
        
        modo = config.get('modo', 'CICLICO')
        deadlines = []
        fin_dia = datetime.max.time().replace(microsecond=0)

        # CÍCLICO
        if modo == 'CICLICO':
            intervalo = int(config.get('intervalo', 0))
            if str_inicio:
                try:
                    fecha_inicio = datetime.strptime(str_inicio, '%Y-%m-%d').date()
                    diff = (fecha_evaluar - fecha_inicio).days
                    if diff >= 0:
                        toca = False
                        if intervalo == 0 and diff == 0: toca = True
                        elif intervalo > 0 and diff % intervalo == 0: toca = True
                        if toca:
                            lim = fecha_evaluar
                            if config.get('mover_habil'): lim = GeneradorTareas.mover_a_habil_siguiente(fecha_evaluar)
                            elif config.get('mover_habil_anterior'): lim = GeneradorTareas.mover_a_habil_anterior(fecha_evaluar)
                            deadlines.append(datetime.combine(lim, fin_dia))
                except: pass

        # ESPECÍFICO
        elif modo == 'ESPECIFICO':
            meses = config.get('meses', [])
            # El objetivo AJUSTADO puede caer fuera del mes que lo origina: con
            # día de corte 1 y ajuste al hábil ANTERIOR, el vencimiento de
            # febrero cae en enero; con «último día» y ajuste SIGUIENTE, el de
            # enero cae en febrero.
            #
            # Comprobar solo el mes de `fecha_evaluar` perdía esos vencimientos
            # EN SILENCIO. Medido sobre 2026: «día 1 + anterior» generaba 6 de
            # 12, y «último día + siguiente», 8 de 12. En un sistema de
            # cumplimiento eso son obligaciones que nunca aparecen —sin tarea,
            # sin alerta y sin rastro— hasta que las pregunta un ente de
            # control. Por eso se miran también los meses vecinos.
            for desplazamiento in (-1, 0, 1):
                year, month = GeneradorTareas._mes_desplazado(fecha_evaluar, desplazamiento)
                if month not in meses:
                    continue
                target = GeneradorTareas._objetivo_del_mes(year, month, config)
                if target == fecha_evaluar:
                    deadlines.append(datetime.combine(fecha_evaluar, fin_dia))
                    break  # un solo vencimiento por día, aunque dos meses coincidan

        # SEMANAL
        elif modo == 'SEMANAL':
            dias = config.get('dias_semana', [])
            if fecha_evaluar.weekday() in dias:
                lim = fecha_evaluar
                if config.get('mover_habil'): lim = GeneradorTareas.mover_a_habil_siguiente(fecha_evaluar)
                deadlines.append(datetime.combine(lim, fin_dia))

        # HORAS
        elif modo == 'HOURLY':
            # Valida inicio
            valid_start = True
            if str_inicio:
                 if fecha_evaluar < datetime.strptime(str_inicio, '%Y-%m-%d').date(): valid_start = False
            
            if valid_start:
                hora_ini = int(config.get('hora_inicio', 8))
                hora_fin = int(config.get('hora_fin', 18))
                intervalo = int(config.get('intervalo_horas', 1))
                curr = hora_ini
                while curr <= hora_fin:
                    deadlines.append(datetime.combine(fecha_evaluar, datetime.min.time()).replace(hour=curr))
                    curr += intervalo

        return deadlines

    @staticmethod
    def generar_pendientes_del_dia():
        from app.models.meta_tables import Delegacion
        from app.core.config import MAX_RETROACTIVE_DAYS

        creados = 0
        hoy = datetime.now().date()
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

                delta_dias = (hoy - start_scan).days
                for i in range(delta_dias + 1):
                    dia_evaluar = start_scan + timedelta(days=i)
                    vencimientos = GeneradorTareas.obtener_deadlines_para_fecha(dia_evaluar, config)

                    for venc in vencimientos:
                        exists = db.query(EntregaEsperada).filter(
                            EntregaEsperada.informe_id == inf.id,
                            EntregaEsperada.fecha_limite_teorica == venc
                        ).first()
                        if not exists:
                            for resp in inf.responsables:
                                # Check for active delegation on this day
                                dia_dt = datetime.combine(dia_evaluar, datetime.min.time())
                                delegacion = db.query(Delegacion).filter(
                                    Delegacion.delegante_id == resp.id,
                                    Delegacion.activo == True,
                                    Delegacion.fecha_inicio <= dia_dt,
                                    Delegacion.fecha_fin >= datetime.combine(dia_evaluar, datetime.max.time().replace(microsecond=0))
                                ).first()

                                uid_asignado = delegacion.delegado_id if delegacion else resp.id
                                es_delegada = delegacion is not None

                                db.add(EntregaEsperada(
                                    informe_id=inf.id,
                                    usuario_id=uid_asignado,
                                    fecha_programada=dia_dt + timedelta(hours=8),
                                    fecha_limite_teorica=venc,
                                    estado='PENDIENTE'
                                ))
                                creados += 1
                                pendientes_notif.append((uid_asignado, inf.nombre, venc, es_delegada))

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