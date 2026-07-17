"""
Background Scheduler Service
Automates periodic tasks: task generation, alerts, backups
"""
from apscheduler.schedulers.background import BackgroundScheduler
from apscheduler.triggers.cron import CronTrigger
from apscheduler.triggers.interval import IntervalTrigger
from app.services.scheduler import GeneradorTareas
from app.services.alert_service import AlertService
from app.services.backup_service import BackupService
from app.core.config import ENABLE_AUTO_SCHEDULER, SCHEDULER_HOUR, SCHEDULER_MINUTE
from datetime import datetime
import asyncio

class AutomationScheduler:
    """
    Gestiona tareas automáticas en segundo plano:
    1. Generación de tareas diarias (medianoche)
    2. Procesamiento de alertas (cada 6 horas)
    3. Backups automáticos (diarios a las 2 AM)
    """

    def __init__(self):
        self.scheduler = BackgroundScheduler(timezone='America/Bogota')
        self._setup_jobs()

    def _setup_jobs(self):
        """Configura los trabajos programados"""

        if not ENABLE_AUTO_SCHEDULER:
            print("⚠️ Auto-scheduler disabled in config")
            return

        # 1. GENERACIÓN DE TAREAS (diario a medianoche configurable)
        self.scheduler.add_job(
            func=self._job_generar_tareas,
            trigger=CronTrigger(hour=SCHEDULER_HOUR, minute=SCHEDULER_MINUTE),
            id='generar_tareas_diarias',
            name='Generación de Tareas Diarias',
            replace_existing=True
        )

        # 2. PROCESAMIENTO DE ALERTAS (cada 6 horas)
        self.scheduler.add_job(
            func=self._job_procesar_alertas,
            trigger=IntervalTrigger(hours=6),
            id='procesar_alertas',
            name='Procesamiento de Alertas',
            replace_existing=True
        )

        # 3. BACKUP AUTOMÁTICO (diario a las 2 AM)
        self.scheduler.add_job(
            func=self._job_backup_automatico,
            trigger=CronTrigger(hour=2, minute=0),
            id='backup_automatico',
            name='Backup Automático',
            replace_existing=True
        )

        # 4. ALERTA DE SLA A SUPERVISORES (lunes a las 8 AM)
        self.scheduler.add_job(
            func=self._job_reporte_sla,
            trigger=CronTrigger(day_of_week='mon', hour=8, minute=0),
            id='reporte_sla_semanal',
            name='Reporte SLA Semanal',
            replace_existing=True
        )

    def _job_generar_tareas(self):
        """Job: Generar tareas pendientes del día"""
        try:
            print(f"[{datetime.now()}] 🔄 Ejecutando generación de tareas...")
            creados = GeneradorTareas.generar_pendientes_del_dia()
            print(f"[{datetime.now()}] ✅ Tareas generadas: {creados}")
        except Exception as e:
            print(f"[{datetime.now()}] ❌ Error en generación de tareas: {e}")

    def _job_procesar_alertas(self):
        """Job: Procesar alertas de vencimiento"""
        try:
            print(f"[{datetime.now()}] 📧 Procesando alertas...")
            # APScheduler no soporta async directamente, usamos asyncio.run
            enviadas = asyncio.run(AlertService.procesar_alertas_pendientes())
            print(f"[{datetime.now()}] ✅ Alertas enviadas: {enviadas}")
        except Exception as e:
            print(f"[{datetime.now()}] ❌ Error en alertas: {e}")

    def _job_backup_automatico(self):
        """Job: Crear backup automático"""
        try:
            print(f"[{datetime.now()}] 💾 Creando backup automático...")
            backup_service = BackupService()
            success, filename = backup_service.crear_copia(automatico=True)

            if success:
                print(f"[{datetime.now()}] ✅ Backup creado: {filename}")
            else:
                print(f"[{datetime.now()}] ❌ Error en backup: {filename}")
        except Exception as e:
            print(f"[{datetime.now()}] ❌ Error en backup: {e}")

    def _job_reporte_sla(self):
        """Job: Enviar reporte de SLA a supervisores"""
        try:
            print(f"[{datetime.now()}] 📊 Generando reporte SLA...")
            from app.core.database import get_db_session
            from app.models.meta_tables import EntregaEsperada, Usuario
            from sqlalchemy import func
            from datetime import timedelta

            with get_db_session() as db:
                # Calcular métricas de la última semana
                semana_pasada = datetime.now() - timedelta(days=7)

                total = db.query(func.count(EntregaEsperada.id)).filter(
                    EntregaEsperada.fecha_programada >= semana_pasada
                ).scalar()

                cumplidas = db.query(func.count(EntregaEsperada.id)).filter(
                    EntregaEsperada.estado == 'CUMPLIDO',
                    EntregaEsperada.fecha_programada >= semana_pasada
                ).scalar()

                extemporaneas = db.query(func.count(EntregaEsperada.id)).filter(
                    EntregaEsperada.estado == 'EXTEMPORANEO',
                    EntregaEsperada.fecha_programada >= semana_pasada
                ).scalar()

                incumplidas = db.query(func.count(EntregaEsperada.id)).filter(
                    EntregaEsperada.estado == 'INCUMPLIDO',
                    EntregaEsperada.fecha_programada >= semana_pasada
                ).scalar()

                pendientes = db.query(func.count(EntregaEsperada.id)).filter(
                    EntregaEsperada.estado == 'PENDIENTE',
                    EntregaEsperada.fecha_programada >= semana_pasada
                ).scalar()

                tasa_cumplimiento = (cumplidas / total * 100) if total > 0 else 0

                metricas = {
                    'total': total,
                    'cumplidas': cumplidas,
                    'extemporaneas': extemporaneas,
                    'incumplidas': incumplidas,
                    'pendientes': pendientes,
                    'tasa_cumplimiento': tasa_cumplimiento
                }

                # Enviar a todos los supervisores
                supervisores = db.query(Usuario).filter(
                    Usuario.rol_sistema == 'SUPERVISOR',
                    Usuario.activo == True
                ).all()

                for sup in supervisores:
                    if sup.email:
                        asyncio.run(AlertService.enviar_alerta_supervisor_sla(sup.email, metricas))

                print(f"[{datetime.now()}] ✅ Reportes SLA enviados a {len(supervisores)} supervisores")

        except Exception as e:
            print(f"[{datetime.now()}] ❌ Error en reporte SLA: {e}")

    def start(self):
        """Inicia el scheduler"""
        if not ENABLE_AUTO_SCHEDULER:
            print("⚠️ Scheduler deshabilitado por configuración")
            return

        self.scheduler.start()
        print("✅ Background Scheduler iniciado")
        print(f"   - Generación de tareas: Diario a las {SCHEDULER_HOUR:02d}:{SCHEDULER_MINUTE:02d}")
        print(f"   - Alertas: Cada 6 horas")
        print(f"   - Backups: Diario a las 02:00")
        print(f"   - Reporte SLA: Lunes a las 08:00")

    def stop(self):
        """Detiene el scheduler"""
        if self.scheduler.running:
            self.scheduler.shutdown()
            print("🛑 Background Scheduler detenido")

    def get_jobs(self):
        """Obtiene lista de trabajos programados"""
        return self.scheduler.get_jobs()


# Instancia global del scheduler
_scheduler_instance = None

def init_scheduler():
    """Inicializa el scheduler global"""
    global _scheduler_instance
    if _scheduler_instance is None:
        _scheduler_instance = AutomationScheduler()
        _scheduler_instance.start()
    return _scheduler_instance

def get_scheduler():
    """Obtiene la instancia del scheduler"""
    return _scheduler_instance
