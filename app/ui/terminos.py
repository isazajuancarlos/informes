"""
Página de Política de Privacidad y Tratamiento de Datos Personales.

Cumple con:
- Ley 1581/2012 (Habeas Data Colombia): Protección de datos personales
- Decreto 1377/2013: Reglamenta parcialmente la Ley 1581/2012
- Circular Externa 002/2015 SIC: Instrucciones para la implementación
- Artículo 10 Ley 1581/2012: Datos semiprivados y su tratamiento
- GDPR Art. 13/14: Información en caso de recopilación (referencia internacional)
"""

from nicegui import ui, app
from datetime import date
from app.core.config import NOMBRE_PRODUCTO, NOMBRE_LARGO


def page_terminos():
    """
    Política de privacidad y tratamiento de datos personales.
    Acceso público — no requiere autenticación.
    """
    esta_autenticado = app.storage.user.get('authenticated', False)

    with ui.column().classes('w-full max-w-3xl mx-auto p-6 gap-4'):

        # Botón de regreso
        with ui.row().classes('items-center gap-2'):
            if esta_autenticado:
                ui.button('← Volver', on_click=lambda: ui.navigate.to('/')).props('flat size=sm')
            else:
                ui.button('← Ir al Login', on_click=lambda: ui.navigate.to('/login')).props('flat size=sm')

        # Encabezado
        ui.label('Política de Privacidad y Tratamiento de Datos Personales') \
            .classes('text-2xl font-bold text-slate-800')
        ui.label(f'Última actualización: {date.today().strftime("%d de %B de %Y")}') \
            .classes('text-sm text-slate-500')

        ui.separator()

        # ─── 1. RESPONSABLE DEL TRATAMIENTO ───────────────────────────────
        with ui.card().classes('w-full p-4'):
            ui.label('1. Responsable del Tratamiento de Datos').classes('text-lg font-bold text-slate-700 mb-2')
            ui.markdown(f"""
El **{NOMBRE_LARGO}** ({NOMBRE_PRODUCTO}) es operado por la organización que lo ha implementado
("el Responsable"). El Responsable es quien determina los fines y medios del tratamiento de sus
datos personales, de conformidad con el **Artículo 3(e) de la Ley 1581/2012**.

Para ejercer sus derechos o presentar consultas, puede contactar al área de Sistemas o al
Oficial de Protección de Datos designado por su organización.
""")

        # ─── 2. DATOS RECOPILADOS ─────────────────────────────────────────
        with ui.card().classes('w-full p-4'):
            ui.label('2. Datos Personales que Recopilamos').classes('text-lg font-bold text-slate-700 mb-2')
            ui.markdown("""
Recopilamos y tratamos los siguientes datos para operar el sistema:

| Categoría | Datos | Base Legal |
|-----------|-------|------------|
| **Identificación** | Nombre completo, dirección de correo electrónico | Ejecución de contrato / relación laboral |
| **Acceso** | Contraseña (almacenada con hash Argon2id, no legible ni reversible) | Seguridad del sistema |
| **Actividad** | Fecha/hora de inicio y cierre de sesión, dirección IP | Seguridad y auditoría (Ley 1581/2012) |
| **Operacional** | Registros de tareas asignadas, entregas y comentarios | Ejecución de obligaciones laborales/contractuales |

> **Nota:** No recopilamos datos sensibles (salud, origen étnico, opiniones políticas, datos
> biométricos) en el sentido del **Artículo 6 de la Ley 1581/2012**.
""")

        # ─── 3. FINALIDADES ──────────────────────────────────────────────
        with ui.card().classes('w-full p-4'):
            ui.label('3. Finalidades del Tratamiento').classes('text-lg font-bold text-slate-700 mb-2')
            ui.markdown("""
Sus datos son tratados exclusivamente para las siguientes finalidades
(**Artículo 4(e) Ley 1581/2012 — Principio de Finalidad**):

1. **Autenticación y control de acceso**: verificar su identidad para ingresar al sistema.
2. **Gestión de tareas**: asignarle informes y registrar el estado de sus entregas.
3. **Notificaciones**: enviarle alertas sobre vencimientos o incumplimientos por correo electrónico.
4. **Auditoría de seguridad**: registrar eventos de acceso para detectar uso no autorizado
   (Ley 1581/2012 y recomendación ISO 27001 A.12.4).
5. **Cumplimiento normativo**: mantener trazabilidad exigida por regulaciones aplicables.

Los datos **no serán vendidos, arrendados ni cedidos** a terceros sin su autorización,
salvo por obligación legal o autoridad competente (**Artículo 10 Ley 1581/2012**).
""")

        # ─── 4. DERECHOS ARCO ────────────────────────────────────────────
        with ui.card().classes('w-full p-4 bg-blue-50 border border-blue-200'):
            ui.label('4. Sus Derechos (ARCO) — Artículos 8 y 15 Ley 1581/2012') \
                .classes('text-lg font-bold text-blue-800 mb-2')
            ui.markdown("""
Como titular de datos personales, tiene los siguientes derechos reconocidos por la Ley 1581/2012:

- **Acceso**: conocer los datos personales que tenemos sobre usted y el tratamiento que les damos.
- **Rectificación**: solicitar la corrección de datos inexactos o incompletos.
- **Cancelación (supresión)**: solicitar la eliminación de sus datos cuando no sean necesarios
  para los fines declarados, salvo que exista obligación legal de conservarlos.
- **Oposición**: oponerse al tratamiento de sus datos para fines específicos.
- **Queja ante la SIC**: presentar queja ante la Superintendencia de Industria y Comercio
  si considera que sus derechos han sido vulnerados (www.sic.gov.co).

**Para ejercer sus derechos**, contacte al administrador del sistema. Responderemos dentro
de los plazos establecidos en el **Artículo 14 de la Ley 1581/2012** (10 días hábiles para
consultas, 15 días hábiles para reclamos).
""")

        # ─── 5. RETENCIÓN ────────────────────────────────────────────────
        with ui.card().classes('w-full p-4'):
            ui.label('5. Tiempo de Retención de Datos').classes('text-lg font-bold text-slate-700 mb-2')
            ui.markdown("""
| Tipo de dato | Periodo de retención |
|-------------|---------------------|
| Datos de cuenta (nombre, email) | Durante la vigencia de la relación laboral/contractual + 5 años |
| Registros de auditoría de seguridad | Mínimo 1 año; hasta 5 años según política interna |
| Contraseñas (hash) | Se eliminan al desactivar la cuenta |
| Historial de entregas | Durante la vigencia del sistema + 5 años (requisito documental) |

Una vez vencido el plazo, los datos serán suprimidos o anonimizados de forma irreversible
(**Artículo 4(d) Ley 1581/2012 — Principio de Temporalidad**).
""")

        # ─── 6. SEGURIDAD ────────────────────────────────────────────────
        with ui.card().classes('w-full p-4'):
            ui.label('6. Medidas de Seguridad').classes('text-lg font-bold text-slate-700 mb-2')
            ui.markdown("""
Implementamos medidas técnicas y organizativas para proteger sus datos
(**Artículo 4(f) Ley 1581/2012 — Principio de Seguridad**):

- Contraseñas almacenadas con **Argon2id** (19 MiB, 2 iteraciones), el algoritmo
  recomendado por OWASP; nunca en texto claro y sin posibilidad de revertirlas.
  Las cuentas creadas antes de esta medida se actualizan solas al iniciar sesión.
- Sesiones con **tiempo de expiración** configurable (por defecto 60 minutos).
- **Tokens de restablecimiento** de contraseña con validez de 1 hora y uso único.
- Registro de intentos de acceso fallidos en base de datos.
- Acceso por **roles** (ADMIN, OPERADOR, SUPERVISOR, TERCERO) con segregación de funciones.
- Copias de respaldo automáticas de la base de datos.
""")

        # ─── 7. TRANSFERENCIAS ───────────────────────────────────────────
        with ui.card().classes('w-full p-4'):
            ui.label('7. Transferencias Internacionales').classes('text-lg font-bold text-slate-700 mb-2')
            ui.markdown("""
El sistema opera en servidores configurados por su organización. Si el servicio se aloja en
proveedores de nube fuera de Colombia, aplican las disposiciones del **Artículo 26 de la
Ley 1581/2012** sobre transferencias internacionales. En tal caso, el Responsable deberá
garantizar que el país receptor cuente con un nivel adecuado de protección, o suscribir
las cláusulas contractuales tipo establecidas por la SIC.
""")

        # ─── 8. CAMBIOS A LA POLÍTICA ────────────────────────────────────
        with ui.card().classes('w-full p-4'):
            ui.label('8. Modificaciones a esta Política').classes('text-lg font-bold text-slate-700 mb-2')
            ui.markdown("""
Podemos actualizar esta política periódicamente. Los cambios materiales serán notificados
mediante aviso en el sistema o por correo electrónico con al menos **15 días de anticipación**.
El uso continuado del sistema después de la notificación constituye aceptación de los cambios.
""")

        # ─── PIE ─────────────────────────────────────────────────────────
        ui.separator()
        with ui.row().classes('w-full justify-between items-center text-xs text-slate-400'):
            ui.label(f'{NOMBRE_LARGO} — Política de Privacidad')
            ui.label('Ley 1581/2012 · Decreto 1377/2013 · SIC Colombia')
