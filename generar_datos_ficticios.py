"""
Generador de Datos Ficticios para Sistema de Gestión de Informes
==================================================================

Este script genera 397 informes ficticios que preservan la estructura
y distribuciones del archivo original, pero con datos completamente sintéticos.

IMPORTANTE: Este script reemplaza datos sensibles de entidad pública
por información ficticia para propósitos de demostración.

Ejecutar con:
    python generar_datos_ficticios.py

Genera:
    - datos_demo.json: 397 informes ficticios
    - estadisticas_demo.json: Estadísticas para validación

Autor: Claude Code
Fecha: 8 de Enero, 2026
"""

import json
import random
from datetime import datetime, timedelta

# ============================================================================
# CONFIGURACIÓN: Listas de datos ficticios para generación
# ============================================================================

CIUDADES_FICTICIAS = [
    "Ciudad Central", "Nueva Esperanza", "Villa Progreso", "San Martín",
    "Puerto Nuevo", "Las Colinas", "El Bosque", "Río Verde", "Valle Alto",
    "Costa Azul", "Sierra Nevada", "Bella Vista", "Monte Claro", "Primavera"
]

ENTIDADES_FICTICIAS = [
    "Secretaría de Salud Municipal",
    "Departamento de Salud Pública",
    "Oficina de Estadísticas de Salud",
    "Dirección de Epidemiología",
    "Centro de Control Sanitario",
    "Instituto de Salud Regional",
    "Servicio de Salud Comunitaria"
]

SUBSISTEMAS = [
    "Promoción y Prevención",
    "Aseguramiento",
    "Morbilidad",
    "Estadísticas Vitales",
    "Transversales",
    "Salud Ambiental",
    "Calidad y Redes de Servicios",
    "Vigilancia Epidemiológica"
]

# Distribución real del archivo original (397 informes)
DISTRIBUCION_SUBSISTEMAS = {
    "Promoción y Prevención": 104,
    "Aseguramiento": 86,
    "Morbilidad": 77,
    "Estadísticas Vitales": 54,
    "Transversales": 52,
    "Salud Ambiental": 13,
    "Calidad y Redes de Servicios": 9,
    "Vigilancia Epidemiológica": 2
}

DISTRIBUCION_PERIODICIDADES = {
    "Mensual": 185,
    "Semanal": 96,
    "Trimestral": 41,
    "Anual": 32,
    "Semestral": 11,
    "Diaria": 5,
    "Inmediatamente sea requerido": 5,
    "Quincenal": 4,
    "2 veces por semana": 2,
    "Por solicitud": 2,
    "Cuando lleguen requerimientos eventuales": 2,
    "Bimestral": 5,
    "Trimestral o cuando es requerido": 1,
    "Dependiendo del corte definido": 1,
}

DISTRIBUCION_TIPOS_MOVIMIENTO = {
    "EI": 204,  # Envío Interno
    "EE": 140,  # Envío Externo
    "RE": 53    # Recepción Externa
}

CONJUNTOS_DATOS = [
    "Registros de Atención", "Base de Usuarios", "Indicadores de Salud",
    "Programas Preventivos", "Estadísticas Demográficas", "Datos Epidemiológicos",
    "Información Administrativa", "Recursos Hospitalarios", "Afiliaciones",
    "Consultas Médicas", "Procedimientos", "Urgencias", "Hospitalizaciones",
    "Reportes de Calidad", "Auditorías", "Inventarios", "Personal Médico",
    "Infraestructura", "Finanzas", "Presupuestos", "Contratos", "Proveedores",
    "Medicamentos", "Insumos", "Equipamiento", "Mantenimiento", "Seguridad",
    "Medio Ambiente", "Residuos", "Agua Potable", "Saneamiento", "Vectores"
]

APLICATIVOS = [
    "Sistema Integral de Salud (SIS)", "Base de Datos Centralizada",
    "Plataforma de Reportes", "Módulo de Estadísticas", "Portal Web Institucional",
    "Sistema de Información Hospitalaria", "Software de Gestión Clínica",
    "Aplicativo de Seguimiento", "Herramienta de Consolidación",
    "Sistema de Registro Electrónico", "Base de Datos SQL Server",
    "Aplicativo Web de Validación", "Plataforma de Análisis",
    "Sistema de Gestión Documental", "Portal de Consultas"
]

FORMATOS_PRESENTACION = [
    "Excel", "PDF", "Archivos Planos", "Word", "CSV", "Plano (TXT)",
    "XML", "JSON", "Base de Datos", "Formulario Web"
]

MEDIOS_ENVIO = [
    "Correo Electrónico", "Portal Web Institucional", "SFTP", "FTP",
    "Sistema de Transferencia de Archivos", "Físico (CD/USB)", "Oficio",
    "Aplicativo Web", "API REST", "Correo Certificado"
]

# ============================================================================
# PLANTILLAS DE NOMBRES Y DESCRIPCIONES
# ============================================================================

PLANTILLAS_NOMBRES = [
    "Reporte de {dato} {periodicidad_adj}",
    "Consolidado de {dato}",
    "Informe {tipo} de {dato}",
    "Seguimiento de {dato}",
    "Análisis de {dato} por Periodo",
    "Estadísticas de {dato}",
    "Control de {dato}",
    "Monitoreo de {dato}",
    "Registro de {dato}",
    "Gestión de {dato}",
    "Evaluación de {dato}",
    "Indicadores de {dato}",
    "Resumen {periodicidad_adj} de {dato}",
    "Inventario de {dato}",
    "Seguimiento {periodicidad_adj} de {dato}",
]

PLANTILLAS_DESCRIPCIONES = [
    "Recopilación y análisis de información relacionada con {dato}, procesada desde {sistema} con periodicidad {periodicidad}.",
    "Consolidación de datos de {dato} provenientes de {fuente}, generado para {destinatario}.",
    "Informe que presenta {dato} según criterios establecidos, incluyendo análisis por periodo y segmentación.",
    "Seguimiento detallado de {dato} con indicadores clave de desempeño y métricas de cumplimiento.",
    "Reporte generado desde {sistema} que consolida información de {dato} para análisis y toma de decisiones.",
    "Documento que registra {dato} con el objetivo de mantener trazabilidad y control de gestión.",
    "Análisis estadístico de {dato} diferenciado por variables demográficas y temporales.",
    "Control y monitoreo de {dato} para garantizar cumplimiento normativo y operativo.",
]

DATOS_EJEMPLO = [
    "consultas médicas", "procedimientos quirúrgicos", "atenciones de urgencia",
    "hospitalizaciones", "servicios ambulatorios", "programas de prevención",
    "campañas de vacunación", "controles prenatales", "crecimiento y desarrollo",
    "enfermedades crónicas", "afiliados al sistema", "población atendida",
    "indicadores de calidad", "satisfacción del usuario", "tiempos de espera",
    "recursos humanos", "inventario de medicamentos", "equipamiento médico",
    "infraestructura hospitalaria", "gestión financiera", "presupuesto ejecutado",
    "contratos vigentes", "proveedores autorizados", "auditorías realizadas",
    "eventos adversos", "incidentes de seguridad", "residuos hospitalarios",
    "calidad del agua", "control de vectores", "saneamiento básico"
]

# ============================================================================
# GENERADOR DE DATOS FICTICIOS
# ============================================================================

class GeneradorDatosFicticios:
    def __init__(self, seed=42):
        """Inicializa el generador con semilla para reproducibilidad"""
        random.seed(seed)
        self.consecutivo = 1
        self.informes_generados = []
        self.estadisticas = {
            'subsistemas': {},
            'periodicidades': {},
            'tipos_movimiento': {},
            'conjuntos_datos': {},
        }

    def generar_email_ficticio(self):
        """Genera email institucional ficticio"""
        dominios = ["salud.demo.gov", "informes.demo.gov", "sistema.demo.gov", "reportes.demo.gov"]
        prefijos = ["reportes", "informes", "datos", "estadisticas", "gestion", "registro", "control"]
        return f"{random.choice(prefijos)}{random.randint(1,999)}@{random.choice(dominios)}"

    def generar_url_ficticia(self):
        """Genera URL ficticia"""
        subdominios = ["reportes", "portal", "sistema", "plataforma", "datos", "informes"]
        return f"https://{random.choice(subdominios)}.saluddemo.gov/modulo{random.randint(1,50)}"

    def generar_ruta_ficticia(self):
        """Genera ruta de archivo ficticia"""
        carpetas = ["reportes", "informes", "datos", "estadisticas", "archivos", "documentos"]
        return f"/sistema/datos/{random.choice(carpetas)}/anio_{{año}}/mes_{{mes}}"

    def generar_nombre_informe(self, subsistema, periodicidad):
        """Genera nombre de informe ficticio"""
        plantilla = random.choice(PLANTILLAS_NOMBRES)
        dato = random.choice(DATOS_EJEMPLO)

        periodicidad_adj = {
            "Mensual": "Mensual", "Semanal": "Semanal", "Diaria": "Diario",
            "Trimestral": "Trimestral", "Anual": "Anual", "Semestral": "Semestral",
            "Quincenal": "Quincenal", "Bimestral": "Bimestral"
        }.get(periodicidad, "Periódico")

        tipo_opciones = ["Estadístico", "Ejecutivo", "Operativo", "Técnico", "Consolidado"]

        nombre = plantilla.format(
            dato=dato.title(),
            periodicidad_adj=periodicidad_adj,
            tipo=random.choice(tipo_opciones)
        )

        return nombre

    def generar_descripcion(self, nombre, subsistema):
        """Genera descripción ficticia"""
        plantilla = random.choice(PLANTILLAS_DESCRIPCIONES)
        dato = random.choice(DATOS_EJEMPLO)
        sistema = random.choice(APLICATIVOS)
        fuente = random.choice(["múltiples fuentes de datos", "sistemas integrados", "bases de datos institucionales"])
        destinatario = random.choice(ENTIDADES_FICTICIAS)
        periodicidad = random.choice(list(DISTRIBUCION_PERIODICIDADES.keys()))

        return plantilla.format(
            dato=dato,
            sistema=sistema,
            fuente=fuente,
            destinatario=destinatario,
            periodicidad=periodicidad.lower()
        )

    def generar_informe_ficticio(self, subsistema, periodicidad, tipo_movimiento):
        """Genera un informe ficticio completo"""
        nombre = self.generar_nombre_informe(subsistema, periodicidad)
        descripcion = self.generar_descripcion(nombre, subsistema)

        informe = {
            "consecutivo": f"{self.consecutivo:03d}",
            "nombre": nombre,
            "descripcion": descripcion,
            "subsistema": subsistema,
            "conjunto_datos": random.choice(CONJUNTOS_DATOS),
            "periodicidad": periodicidad,
            "tipo_movimiento": tipo_movimiento,
            "aplicativo": random.choice(APLICATIVOS),
            "presentacion": random.choice(FORMATOS_PRESENTACION),
            "medio_envio": random.choice(MEDIOS_ENVIO),
        }

        # Agregar campos opcionales aleatorios
        if random.random() > 0.7:
            informe["justificacion"] = f"Normativa Interna {random.randint(1000,9999)} de {random.randint(2015,2024)}"

        if random.random() > 0.6:
            informe["criterios_generacion"] = f"Filtrado por periodo, estado activo, criterio {random.randint(1,10)}"

        if random.random() > 0.5:
            informe["estructura_url"] = self.generar_url_ficticia()

        if random.random() > 0.7 and tipo_movimiento in ["EE", "RE"]:
            informe["ruta_envio"] = self.generar_ruta_ficticia()

        if tipo_movimiento == "RE":
            informe["medio_envio"] = f"{informe['medio_envio']} / {self.generar_email_ficticio()}"

        self.consecutivo += 1
        return informe

    def generar_todos_informes(self):
        """Genera los 397 informes respetando distribuciones reales"""
        print("🎲 Generando 397 informes ficticios...")
        print("=" * 80)

        # Generar informes por subsistema
        for subsistema, cantidad in DISTRIBUCION_SUBSISTEMAS.items():
            print(f"   Generando {cantidad:3d} informes de '{subsistema}'...")

            for i in range(cantidad):
                # Seleccionar periodicidad respetando distribución
                periodicidad = self._seleccionar_periodicidad()

                # Seleccionar tipo de movimiento respetando distribución
                tipo_movimiento = self._seleccionar_tipo_movimiento()

                informe = self.generar_informe_ficticio(subsistema, periodicidad, tipo_movimiento)
                self.informes_generados.append(informe)

                # Actualizar estadísticas
                self.estadisticas['subsistemas'][subsistema] = \
                    self.estadisticas['subsistemas'].get(subsistema, 0) + 1
                self.estadisticas['periodicidades'][periodicidad] = \
                    self.estadisticas['periodicidades'].get(periodicidad, 0) + 1
                self.estadisticas['tipos_movimiento'][tipo_movimiento] = \
                    self.estadisticas['tipos_movimiento'].get(tipo_movimiento, 0) + 1

                conjunto = informe['conjunto_datos']
                self.estadisticas['conjuntos_datos'][conjunto] = \
                    self.estadisticas['conjuntos_datos'].get(conjunto, 0) + 1

        print(f"\n✅ Total generados: {len(self.informes_generados)} informes")
        return self.informes_generados

    def _seleccionar_periodicidad(self):
        """Selecciona periodicidad con probabilidad según distribución real"""
        periodicidades = list(DISTRIBUCION_PERIODICIDADES.keys())
        pesos = list(DISTRIBUCION_PERIODICIDADES.values())
        return random.choices(periodicidades, weights=pesos, k=1)[0]

    def _seleccionar_tipo_movimiento(self):
        """Selecciona tipo de movimiento con probabilidad según distribución real"""
        tipos = list(DISTRIBUCION_TIPOS_MOVIMIENTO.keys())
        pesos = list(DISTRIBUCION_TIPOS_MOVIMIENTO.values())
        return random.choices(tipos, weights=pesos, k=1)[0]

    def exportar_json(self, archivo_salida='datos_demo.json'):
        """Exporta los informes a archivo JSON"""
        datos = {
            "total_informes": len(self.informes_generados),
            "fecha_generacion": datetime.now().isoformat(),
            "nota": "DATOS FICTICIOS - Generados automáticamente para demostración",
            "informes": self.informes_generados,
            "estadisticas": self.estadisticas
        }

        with open(archivo_salida, 'w', encoding='utf-8') as f:
            json.dump(datos, f, ensure_ascii=False, indent=2)

        print(f"\n💾 Archivo generado: {archivo_salida}")
        return archivo_salida

    def mostrar_resumen(self):
        """Muestra resumen de generación"""
        print("\n" + "=" * 80)
        print("📊 RESUMEN DE GENERACIÓN")
        print("=" * 80)

        print(f"\n🔢 Total de informes: {len(self.informes_generados)}")

        print("\n📋 Por Subsistema:")
        for subsistema, cantidad in sorted(self.estadisticas['subsistemas'].items(),
                                           key=lambda x: x[1], reverse=True):
            print(f"   • {subsistema:45s}: {cantidad:3d} ({cantidad/len(self.informes_generados)*100:5.1f}%)")

        print("\n⏰ Por Periodicidad:")
        for periodo, cantidad in sorted(self.estadisticas['periodicidades'].items(),
                                        key=lambda x: x[1], reverse=True):
            print(f"   • {periodo:45s}: {cantidad:3d} ({cantidad/len(self.informes_generados)*100:5.1f}%)")

        print("\n🔄 Por Tipo de Movimiento:")
        for tipo, cantidad in sorted(self.estadisticas['tipos_movimiento'].items()):
            tipo_nombre = {"EI": "Envío Interno", "EE": "Envío Externo", "RE": "Recepción Externa"}[tipo]
            print(f"   • {tipo_nombre:45s}: {cantidad:3d} ({cantidad/len(self.informes_generados)*100:5.1f}%)")


# ============================================================================
# MAIN
# ============================================================================

def main():
    print("\n" + "=" * 80)
    print(" GENERADOR DE DATOS FICTICIOS - Sistema de Gestión de Informes")
    print("=" * 80)
    print("\n⚠️  IMPORTANTE: Este script genera datos completamente ficticios")
    print("   para reemplazar información sensible de entidad pública.\n")

    generador = GeneradorDatosFicticios(seed=42)  # Semilla fija para reproducibilidad

    # Generar informes
    generador.generar_todos_informes()

    # Exportar a JSON
    generador.exportar_json('datos_demo.json')

    # Mostrar resumen
    generador.mostrar_resumen()

    print("\n" + "=" * 80)
    print("✅ Generación completada exitosamente")
    print("=" * 80)
    print("\n📁 Archivos generados:")
    print("   • datos_demo.json - 397 informes ficticios para demostración")
    print("\n💡 Siguiente paso:")
    print("   python importar_baseinformes.py --archivo datos_demo.json --dry-run")
    print("\n")


if __name__ == "__main__":
    main()
