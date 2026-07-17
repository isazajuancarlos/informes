# Guía de Actualización v2.0 → v3.0

**Sistema Gestión de Informes - Actualización Mayor**

Fecha: 8 de Enero, 2026
Autor: Claude Code

---

## 📋 Resumen Ejecutivo

Esta actualización incorpora las funcionalidades identificadas en el análisis del archivo `baseInformes.xlsx`, expandiendo significativamente las capacidades del sistema para gestionar flujos de información complejos, conjuntos de datos normalizados y entregas manuales.

### Novedades Principales

1. **Tipo de Flujo de Información**: Clasificación de informes por dirección (Envío Interno, Envío Externo, Recepción Externa, Bidireccional)
2. **Conjuntos de Datos**: Catálogo normalizado para agrupar informes (RIPS, RUAF, Vacunación, etc.)
3. **Entregas Manuales (A Demanda)**: Creación de tareas sin esperar al scheduler automático
4. **Acuerdos de Confidencialidad**: Tracking de requerimientos legales
5. **Plantillas**: Vinculación de URLs y rutas de plantillas
6. **Campos Extendidos**: Subsistema, aplicativo origen, normatividad, rutas de almacenamiento

---

## ⚙️ Requisitos Previos

### 1. Backup de la Base de Datos

**CRÍTICO**: Antes de ejecutar la migración, crear un respaldo completo.

```bash
# Opción 1: Usando el servicio de backup del sistema
python app/services/backup_service.py

# Opción 2: Backup manual con pg_dump
pg_dump -h localhost -U postgres -d informes_db > backup_pre_v3.sql
```

### 2. Verificar Estado Actual

```bash
# Verificar que la aplicación está en v2.0
python -c "from app.models.meta_tables import Informe; print('v2.0 OK' if not hasattr(Informe, 'tipo_flujo') else 'Ya migrado')"
```

---

## 🚀 Proceso de Migración

### Paso 1: Actualizar Dependencias

```bash
pip install -r requirements.txt
```

Las dependencias ya están actualizadas, no hay nuevos paquetes en v3.0.

### Paso 2: Ejecutar Script de Migración

```bash
python migrate_baseinformes.py
```

**¿Qué hace este script?**

1. **Crea nueva tabla**: `conjuntos_datos`
2. **Agrega 14 campos nuevos** a `cat_informes`:
   - `tipo_flujo` (VARCHAR)
   - `subsistema` (VARCHAR)
   - `conjunto_datos_id` (FK)
   - `tiene_acuerdo_confidencialidad` (BOOLEAN)
   - `cargo_autorizador` (VARCHAR)
   - `url_plantilla`, `ruta_plantilla` (VARCHAR)
   - `justificacion_normatividad` (TEXT)
   - `aplicativo_origen`, `criterios_generacion` (VARCHAR/TEXT)
   - `ruta_envio`, `ruta_respaldo` (VARCHAR)
   - `fecha_corte_descripcion`, `dias_limite_descripcion` (VARCHAR)

3. **Agrega 3 campos nuevos** a `entregas_esperadas`:
   - `es_manual` (BOOLEAN)
   - `creador_manual_id` (FK)
   - `notas_admin` (TEXT)

4. **Agrega 3 campos nuevos** a `asignaciones_informe_usuario`:
   - `tipo_responsabilidad` (VARCHAR)
   - `es_principal` (BOOLEAN)
   - `orden` (INTEGER)

5. **Migra datos existentes**: Del campo JSON `datos_generales` a las nuevas columnas relacionales

6. **Crea índices**: Para optimizar consultas

**Salida esperada:**

```
================================================================================
  MIGRACIÓN: BaseInformes v2.0 → v3.0
================================================================================

¿Desea continuar? (s/n): s

📦 CREANDO TABLA: conjuntos_datos
  ✅ Tabla conjuntos_datos

📋 ACTUALIZANDO TABLA: cat_informes
  ✅ Tipo de flujo del informe
  ✅ Subsistema del informe
  ✅ Relación con conjunto de datos
  ...

✅ MIGRACIÓN COMPLETADA

📝 PRÓXIMOS PASOS:
  1. Ejecutar: python importar_baseinformes.py
  2. Verificar datos en UI
  3. Probar funcionalidades nuevas
```

### Paso 3: Importar Datos del Excel (Opcional)

Si desea poblar el sistema con los 397 informes reales del archivo `baseInformes.xlsx`:

```bash
# Modo prueba (DRY RUN) - solo muestra qué haría
python importar_baseinformes.py --dry-run

# Importación real
python importar_baseinformes.py
```

**Advertencias**:
- Este script creará 397 informes nuevos
- Puede duplicar informes si ya existen con el mismo nombre
- Solo ejecutar en entornos de desarrollo/prueba inicialmente

**Salida esperada:**

```
================================================================================
  IMPORTACIÓN: baseInformes.xlsx → Sistema v3.0
================================================================================

📊 Archivo encontrado: /mnt/data/informes/baseInformes.xlsx (10.1 MB)
📑 Hoja de datos: Base Informes - 397 registros válidos

🔧 PROCESANDO INFORMES
  [1/397] ✅ "Informe RIPS Mensual" → Creado (ID: 150)
  [2/397] ✅ "Consolidado RUAF Semanal" → Creado (ID: 151)
  ...

✅ IMPORTACIÓN COMPLETADA
  Total procesados: 397
  Creados: 390
  Duplicados (omitidos): 7
  Errores: 0
```

### Paso 4: Verificar la Aplicación

```bash
python main.py
```

Acceder a `http://localhost:8080` y verificar:

1. **Login**: Debe funcionar normalmente
2. **Dashboard**: Sin errores de carga
3. **Gestor Informes**:
   - Abrir formulario de edición
   - Verificar que aparecen las nuevas secciones:
     - "Clasificación y Origen" (tipo flujo, subsistema, conjunto datos)
     - "Plantillas y Normatividad"
4. **Menú lateral**: Deben aparecer:
   - "Conjuntos de Datos"
   - "Entregas Manuales"

---

## 📚 Nuevas Funcionalidades

### 1. Gestión de Conjuntos de Datos

**Ruta**: `/conjuntos-datos` (Admin solamente)

**Funcionalidad**: Catálogo centralizado de conjuntos de datos para agrupar informes relacionados.

**Ejemplos**:
- RIPS (Registro Individual de Prestación de Servicios de Salud)
- RUAF (Registro Único de Afiliados)
- Vacunación (PAI - Programa Ampliado de Inmunizaciones)
- SGSSS (Sistema General de Seguridad Social en Salud)

**Uso**:
1. Acceder a "Conjuntos de Datos" en el menú
2. Clic en "Nuevo Conjunto de Datos"
3. Completar:
   - Nombre (ej: "RIPS")
   - Descripción (ej: "Registros Individuales de Prestación de Servicios")
   - Responsable (usuario del sistema)
   - Estado (Activo/Inactivo)
4. Guardar

**Relación con Informes**:
- Al crear/editar un informe, se puede seleccionar el conjunto de datos
- En la ficha del conjunto, se muestra el contador de informes asociados

### 2. Entregas Manuales (A Demanda)

**Ruta**: `/entregas-manuales` (Admin solamente)

**Funcionalidad**: Crear entregas puntuales sin esperar al scheduler automático.

**Casos de uso**:
- Informes "A Demanda" solicitados urgentemente
- Corrección de entregas omitidas
- Solicitudes excepcionales de stakeholders

**Uso**:
1. Acceder a "Entregas Manuales" en el menú
2. Completar formulario:
   - **Informe**: Seleccionar de lista desplegable
   - **Responsable**: Usuario que debe completar la tarea
   - **Fecha Programada**: Fecha objetivo de entrega
   - **Hora Límite**: Hora límite (formato 24h)
   - **Notas**: Contexto o justificación (opcional)
3. Clic en "Crear Entrega"

**Validación**:
- El sistema verifica que no exista ya una entrega PENDIENTE para el mismo informe, usuario y fecha
- Si existe, muestra advertencia

**Tabla de Entregas Recientes**:
- Muestra las últimas 50 entregas manuales
- Estados con colores:
  - 🟠 PENDIENTE
  - 🟢 CUMPLIDO
  - 🟡 EXTEMPORANEO
  - 🔴 INCUMPLIDO
- Incluye quién creó la entrega y cuándo

### 3. Tipo de Flujo de Información

**Campo nuevo en Informes**: `tipo_flujo`

**Opciones**:
- **Envío Interno**: Informe generado internamente y enviado a otra área interna
- **Envío Externo**: Informe generado internamente y enviado a entidad externa
- **Recepción Externa**: Informe recibido desde entidad externa
- **Bidireccional**: Flujo de ida y vuelta (envío + respuesta)

**Uso**:
1. En "Gestor Informes" → Crear/Editar informe
2. Sección "Clasificación y Origen"
3. Seleccionar tipo de flujo en dropdown

**Impacto**:
- Permite segmentar informes por dirección del flujo
- Facilita análisis de carga externa vs interna
- Útil para reportes de SLA diferenciados

### 4. Acuerdos de Confidencialidad

**Campos nuevos**:
- `tiene_acuerdo_confidencialidad` (checkbox)
- `cargo_autorizador` (texto)

**Uso**:
1. En formulario de informe, sección "Plantillas y Normatividad"
2. Marcar checkbox "Requiere Acuerdo de Confidencialidad"
3. Especificar "Cargo Autorizador" (ej: "Director General", "Gerente de TI")

**Casos de uso**:
- Informes con datos sensibles de pacientes
- Reportes financieros confidenciales
- Información estratégica

### 5. Plantillas y Normatividad

**Campos nuevos**:
- `url_plantilla`: URL web de la plantilla (ej: SharePoint, Drive)
- `ruta_plantilla`: Ruta filesystem de plantilla local
- `justificacion_normatividad`: Texto libre con marco legal/normativo

**Uso**:
1. Sección "Plantillas y Normatividad"
2. Ingresar URL de plantilla (si aplica)
3. Ingresar ruta de plantilla local (si aplica)
4. Describir normatividad aplicable (ej: "Resolución 3374 de 2000 - MinSalud")

**Ejemplos**:
- URL Plantilla: `https://sharepoint.empresa.co/plantillas/RIPS_mensual.xlsx`
- Ruta Plantilla: `/srv/plantillas/informe_vacunacion.docx`
- Justificación: "Resolución 256 de 2016 - Reporte obligatorio de vacunación"

---

## 🗂️ Cambios en la Base de Datos

### Nueva Tabla: `conjuntos_datos`

```sql
CREATE TABLE conjuntos_datos (
    id SERIAL PRIMARY KEY,
    nombre VARCHAR(200) NOT NULL UNIQUE,
    descripcion TEXT,
    responsable_id INTEGER REFERENCES usuarios(id),
    activo BOOLEAN DEFAULT TRUE,
    fecha_creacion TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
```

### Campos Agregados: `cat_informes`

| Campo | Tipo | Descripción |
|-------|------|-------------|
| `tipo_flujo` | VARCHAR(20) | ENVIO_INTERNO, ENVIO_EXTERNO, RECEPCION_EXTERNA, BIDIRECCIONAL |
| `subsistema` | VARCHAR(100) | Ej: "Promoción y Prevención", "Aseguramiento" |
| `conjunto_datos_id` | INTEGER (FK) | Relación con `conjuntos_datos.id` |
| `tiene_acuerdo_confidencialidad` | BOOLEAN | Indicador de requerimiento legal |
| `cargo_autorizador` | VARCHAR(200) | Cargo que autoriza uso de datos |
| `url_plantilla` | VARCHAR(500) | URL web de plantilla |
| `ruta_plantilla` | VARCHAR(500) | Ruta filesystem |
| `justificacion_normatividad` | TEXT | Marco normativo |
| `aplicativo_origen` | VARCHAR(200) | Sistema que genera el informe |
| `criterios_generacion` | TEXT | Criterios/filtros aplicados |
| `ruta_envio` | VARCHAR(500) | Ruta SFTP/FTP de envío |
| `ruta_respaldo` | VARCHAR(500) | Ruta de backup |
| `fecha_corte_descripcion` | VARCHAR(200) | Descripción de fecha de corte |
| `dias_limite_descripcion` | VARCHAR(200) | Descripción de días límite |

### Campos Agregados: `entregas_esperadas`

| Campo | Tipo | Descripción |
|-------|------|-------------|
| `es_manual` | BOOLEAN | TRUE si fue creada manualmente (no por scheduler) |
| `creador_manual_id` | INTEGER (FK) | Usuario admin que creó la entrega manual |
| `notas_admin` | TEXT | Notas del admin sobre la creación manual |

### Campos Agregados: `asignaciones_informe_usuario`

| Campo | Tipo | Descripción |
|-------|------|-------------|
| `tipo_responsabilidad` | VARCHAR(20) | EJECUTA, REVISA, APRUEBA, INFORMA |
| `es_principal` | BOOLEAN | TRUE si es el responsable principal |
| `orden` | INTEGER | Orden de ejecución (0=primero) |

---

## 🔄 Compatibilidad y Retro-compatibilidad

### ✅ Compatibilidad Total

- **Datos existentes**: Todos los informes, usuarios y entregas anteriores se mantienen intactos
- **Funcionalidad v2.0**: Todas las funciones previas siguen operativas
- **API**: Sin cambios breaking en endpoints

### ⚠️ Consideraciones

1. **Nuevos campos son opcionales**: Los informes existentes pueden funcionar sin completar los nuevos campos
2. **Default values**:
   - `tipo_flujo` = 'ENVIO_INTERNO'
   - `tiene_acuerdo_confidencialidad` = FALSE
3. **Migración de datos JSON**: El script migra automáticamente `subsistema` y `tipo_movimiento` desde `datos_generales` JSON a campos relacionales

---

## 🧪 Testing Post-Migración

### Test 1: Verificar Tablas y Columnas

```bash
python -c "
from app.models.meta_tables import Informe, ConjuntoDatos, EntregaEsperada
from sqlalchemy import inspect
from app.core.database import engine

inspector = inspect(engine)

# Verificar tabla conjuntos_datos
assert 'conjuntos_datos' in inspector.get_table_names()
print('✅ Tabla conjuntos_datos existe')

# Verificar campos en cat_informes
cols_informes = [col['name'] for col in inspector.get_columns('cat_informes')]
assert 'tipo_flujo' in cols_informes
assert 'conjunto_datos_id' in cols_informes
assert 'tiene_acuerdo_confidencialidad' in cols_informes
print('✅ Campos nuevos en cat_informes')

# Verificar campos en entregas_esperadas
cols_entregas = [col['name'] for col in inspector.get_columns('entregas_esperadas')]
assert 'es_manual' in cols_entregas
print('✅ Campos nuevos en entregas_esperadas')

print('\\n🎉 Migración verificada correctamente')
"
```

### Test 2: Crear Conjunto de Datos

1. Acceder a `/conjuntos-datos`
2. Crear conjunto "RIPS"
3. Verificar que aparece en la tabla
4. Editar y cambiar responsable
5. Guardar y verificar cambios

### Test 3: Crear Entrega Manual

1. Acceder a `/entregas-manuales`
2. Seleccionar informe activo
3. Asignar a usuario operador
4. Establecer fecha y hora
5. Agregar notas
6. Crear entrega
7. Verificar que aparece en tabla de entregas recientes
8. Ir a `/mis-pendientes` (como operador) y verificar que aparece la tarea

### Test 4: Editar Informe con Nuevos Campos

1. Ir a `/gestor-informes`
2. Editar un informe existente
3. Cambiar tipo de flujo a "Envío Externo"
4. Seleccionar conjunto de datos
5. Marcar "Requiere Acuerdo de Confidencialidad"
6. Agregar URL de plantilla
7. Guardar
8. Reabrir y verificar que los cambios se guardaron

---

## 🐛 Troubleshooting

### Error: "Column 'tipo_flujo' does not exist"

**Causa**: La migración no se ejecutó correctamente

**Solución**:
```bash
# Verificar que la migración se ejecutó
python migrate_baseinformes.py
```

### Error: "FOREIGN KEY constraint failed" al guardar informe

**Causa**: Se está intentando asignar un `conjunto_datos_id` que no existe

**Solución**:
1. Ir a `/conjuntos-datos`
2. Verificar que el conjunto existe y está activo
3. Si no existe, crearlo primero

### Error: "Duplicate entry" al importar Excel

**Causa**: Ya existen informes con el mismo nombre

**Solución**:
```bash
# Ejecutar en modo dry-run para ver duplicados
python importar_baseinformes.py --dry-run

# Renombrar informes duplicados manualmente en la UI
# O editar el script para agregar sufijo único
```

### La aplicación no inicia después de la migración

**Causa**: Posible error en modelo ORM vs esquema DB

**Solución**:
```bash
# Verificar integridad de modelos
python -c "from app.models.meta_tables import Base; from app.core.database import engine; Base.metadata.create_all(engine); print('OK')"

# Si persiste, revisar logs en consola
python main.py
```

---

## 📊 Estadísticas de Cambios

### Archivos Modificados

- `app/models/meta_tables.py`: +60 líneas (nuevos campos y relaciones)
- `app/ui/reports_manager.py`: +120 líneas (UI nuevos campos)
- `app/ui/layout.py`: +2 líneas (nuevos botones menú)
- `main.py`: +6 líneas (rutas nuevas)

### Archivos Nuevos

- `migrate_baseinformes.py`: 280 líneas (script migración)
- `importar_baseinformes.py`: 399 líneas (importador Excel)
- `app/ui/conjuntos_datos.py`: 211 líneas (módulo UI)
- `app/ui/entregas_manuales.py`: 196 líneas (módulo UI)
- `UPGRADE_V3.md`: Este archivo
- `INFORME_ANALISIS_BASEINFORMES.md`: 821 líneas (análisis técnico)
- `datos_prueba_extraidos.json`: 397 registros

### Total Líneas de Código Añadidas

**~1,900 líneas** de código Python, SQL y documentación

---

## 🔒 Recomendaciones de Seguridad

1. **Ejecutar migración en horario de baja demanda**: Preferiblemente fuera del horario laboral
2. **Mantener backup por al menos 30 días**: Por si se requiere rollback
3. **Probar en ambiente de desarrollo primero**: Antes de migrar producción
4. **Verificar permisos de archivos**: Los scripts de migración requieren permisos de escritura en DB

---

## 📞 Soporte

Para reportar problemas o solicitar ayuda:

1. Revisar logs del sistema:
   ```bash
   python main.py 2>&1 | tee migration.log
   ```

2. Verificar integridad de la base de datos:
   ```bash
   psql -d informes_db -c "SELECT COUNT(*) FROM conjuntos_datos;"
   ```

3. Crear issue en repositorio con:
   - Descripción del problema
   - Logs relevantes
   - Pasos para reproducir

---

## ✅ Checklist de Migración

Usar esta lista para verificar cada paso:

- [ ] Backup de base de datos creado
- [ ] Script `migrate_baseinformes.py` ejecutado sin errores
- [ ] Verificación de tablas y columnas exitosa
- [ ] Aplicación inicia correctamente
- [ ] Login funciona
- [ ] Dashboard se carga sin errores
- [ ] Menú muestra "Conjuntos de Datos" y "Entregas Manuales"
- [ ] Gestor Informes muestra nuevas secciones
- [ ] Test de creación de Conjunto de Datos exitoso
- [ ] Test de creación de Entrega Manual exitoso
- [ ] Test de edición de Informe con nuevos campos exitoso
- [ ] (Opcional) Importación de Excel ejecutada
- [ ] Documentación actualizada
- [ ] Usuarios notificados de nuevas funcionalidades

---

**Versión del documento**: 1.0
**Última actualización**: 8 de Enero, 2026
**Compatibilidad**: Sistema Gestión Informes v2.0 → v3.0
