"""
Script de Importación: datos_demo.json → Sistema v3.0
Importa 397 informes FICTICIOS desde el archivo JSON generado

⚠️ IMPORTANTE: Este script usa DATOS COMPLETAMENTE FICTICIOS
   generados automáticamente para propósitos de demostración.
   No contiene información sensible de entidades públicas.

REQUISITO: Ejecutar DESPUÉS de migrate_baseinformes.py

Ejecutar con:
    python importar_datos_demo.py

Opciones:
    --dry-run : Simula importación sin guardar datos
    --limit N : Importa solo los primeros N informes
    --archivo RUTA : Archivo JSON a importar (default: datos_demo.json)

Autor: Claude Code
Fecha: 8 de Enero, 2026
"""

import sys
import os
sys.path.append(os.path.dirname(os.path.abspath(__file__)))

import json
import argparse
from datetime import datetime
from app.core.database import get_db_session
from app.models.meta_tables import (
    Informe, Usuario, Stakeholder, ConjuntoDatos,
    ConfiguracionInformeVariable, MetaVariable, Participacion, ROL_ORIGEN
)

# Mapas de conversión
MAPA_TIPO_FLUJO = {
    'EI': 'ENVIO_INTERNO',
    'EE': 'ENVIO_EXTERNO',
    'RE': 'RECEPCION_EXTERNA',
    'BI': 'BIDIRECCIONAL',
}

TODOS_LOS_MESES = list(range(1, 13))

#: Cada periodicidad, expresada como lo que ES.
#:
#: Aquí «Mensual» era `cada 30 días` y «Anual» era `cada 365`. No son lo mismo y
#: la diferencia no es un matiz:
#:
#:   cada 30 días  → 12,17 al año, y el día del mes SE DESPLAZA. Un informe que
#:                   vence el 5 de enero vence el 4 de febrero, el 6 de marzo…
#:   cada 365 días → se corre un día en cada bisiesto, para siempre.
#:   cada 15 días  → 24,3 al año; una quincena son dos días FIJOS del mes.
#:
#: Un informe mensual no vence «cada 30 días»: vence un día del mes, y el motor
#: ya sabe expresarlo. Dejar el mapeo viejo hacía que la demostración enseñara
#: el sistema que el producto DEJÓ de ser — y una demo que miente sobre el
#: producto es peor que no tener demo.
#:
#: `A_DEMANDA` tampoco existía: el motor conoce `INMEDIATO`. La cadena mal
#: escrita no daba error, simplemente no generaba nada nunca.
MAPA_PERIODICIDAD = {
    'Diaria': ('CICLICO', 1),
    # Semanal es UN día a la semana. Marcar de lunes a viernes son 260 al año:
    # eso es un informe diario en días laborales, no uno semanal.
    'Semanal': ('SEMANAL', [0]),
    '2 veces por semana': ('SEMANAL', [0, 3]),
    # Quincenal: dos días FIJOS de cada mes — 24 al año. Se eligen el 1 y el 15
    # porque existen en los doce meses; el 30 no existe en febrero y el
    # validador lo rechaza, que es exactamente lo que debe hacer.
    'Quincenal': ('ESPECIFICO', {'meses': TODOS_LOS_MESES, 'dias_corte': [1, 15]}),
    # «Dentro de los primeros cinco días hábiles» es de las formas más
    # frecuentes de fijar un plazo en la norma colombiana, y no se puede
    # expresar con días corridos: si el mes empieza en festivo, el 5º hábil es
    # el 8. Uno de cada ocho informes de la base real la usa.
    'Mensual': ('ESPECIFICO', {'meses': TODOS_LOS_MESES, 'dia_habil': 5}),
    'Bimestral': ('ESPECIFICO', {'meses': [1, 3, 5, 7, 9, 11], 'dia_corte': 10}),
    'Trimestral': ('ESPECIFICO', {'meses': [1, 4, 7, 10], 'dia_corte': 15}),
    'Semestral': ('ESPECIFICO', {'meses': [1, 7], 'dia_corte': 15}),
    'Anual': ('ESPECIFICO', {'meses': [1], 'dia_corte': 31}),
    'Inmediatamente sea requerido': ('INMEDIATO', None),
    'Por solicitud': ('INMEDIATO', None),
    'Cuando lleguen requerimientos eventuales': ('INMEDIATO', None),
    'Dependiendo del corte definido': ('INMEDIATO', None),
    'Trimestral o cuando es requerido': ('INMEDIATO', None),
}

MAPA_FORMATOS = {
    'Excel': ['xlsx', 'xls'],
    'PDF': ['pdf'],
    'Word': ['docx', 'doc'],
    'Archivos Planos': ['txt', 'csv'],
    'Plano': ['txt'],
    'CSV': ['csv'],
    'Plano (TXT)': ['txt'],
    'XML': ['xml'],
    'JSON': ['json'],
    'Formulario Web': ['pdf', 'html'],
}


class ImportadorDatosDemo:
    def __init__(self, archivo='datos_demo.json', dry_run=False, limit=None):
        self.archivo = archivo
        self.dry_run = dry_run
        self.limit = limit
        self.stats = {
            'total': 0,
            'importados': 0,
            'errores': 0,
            'omitidos': 0,
            'conjuntos_datos_creados': 0,
        }
        self.errores_log = []
        self.cache_conjuntos = {}  # Cache para conjuntos de datos

    def log_error(self, consecutivo, mensaje):
        """Registra un error"""
        self.errores_log.append({'consecutivo': consecutivo, 'mensaje': mensaje})
        self.stats['errores'] += 1

    def leer_json(self):
        """Lee el archivo JSON y retorna lista de informes"""
        print(f"📖 Leyendo {self.archivo}...")

        if not os.path.exists(self.archivo):
            raise FileNotFoundError(f"Archivo no encontrado: {self.archivo}")

        with open(self.archivo, 'r', encoding='utf-8') as f:
            data = json.load(f)

        informes = data.get('informes', [])

        # Aplicar límite si se especificó
        if self.limit:
            informes = informes[:self.limit]

        self.stats['total'] = len(informes)

        print(f"  ✅ Encontrados {self.stats['total']} informes")
        if 'nota' in data:
            print(f"  ℹ️  {data['nota']}")

        return informes

    def obtener_o_crear_conjunto_datos(self, db, nombre, responsable_admin):
        """Obtiene o crea un conjunto de datos (con cache)"""
        if not nombre or not str(nombre).strip():
            return None

        nombre_limpio = str(nombre).strip()

        # Verificar cache primero
        if nombre_limpio in self.cache_conjuntos:
            return self.cache_conjuntos[nombre_limpio]

        # Buscar en BD
        conjunto = db.query(ConjuntoDatos).filter_by(nombre=nombre_limpio).first()

        if not conjunto:
            conjunto = ConjuntoDatos(
                nombre=nombre_limpio,
                descripcion=f"Conjunto de datos de demostración",
                responsable_id=responsable_admin.id,
                activo=True
            )
            db.add(conjunto)
            db.flush()
            self.stats['conjuntos_datos_creados'] += 1
            print(f"    📦 Creado conjunto: {nombre_limpio}")

        # Guardar en cache
        self.cache_conjuntos[nombre_limpio] = conjunto

        return conjunto

    def parsear_periodicidad(self, periodicidad_str):
        """Convierte la periodicidad del origen en una regla del motor.

        Una periodicidad que no se reconoce se queda como `INMEDIATO` —a
        demanda— y NO se le inventa un «cada 30 días». Rellenar el hueco produce
        un informe que parece configurado y vence cuando nadie dijo: es peor que
        uno sin cronograma, porque el que no lo tiene se ve a simple vista.
        """
        inicio = datetime.now().strftime('%Y-%m-%d')
        texto = str(periodicidad_str or '').strip()

        modo, valor = MAPA_PERIODICIDAD.get(texto, ('INMEDIATO', None))

        if modo == 'CICLICO':
            return {"modo": "CICLICO", "intervalo": valor,
                    "frecuencia": "DAILY", "fecha_inicio": inicio}
        if modo == 'SEMANAL':
            return {"modo": "SEMANAL", "dias_semana": valor,
                    "fecha_inicio": inicio}
        if modo == 'ESPECIFICO':
            # Entrega adelantada al hábil anterior: el vencimiento NO se mueve
            # —lo fija la norma— y adelantar nunca llega tarde, que es la
            # opción segura para un plazo legal.
            return {"modo": "ESPECIFICO", **valor,
                    "ajuste_habil_anterior": True, "fecha_inicio": inicio}
        return {"modo": "INMEDIATO",
                "descripcion": texto or "Sin periodicidad declarada",
                "fecha_inicio": inicio}

    def parsear_extensiones(self, presentacion):
        """Extrae extensiones permitidas desde el formato de presentación"""
        presentacion_str = str(presentacion).strip()

        extensiones_set = set()

        # Buscar en mapa de formatos
        for formato, exts in MAPA_FORMATOS.items():
            if formato.lower() in presentacion_str.lower():
                extensiones_set.update(exts)

        # Si no encontró nada, usar default
        if not extensiones_set:
            extensiones_set = {'xlsx', 'pdf'}

        return list(extensiones_set)

    def importar_informe(self, db, informe_data, admin_user):
        """Importa un informe individual desde JSON"""
        consecutivo = informe_data.get('consecutivo', 'N/A')

        try:
            # Verificar si ya existe
            nombre_informe = str(informe_data['nombre']).strip()[:200]

            if db.query(Informe).filter_by(nombre=nombre_informe).first():
                self.stats['omitidos'] += 1
                return False  # Ya existe

            # Obtener o crear conjunto de datos
            conjunto_datos = self.obtener_o_crear_conjunto_datos(
                db, informe_data.get('conjunto_datos'), admin_user
            )

            # Parsear periodicidad
            recurrencia_json = self.parsear_periodicidad(informe_data.get('periodicidad'))

            # El tipo de flujo ya no se asigna: se deduce de las partes.
            # Se conserva el mapeo para decidir QUÉ partes crear.
            flujo_declarado = MAPA_TIPO_FLUJO.get(
                str(informe_data.get('tipo_movimiento', '')).strip(),
                'ENVIO_INTERNO'
            )

            # Parsear extensiones
            extensiones = self.parsear_extensiones(informe_data.get('presentacion', 'Excel'))

            # Crear informe
            informe = Informe(
                nombre=nombre_informe,
                descripcion=str(informe_data.get('descripcion', ''))[:500],
                creador_id=admin_user.id,
                fecha_creacion=datetime.now(),
                recurrencia_json=recurrencia_json,
                activo=True,

                # Nuevos campos v3.0
                subsistema=str(informe_data.get('subsistema', ''))[:100] if informe_data.get('subsistema') else None,
                conjunto_datos_id=conjunto_datos.id if conjunto_datos else None,
                tiene_acuerdo_confidencialidad=informe_data.get('tiene_acuerdo', False),
                cargo_autorizador=str(informe_data.get('cargo_autorizador', ''))[:200] if informe_data.get('cargo_autorizador') else None,
                url_plantilla=str(informe_data.get('estructura_url', ''))[:500] if informe_data.get('estructura_url') else None,
                justificacion_normatividad=str(informe_data.get('justificacion', '')) if informe_data.get('justificacion') else None,
                aplicativo_origen=str(informe_data.get('aplicativo', ''))[:200] if informe_data.get('aplicativo') else None,
                criterios_generacion=str(informe_data.get('criterios_generacion', '')) if informe_data.get('criterios_generacion') else None,
                ruta_envio=str(informe_data.get('ruta_envio', ''))[:500] if informe_data.get('ruta_envio') else None,

                datos_generales={
                    'extensiones': extensiones,
                    'medio_envio': str(informe_data.get('medio_envio', '')),
                    'presentacion': str(informe_data.get('presentacion', '')),
                    'importado_desde': 'datos_demo.json',
                    'fecha_importacion': datetime.now().isoformat(),
                    'consecutivo_original': str(consecutivo),
                    'datos_ficticios': True  # Marcar como datos de demostración
                }
            )

            db.add(informe)
            db.flush()

            # Asignar responsable (admin por defecto) como ORIGEN del flujo.
            db.add(Participacion(informe_id=informe.id, usuario_id=admin_user.id,
                                 rol=ROL_ORIGEN, orden=0))

            self.stats['importados'] += 1
            return True

        except Exception as e:
            self.log_error(consecutivo, str(e))
            return False

    def ejecutar(self):
        """Ejecuta la importación completa"""
        print("=" * 80)
        print("  IMPORTACIÓN: datos_demo.json → Sistema v3.0")
        print("=" * 80)
        print("\n  ⚠️  DATOS FICTICIOS - Para propósitos de demostración únicamente\n")

        if self.dry_run:
            print("  ⚠️  MODO DRY-RUN: No se guardarán cambios en la base de datos\n")

        # Leer JSON
        informes = self.leer_json()

        # Obtener usuario admin
        print("\n🔐 Conectando a base de datos...")
        with get_db_session() as db:
            admin = db.query(Usuario).filter_by(es_admin=True).first()

            if not admin:
                print("❌ ERROR: No se encontró usuario administrador")
                print("   Ejecute primero: python seed_data.py")
                return

            print(f"  ✅ Usuario admin: {admin.nombre} ({admin.email})")

            # Importar informes
            print(f"\n📦 Importando {self.stats['total']} informes ficticios...")
            print("-" * 80)

            for informe_data in informes:
                consecutivo = informe_data.get('consecutivo', 'N/A')
                nombre = str(informe_data.get('nombre', 'Sin nombre'))[:60]

                if self.importar_informe(db, informe_data, admin):
                    print(f"  ✅ [{consecutivo}] {nombre}")
                else:
                    if self.stats['omitidos'] > self.stats['importados']:
                        print(f"  ⏭️  [{consecutivo}] {nombre} (ya existe)")
                    else:
                        print(f"  ❌ [{consecutivo}] {nombre} (error)")

                # Commit cada 50 registros
                if self.stats['importados'] % 50 == 0 and self.stats['importados'] > 0:
                    if not self.dry_run:
                        db.commit()
                        print(f"\n  💾 Guardados {self.stats['importados']} informes...\n")

            # Commit final
            if not self.dry_run:
                db.commit()
                print(f"\n  💾 Guardados todos los cambios")
            else:
                print(f"\n  ⚠️  Cambios NO guardados (modo dry-run)")

        # Mostrar estadísticas
        self.mostrar_estadisticas()

        # Mostrar errores
        if self.errores_log:
            self.mostrar_errores()

    def mostrar_estadisticas(self):
        """Muestra estadísticas de la importación"""
        print("\n" + "=" * 80)
        print("📊 ESTADÍSTICAS DE IMPORTACIÓN")
        print("=" * 80)
        print(f"  Total de registros procesados:     {self.stats['total']}")
        print(f"  ✅ Importados correctamente:        {self.stats['importados']}")
        print(f"  ⏭️  Omitidos (ya existían):          {self.stats['omitidos']}")
        print(f"  ❌ Errores:                          {self.stats['errores']}")
        print(f"  📦 Conjuntos de datos creados:      {self.stats['conjuntos_datos_creados']}")

        if self.stats['importados'] > 0:
            tasa_exito = (self.stats['importados'] / self.stats['total']) * 100
            print(f"\n  Tasa de éxito: {tasa_exito:.1f}%")

    def mostrar_errores(self):
        """Muestra log de errores"""
        print("\n" + "=" * 80)
        print("❌ ERRORES ENCONTRADOS")
        print("=" * 80)

        for i, error in enumerate(self.errores_log[:10], 1):
            print(f"\n{i}. Consecutivo {error['consecutivo']}:")
            print(f"   {error['mensaje']}")

        if len(self.errores_log) > 10:
            print(f"\n... y {len(self.errores_log) - 10} errores más")


def main():
    parser = argparse.ArgumentParser(
        description='Importar datos de demostración (ficticios) al sistema'
    )
    parser.add_argument('--dry-run', action='store_true',
                        help='Simular importación sin guardar')
    parser.add_argument('--limit', type=int,
                        help='Importar solo los primeros N registros')
    parser.add_argument('--archivo', type=str, default='datos_demo.json',
                        help='Archivo JSON a importar (default: datos_demo.json)')

    args = parser.parse_args()

    importador = ImportadorDatosDemo(
        archivo=args.archivo,
        dry_run=args.dry_run,
        limit=args.limit
    )

    try:
        importador.ejecutar()

        print("\n" + "=" * 80)
        if not args.dry_run:
            print("✅ IMPORTACIÓN COMPLETADA")
            print("=" * 80)
            print("\n📝 PRÓXIMOS PASOS:")
            print("  1. Verificar informes en UI: http://localhost:8080/gestor-informes")
            print("  2. Revisar conjuntos de datos: http://localhost:8080/conjuntos-datos")
            print("  3. Asignar responsables específicos a informes")
            print("  4. Ejecutar generación de tareas: /mis-pendientes → 'Actualizar Lista'")
            print("\n💡 NOTA: Estos son datos ficticios para demostración")
        else:
            print("✅ SIMULACIÓN COMPLETADA")
            print("=" * 80)
            print("\n📝 Para guardar cambios, ejecute:")
            print("  python importar_datos_demo.py")

        print("\n")

    except Exception as e:
        print(f"\n❌ ERROR FATAL: {e}")
        import traceback
        traceback.print_exc()
        sys.exit(1)


if __name__ == "__main__":
    main()
