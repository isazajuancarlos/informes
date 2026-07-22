from nicegui import ui, app
from app.services.auth import AuthService
from app.services.email_service import EmailService
from app.core.database import get_db_session
from app.services.session_manager import SessionManager, SecurityAudit


def page_login():
    recuperar = {'email': ''}

    async def solicitar_recuperacion():
        email_rec = recuperar['email'].strip()
        if not email_rec:
            ui.notify('Ingrese su correo electrónico', type='warning')
            return
        token = AuthService.generar_token_recuperacion(email_rec)
        if token:
            await EmailService.enviar_recuperacion(email_rec, token)
            ui.notify('Enlace de recuperación enviado. Revise su correo.', type='positive')
            dialogo_recuperar.close()
        else:
            ui.notify('No se encontró una cuenta con ese correo', type='negative')

    def try_login():
        email = email_input.value.strip()
        password = password_input.value
        if not email or not password:
            ui.notify('Ingrese correo y contraseña', type='warning')
            return

        user = AuthService.authenticate_user(email, password)
        if not user:
            SecurityAudit.log_login_attempt(email, success=False)
            ui.notify('Correo o contraseña incorrectos', type='negative')
            return

        if not user.activo:
            ui.notify('Usuario inactivo. Contacte al administrador.', type='negative')
            return

        # Determine role
        rol = user.rol_sistema or 'OPERADOR'
        if user.es_admin:
            rol = 'ADMIN'

        with get_db_session() as db:
            from app.models.meta_tables import Stakeholder
            if db.query(Stakeholder).filter_by(usuario_id=user.id).first():
                rol = 'TERCERO'

        app.storage.user.update({
            'user_id': user.id,
            'nombre': user.nombre,
            'email': user.email,
            'es_admin': user.es_admin,
            'rol': rol,
            'authenticated': True,
        })

        SessionManager.init_session()
        SecurityAudit.log_login_attempt(user.email, success=True)
        ui.notify(f'Bienvenido, {user.nombre}', type='positive')

        if rol == 'ADMIN':
            ui.navigate.to('/dashboard')
        elif rol == 'SUPERVISOR':
            ui.navigate.to('/analytics')
        elif rol == 'TERCERO':
            ui.navigate.to('/portal-terceros')
        else:
            ui.navigate.to('/mis-pendientes')

    # ── UI ──────────────────────────────────────────────────────────────────
    ui.add_head_html('''
        <style>
            body { background: #0f172a; margin: 0; }
            .login-panel {
                background: linear-gradient(135deg, #1e293b 0%, #0f172a 100%);
            }
            .login-card {
                backdrop-filter: blur(12px);
                border: 1px solid rgba(255,255,255,0.08);
            }

            /* El panel de marca se mostraba con las clases `hidden lg:flex`, y
               NO SE MOSTRABA NUNCA: media pantalla en negro y el nombre del
               sistema invisible.

               NiceGUI compila Tailwind EN EL NAVEGADOR, y ahí el orden en que
               emite `.hidden` y `.lg\\:flex` no está garantizado; si `.hidden`
               queda después, gana siempre. Medido con navegador real a 1440 px
               de ancho —muy por encima del corte de 1024—: `display: none`.

               Se controla aquí, que es donde el orden sí lo decidimos nosotros.
               El `!important` no es pereza: compite contra utilidades generadas
               en tiempo de ejecución cuya posición en la hoja no controlamos. */
            .login-panel { display: none !important; }
            @media (min-width: 1024px) {
                .login-panel { display: flex !important; }
            }
        </style>
    ''')

    # `gap-0` y quitar el relleno de la página no son cosmética: la fila de
    # NiceGUI trae 16 px de separación y la página 16 de relleno, así que dos
    # columnas al 50 % suman 1424 px donde caben 1408 — y la segunda BAJA. Con
    # el panel de marca oculto solo quedaba una columna, no había desbordamiento,
    # y el fallo se disfrazaba de «media pantalla vacía a la derecha».
    ui.query('.nicegui-content').classes('p-0 gap-0')

    with ui.row().classes('w-full min-h-screen m-0 p-0 gap-0 no-wrap'):

        # ── Panel izquierdo (branding) ────────────────────────────────────
        # Sin `hidden lg:flex`: lo decide el CSS de arriba. Ver el comentario.
        with ui.column().classes(
            'w-1/2 min-h-screen login-panel '
            'items-center justify-center p-12 gap-8'
        ):
            with ui.column().classes('items-center gap-6 max-w-sm'):
                with ui.row().classes('items-center gap-3'):
                    ui.icon('analytics').classes('text-6xl text-blue-400')
                    ui.label('SIGER').classes(
                        'text-4xl font-black text-white tracking-wider'
                    )

                ui.label('Sistema Integral de\nGestión de Informes').classes(
                    'text-2xl font-light text-slate-300 text-center whitespace-pre-line'
                )

                ui.separator().classes('w-24 opacity-20 my-4')

                with ui.column().classes('gap-4 w-full'):
                    for icon, texto in [
                        ('schedule', 'Entregas programadas y a demanda'),
                        ('psychology', 'Motor de reglas de negocio'),
                        ('group', 'Gestión de roles y delegaciones'),
                        ('bar_chart', 'Analytics e indicadores SLA'),
                    ]:
                        with ui.row().classes('items-center gap-3'):
                            ui.icon(icon).classes('text-blue-400 text-xl')
                            ui.label(texto).classes('text-slate-400 text-sm')

        # ── Panel derecho (formulario) ────────────────────────────────────
        with ui.column().classes(
            'w-full lg:w-1/2 min-h-screen bg-slate-950 '
            'items-center justify-center p-6'
        ):
            with ui.card().classes(
                'login-card w-full max-w-md p-10 rounded-2xl shadow-2xl bg-slate-900'
            ):
                # Header
                with ui.column().classes('items-center gap-2 mb-8'):
                    with ui.row().classes('items-center gap-2 lg:hidden'):
                        ui.icon('analytics').classes('text-3xl text-blue-400')
                        ui.label('SIGER').classes('text-2xl font-black text-white')
                    ui.label('Iniciar sesión').classes(
                        'text-2xl font-bold text-white mt-2'
                    )
                    ui.label('Ingrese sus credenciales para continuar').classes(
                        'text-slate-400 text-sm text-center'
                    )

                # Inputs
                with ui.column().classes('w-full gap-4'):
                    with ui.column().classes('w-full gap-1'):
                        ui.label('Correo electrónico').classes(
                            'text-slate-300 text-sm font-medium'
                        )
                        email_input = (
                            ui.input(placeholder='usuario@ejemplo.com')
                            .props('outlined dense color=blue-4 input-class="text-white"')
                            .classes('w-full')
                            .on('keydown.enter', try_login)
                        )
                        email_input.props(
                            'bg-color=slate-800 label-color=slate-400'
                        )

                    with ui.column().classes('w-full gap-1'):
                        ui.label('Contraseña').classes(
                            'text-slate-300 text-sm font-medium'
                        )
                        password_input = (
                            ui.input(placeholder='••••••••', password=True,
                                     password_toggle_button=True)
                            .props('outlined dense color=blue-4')
                            .classes('w-full')
                            .on('keydown.enter', try_login)
                        )

                # Submit
                ui.button('Ingresar', icon='login', on_click=try_login).classes(
                    'w-full mt-2 bg-blue-600 hover:bg-blue-700 text-white '
                    'font-semibold rounded-lg h-11'
                ).props('no-caps')

                # Forgot password
                with ui.row().classes('w-full justify-center mt-4'):
                    ui.button(
                        '¿Olvidó su contraseña?',
                        on_click=lambda: dialogo_recuperar.open()
                    ).props('flat dense no-caps').classes('text-blue-400 text-sm')

                # Footer
                ui.separator().classes('my-6 opacity-10')
                with ui.row().classes('w-full items-center justify-center gap-2'):
                    ui.icon('lock', size='xs').classes('text-slate-600')
                    ui.label('Acceso restringido a personal autorizado').classes(
                        'text-slate-600 text-xs'
                    )

    # ── Diálogo recuperación ─────────────────────────────────────────────
    with ui.dialog() as dialogo_recuperar, ui.card().classes(
        'w-96 p-8 bg-slate-900 rounded-2xl'
    ):
        ui.label('Recuperar contraseña').classes('text-xl font-bold text-white mb-1')
        ui.label(
            'Le enviaremos un enlace a su correo para restablecer la contraseña.'
        ).classes('text-slate-400 text-sm mb-6')

        ui.input(
            'Correo electrónico', placeholder='usuario@ejemplo.com'
        ).bind_value(recuperar, 'email').props('outlined dense color=blue-4').classes('w-full')

        with ui.row().classes('w-full justify-end gap-2 mt-6'):
            ui.button('Cancelar', on_click=dialogo_recuperar.close).props(
                'flat no-caps'
            ).classes('text-slate-400')
            ui.button('Enviar enlace', icon='send',
                      on_click=solicitar_recuperacion).props('no-caps').classes(
                'bg-blue-600 text-white'
            )
