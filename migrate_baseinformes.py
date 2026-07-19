"""OBSOLETO — migración de la era Excel (esquema v2 -> v3).

NO CORRER sobre una base ya migrada al flujo unificado: asigna
`informe.tipo_flujo`, que desde el refactor es una propiedad DERIVADA de solo
lectura, y fallará. Se conserva por valor histórico y para bases que se hayan
quedado atrás; para esas, el orden correcto es este guion primero y
`migrar_flujo_unificado.py` después.

Script de Migración: BaseInformes v2.0 → v3.0
Agrega nuevas funcionalidades identificadas en análisis de baseInformes.xlsx

IMPORTANTE: Este script agrega nuevos campos y tablas sin eliminar datos existentes.

Ejecutar con:
    python migrate_baseinformes.py

Autor: Claude Code
Fecha: 8 de Enero, 2026
"""

import sys
import os
sys.path.append(os.path.dirname(os.path.abspath(__file__)))

from sqlalchemy import text, inspect
from app.core.database import engine, get_db_session
from app.models.meta_tables import Base, ConjuntoDatos, Informe, EntregaEsperada
from datetime import datetime

def ejecutar_sql_seguro(sql, descripcion=""):
    """Ejecuta SQL y maneja errores de forma segura"""
    with engine.connect() as conn:
        try:
            conn.execute(text(sql))
            conn.commit()
            print(f"  ✅ {descripcion}")
            return True
        except Exception as e:
            error_msg = str(e)
            if "already exists" in error_msg or "duplicate column" in error_msg:
                print(f"  ⚠️  {descripcion} (ya existe, omitiendo)")
                return True
            else:
                print(f"  ❌ ERROR en {descripcion}: {e}")
                return False

def verificar_columna_existe(tabla, columna):
    """Verifica si una columna ya existe en una tabla"""
    inspector = inspect(engine)
    columnas = [col['name'] for col in inspector.get_columns(tabla)]
    return columna in columnas

def crear_tabla_conjuntos_datos():
    """Crea la nueva tabla de Conjuntos de Datos"""
    print("\n📦 CREANDO TABLA: conjuntos_datos")
    print("-" * 80)

    sql = """
    CREATE TABLE IF NOT EXISTS conjuntos_datos (
        id SERIAL PRIMARY KEY,
        nombre VARCHAR(200) NOT NULL UNIQUE,
        descripcion TEXT,
        responsable_id INTEGER REFERENCES usuarios(id),
        activo BOOLEAN DEFAULT TRUE,
        fecha_creacion TIMESTAMP DEFAULT CURRENT_TIMESTAMP
    );
    CREATE INDEX IF NOT EXISTS idx_conjuntos_datos_nombre ON conjuntos_datos(nombre);
    """
    return ejecutar_sql_seguro(sql, "Tabla conjuntos_datos")

def agregar_campos_informe():
    """Agrega nuevos campos a la tabla cat_informes"""
    print("\n📋 ACTUALIZANDO TABLA: cat_informes")
    print("-" * 80)

    campos = [
        ("tipo_flujo", "VARCHAR(20) DEFAULT 'ENVIO_INTERNO'", "Tipo de flujo del informe"),
        ("subsistema", "VARCHAR(100)", "Subsistema del informe"),
        ("conjunto_datos_id", "INTEGER REFERENCES conjuntos_datos(id)", "Relación con conjunto de datos"),
        ("tiene_acuerdo_confidencialidad", "BOOLEAN DEFAULT FALSE", "Indicador de acuerdo de confidencialidad"),
        ("cargo_autorizador", "VARCHAR(200)", "Cargo del autorizador"),
        ("url_plantilla", "VARCHAR(500)", "URL de plantilla del informe"),
        ("ruta_plantilla", "VARCHAR(500)", "Ruta física de plantilla"),
        ("justificacion_normatividad", "TEXT", "Justificación o normatividad"),
        ("aplicativo_origen", "VARCHAR(200)", "Aplicativo/herramienta origen"),
        ("criterios_generacion", "TEXT", "Criterios de generación"),
        ("ruta_envio", "VARCHAR(500)", "Ruta de envío (SFTP/FTP)"),
        ("ruta_respaldo", "VARCHAR(500)", "Ruta de respaldo"),
        ("fecha_corte_descripcion", "VARCHAR(200)", "Descripción de fecha de corte"),
        ("dias_limite_descripcion", "VARCHAR(200)", "Descripción de días límite"),
    ]

    for campo, tipo, desc in campos:
        if not verificar_columna_existe('cat_informes', campo):
            sql = f"ALTER TABLE cat_informes ADD COLUMN {campo} {tipo};"
            ejecutar_sql_seguro(sql, desc)
        else:
            print(f"  ⚠️  {desc} (ya existe)")

def agregar_campos_asignaciones():
    """Agrega campos a la tabla de asignaciones para roles diferenciados"""
    print("\n👥 ACTUALIZANDO TABLA: asignaciones_informe_usuario")
    print("-" * 80)

    campos = [
        ("tipo_responsabilidad", "VARCHAR(20) DEFAULT 'EJECUTA'", "Tipo de responsabilidad"),
        ("es_principal", "BOOLEAN DEFAULT TRUE", "Indicador de responsable principal"),
        ("orden", "INTEGER DEFAULT 0", "Orden del responsable"),
    ]

    for campo, tipo, desc in campos:
        if not verificar_columna_existe('asignaciones_informe_usuario', campo):
            sql = f"ALTER TABLE asignaciones_informe_usuario ADD COLUMN {campo} {tipo};"
            ejecutar_sql_seguro(sql, desc)
        else:
            print(f"  ⚠️  {desc} (ya existe)")

def agregar_campos_entregas():
    """Agrega campos para entregas manuales (A Demanda)"""
    print("\n📝 ACTUALIZANDO TABLA: entregas_esperadas")
    print("-" * 80)

    campos = [
        ("es_manual", "BOOLEAN DEFAULT FALSE", "Indicador de creación manual"),
        ("creador_manual_id", "INTEGER REFERENCES usuarios(id)", "Usuario que creó manualmente"),
        ("notas_admin", "TEXT", "Notas del administrador"),
    ]

    for campo, tipo, desc in campos:
        if not verificar_columna_existe('entregas_esperadas', campo):
            sql = f"ALTER TABLE entregas_esperadas ADD COLUMN {campo} {tipo};"
            ejecutar_sql_seguro(sql, desc)
        else:
            print(f"  ⚠️  {desc} (ya existe)")

def crear_indices():
    """Crea índices para mejorar el rendimiento"""
    print("\n🔍 CREANDO ÍNDICES")
    print("-" * 80)

    indices = [
        ("idx_cat_informes_tipo_flujo", "cat_informes", "tipo_flujo"),
        ("idx_cat_informes_subsistema", "cat_informes", "subsistema"),
        ("idx_entregas_esperadas_es_manual", "entregas_esperadas", "es_manual"),
    ]

    for idx_name, tabla, columna in indices:
        sql = f"CREATE INDEX IF NOT EXISTS {idx_name} ON {tabla}({columna});"
        ejecutar_sql_seguro(sql, f"Índice {idx_name}")

def migrar_datos_json_a_columnas():
    """Migra datos del campo datos_generales JSON a las nuevas columnas"""
    print("\n🔄 MIGRANDO DATOS DE JSON A COLUMNAS")
    print("-" * 80)

    with get_db_session() as db:
        informes = db.query(Informe).all()
        count = 0

        for informe in informes:
            modificado = False

            # Migrar subsistema si está en datos_generales
            if informe.datos_generales and not informe.subsistema:
                subsistema = informe.datos_generales.get('subsistema')
                if subsistema:
                    informe.subsistema = subsistema
                    modificado = True

            # Migrar tipo_movimiento a tipo_flujo
            if informe.datos_generales:
                tipo_mov = informe.datos_generales.get('tipo_movimiento')
                if tipo_mov and not informe.tipo_flujo:
                    # Mapear códigos
                    mapa = {
                        'EI': 'ENVIO_INTERNO',
                        'EE': 'ENVIO_EXTERNO',
                        'RE': 'RECEPCION_EXTERNA',
                        'BI': 'BIDIRECCIONAL'
                    }
                    informe.tipo_flujo = mapa.get(tipo_mov, 'ENVIO_INTERNO')
                    modificado = True

            if modificado:
                count += 1

        if count > 0:
            db.commit()
            print(f"  ✅ Migrados {count} informes")
        else:
            print(f"  ℹ️  No hay datos JSON para migrar")

def verificar_migracion():
    """Verifica que la migración se haya completado correctamente"""
    print("\n🔍 VERIFICANDO MIGRACIÓN")
    print("-" * 80)

    with get_db_session() as db:
        # Verificar tabla conjuntos_datos
        try:
            count = db.query(ConjuntoDatos).count()
            print(f"  ✅ Tabla conjuntos_datos: {count} registros")
        except Exception as e:
            print(f"  ❌ Error en conjuntos_datos: {e}")

        # Verificar campos en Informe
        informe_test = db.query(Informe).first()
        if informe_test:
            nuevos_campos = ['tipo_flujo', 'subsistema', 'tiene_acuerdo_confidencialidad',
                            'url_plantilla', 'aplicativo_origen']
            for campo in nuevos_campos:
                if hasattr(informe_test, campo):
                    print(f"  ✅ Campo Informe.{campo}: OK")
                else:
                    print(f"  ❌ Campo Informe.{campo}: FALTA")

        # Verificar campos en EntregaEsperada
        entrega_test = db.query(EntregaEsperada).first()
        if entrega_test:
            if hasattr(entrega_test, 'es_manual'):
                print(f"  ✅ Campo EntregaEsperada.es_manual: OK")
            else:
                print(f"  ❌ Campo EntregaEsperada.es_manual: FALTA")

def main():
    """Ejecuta la migración completa"""
    print("=" * 80)
    print("  MIGRACIÓN: BaseInformes v2.0 → v3.0")
    print("  Análisis: baseInformes.xlsx")
    print("=" * 80)

    print("\n⚠️  ADVERTENCIA:")
    print("  Esta migración agregará nuevas columnas y tablas a la base de datos.")
    print("  Los datos existentes NO se eliminarán.")
    print("\n  Es RECOMENDABLE hacer un backup antes de continuar.")
    print("  Ejecutar: python app/services/backup_service.py")

    respuesta = input("\n¿Desea continuar? (s/n): ").lower()
    if respuesta != 's':
        print("\n❌ Migración cancelada por el usuario")
        return

    print("\n" + "=" * 80)
    print("INICIANDO MIGRACIÓN")
    print("=" * 80)

    # Paso 1: Crear nueva tabla
    if not crear_tabla_conjuntos_datos():
        print("\n❌ Error crítico al crear tabla. Abortando.")
        return

    # Paso 2: Agregar campos a cat_informes
    agregar_campos_informe()

    # Paso 3: Agregar campos a asignaciones
    agregar_campos_asignaciones()

    # Paso 4: Agregar campos a entregas_esperadas
    agregar_campos_entregas()

    # Paso 5: Crear índices
    crear_indices()

    # Paso 6: Migrar datos JSON a columnas
    migrar_datos_json_a_columnas()

    # Paso 7: Verificar
    verificar_migracion()

    print("\n" + "=" * 80)
    print("✅ MIGRACIÓN COMPLETADA")
    print("=" * 80)

    print("\n📝 PRÓXIMOS PASOS:")
    print("  1. Ejecutar: python importar_baseinformes.py")
    print("  2. Verificar datos en UI")
    print("  3. Probar funcionalidades nuevas")
    print("\n")

if __name__ == "__main__":
    try:
        main()
    except Exception as e:
        print(f"\n❌ ERROR FATAL: {e}")
        import traceback
        traceback.print_exc()
        sys.exit(1)
