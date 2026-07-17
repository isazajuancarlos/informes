# Datos de Demostración - Sistema de Gestión de Informes

**Fecha:** 8 de Enero, 2026
**Versión del Sistema:** 3.0

---

## ⚠️ IMPORTANTE: Uso de Datos Ficticios

Este repositorio utiliza **DATOS COMPLETAMENTE FICTICIOS** para propósitos de demostración y desarrollo.

### Por qué Datos Ficticios

Los datos originales provenían de un archivo Excel (`baseInformes.xlsx`) que contenía **información sensible de una entidad pública**, incluyendo:

- Nombres de secretarías y departamentos reales
- Ciudades y regiones específicas
- Correos electrónicos institucionales
- URLs de sistemas gubernamentales
- Nombres de aplicativos internos
- Referencias a normativas específicas

Por razones de **protección de datos** y **cumplimiento normativo**, estos datos fueron reemplazados por información sintética.

---

## 📊 Características de los Datos de Demostración

### Archivo Principal: `datos_demo.json`

```json
{
  "total_informes": 397,
  "fecha_generacion": "2026-01-08T...",
  "nota": "DATOS FICTICIOS - Generados automáticamente para demostración",
  "informes": [ ... ],
  "estadisticas": { ... }
}
```

### Lo que SE PRESERVÓ (Estructura)

✅ **Cantidad de registros**: 397 informes
✅ **Distribución por subsistema**:
   - Promoción y Prevención: 104 (26.2%)
   - Aseguramiento: 86 (21.7%)
   - Morbilidad: 77 (19.4%)
   - Estadísticas Vitales: 54 (13.6%)
   - Transversales: 52 (13.1%)
   - Salud Ambiental: 13 (3.3%)
   - Calidad y Redes de Servicios: 9 (2.3%)
   - Vigilancia Epidemiológica: 2 (0.5%)

✅ **Distribución por periodicidad**:
   - Mensual: 185 informes
   - Semanal: 96 informes
   - Trimestral: 41 informes
   - Anual: 32 informes
   - Semestral: 11 informes
   - Diaria: 5 informes
   - A Demanda: 9 informes
   - Otras: 18 informes

✅ **Tipos de flujo**:
   - Envío Interno (EI): 204 informes
   - Envío Externo (EE): 140 informes
   - Recepción Externa (RE): 53 informes

✅ **Estructura de campos**: Todos los campos del modelo v3.0
✅ **Relaciones**: Conjuntos de datos, responsables, stakeholders
✅ **Reglas de negocio**: Variables y consecuencias

### Lo que FUE REEMPLAZADO (Contenido)

❌ **Nombres de entidades**: "Secretaría de Salud Municipal" en lugar de nombres reales
❌ **Ciudades**: "Ciudad Central", "Nueva Esperanza" en lugar de nombres reales
❌ **Emails**: "reportes123@salud.demo.gov" en lugar de emails institucionales
❌ **URLs**: "https://portal.saluddemo.gov/modulo42" en lugar de URLs reales
❌ **Nombres de informes**: Generados automáticamente con plantillas
❌ **Descripciones**: Texto genérico manteniendo coherencia
❌ **Aplicativos**: Nombres genéricos de sistemas

---

## 🔧 Cómo Usar los Datos de Demostración

### 1. Generar Datos Ficticios (Ya ejecutado)

```bash
python generar_datos_ficticios.py
```

**Salida**: `datos_demo.json` (397 informes ficticios)

### 2. Importar a la Base de Datos

```bash
# Modo prueba (DRY RUN) - No guarda cambios
python importar_datos_demo.py --dry-run

# Importación real
python importar_datos_demo.py

# Importar solo los primeros 50 para pruebas rápidas
python importar_datos_demo.py --limit 50
```

### 3. Verificar en la UI

```bash
python main.py
```

Acceder a: `http://localhost:8080`

**Rutas importantes**:
- `/gestor-informes` - Ver informes importados
- `/conjuntos-datos` - Ver conjuntos de datos creados
- `/mis-pendientes` - Generar tareas de demostración

---

## 📁 Archivos y Estructura

### Archivos Principales

```
/mnt/data/informes/
├── datos_demo.json                    # 397 informes ficticios (USAR ESTE)
├── generar_datos_ficticios.py         # Generador de datos sintéticos
├── importar_datos_demo.py             # Importador para datos de demo
├── migrate_baseinformes.py            # Migración de BD v2.0 → v3.0
│
├── archivado_datos_sensibles/         # ⚠️ NO USAR - Datos reales archivados
│   ├── baseInformes.xlsx              # Excel original (SENSIBLE)
│   ├── datos_prueba_extraidos.json    # JSON extraído (SENSIBLE)
│   ├── importar_baseinformes.py       # Importador original (obsoleto)
│   └── README.md                      # Explicación del archivo
│
└── DATOS_DEMO_README.md               # Este archivo
```

### Archivos de Documentación

- **CLAUDE.md** - Guía para Claude Code (actualizada con datos demo)
- **UPGRADE_V3.md** - Guía de migración v2.0 → v3.0
- **MANUAL_USUARIO.md** - Manual de usuario del sistema
- **INFORME_ANALISIS_BASEINFORMES.md** - Análisis técnico del archivo original

---

## 🎲 Generación de Datos Ficticios

### Algoritmo de Generación

El script `generar_datos_ficticios.py` utiliza:

1. **Seed fija (42)**: Reproducibilidad garantizada
2. **Distribuciones estadísticas**: Respeta proporciones originales
3. **Plantillas de texto**: Nombres y descripciones coherentes
4. **Datos aleatorios**: Ciudades, entidades, emails, URLs ficticios
5. **Validación**: Estructura idéntica al formato original

### Ejemplo de Registro Generado

```json
{
  "consecutivo": "001",
  "nombre": "Análisis de Controles Prenatales por Periodo",
  "descripcion": "Informe que presenta auditorías realizadas según criterios establecidos...",
  "subsistema": "Promoción y Prevención",
  "conjunto_datos": "Datos Epidemiológicos",
  "periodicidad": "Semanal",
  "tipo_movimiento": "EI",
  "aplicativo": "Sistema de Registro Electrónico",
  "presentacion": "XML",
  "medio_envio": "Correo Electrónico",
  "estructura_url": "https://reportes.saluddemo.gov/modulo36"
}
```

---

## 🔒 Seguridad y Cumplimiento

### Datos Archivados

Los datos reales han sido movidos a: `archivado_datos_sensibles/`

**⚠️ NO USAR archivos de esta carpeta en el sistema**

### Verificación de Datos Ficticios

Todos los informes importados desde `datos_demo.json` tienen en `datos_generales`:

```json
{
  "datos_ficticios": true,
  "importado_desde": "datos_demo.json",
  "fecha_importacion": "2026-01-08T..."
}
```

---

## 📊 Estadísticas de Generación

### Resumen

- **Total de informes**: 397
- **Subsistemas**: 8 categorías
- **Periodicidades**: 13 tipos diferentes
- **Conjuntos de datos**: 30+ únicos
- **Aplicativos**: 15 sistemas ficticios
- **Formatos**: 10 tipos de presentación
- **Medios de envío**: 10 opciones

### Distribuciones Exactas

Ver salida de:
```bash
python generar_datos_ficticios.py
```

---

## 🧪 Testing con Datos de Demostración

### Pruebas Recomendadas

1. **Importación completa**:
   ```bash
   python importar_datos_demo.py
   ```

2. **Generación de tareas**:
   - Acceder a `/mis-pendientes`
   - Clic en "Actualizar Lista"
   - Verificar generación de entregas esperadas

3. **Creación de conjuntos de datos**:
   - Acceder a `/conjuntos-datos`
   - Verificar 30+ conjuntos creados

4. **Creación de entregas manuales**:
   - Acceder a `/entregas-manuales`
   - Crear entrega "A Demanda"

5. **Flujo completo de operador**:
   - Login como operador@co.co
   - Ver tareas pendientes
   - Simular carga de archivo
   - Verificar cambio de estado

---

## 🔄 Regenerar Datos

Si necesitas regenerar los datos ficticios:

```bash
# 1. Regenerar JSON (opcional, cambia semilla para datos diferentes)
python generar_datos_ficticios.py

# 2. Limpiar BD (CUIDADO: borra TODO)
python reset_total.py

# 3. Reimportar datos
python importar_datos_demo.py
```

---

## 📞 Soporte

### Preguntas Frecuentes

**P: ¿Puedo usar los datos archivados?**
R: No, están archivados por contener información sensible de entidad pública.

**P: ¿Los datos ficticios funcionan igual que los reales?**
R: Sí, tienen la misma estructura y distribuciones estadísticas.

**P: ¿Puedo modificar el generador?**
R: Sí, edita `generar_datos_ficticios.py` y ejecuta nuevamente.

**P: ¿Cómo identifico datos ficticios en la BD?**
R: Campo `datos_generales.datos_ficticios = true` en cada informe.

---

## 📝 Changelog

### 8 de Enero, 2026
- ✅ Creación de generador de datos ficticios
- ✅ Generación de 397 informes sintéticos
- ✅ Archivo de datos sensibles originales
- ✅ Actualización de documentación
- ✅ Creación de script de importación para datos demo

---

**Versión**: 1.0
**Autor**: Claude Code
**Compatible con**: Sistema v3.0
**Licencia**: Uso interno/educativo

Para más información, consultar `CLAUDE.md` o `UPGRADE_V3.md`.
