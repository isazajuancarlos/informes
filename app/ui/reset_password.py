from nicegui import ui
from app.services.auth import AuthService

def page_reset_password(token: str = None):
    # Nota: NiceGUI inyecta el parámetro 'token' de la URL automáticamente
    
    nuevo_pass = {'p1': '', 'p2': ''}

    def cambiar():
        if not token:
            ui.notify('Token no proporcionado', type='negative'); return
        if not nuevo_pass['p1'] or not nuevo_pass['p2']:
            ui.notify('Complete los campos', type='warning'); return
        if nuevo_pass['p1'] != nuevo_pass['p2']:
            ui.notify('Las contraseñas no coinciden', type='negative'); return
        
        ok, msg = AuthService.consumir_token_y_cambiar_pass(token, nuevo_pass['p1'])
        
        if ok:
            ui.notify(msg, type='positive', close_button=True)
            ui.navigate.to('/login')
        else:
            ui.notify(msg, type='negative', close_button=True)

    with ui.column().classes('w-full h-screen items-center justify-center bg-gray-100'):
        with ui.card().classes('w-96 p-8 shadow-lg'):
            ui.label('Restablecer Contraseña').classes('text-2xl font-bold text-center w-full mb-6')
            
            if not token:
                ui.label('Enlace inválido o incompleto.').classes('text-red font-bold text-center w-full')
                ui.button('Ir al Login', on_click=lambda: ui.navigate.to('/login')).classes('w-full mt-4')
            else:
                ui.input('Nueva Contraseña', password=True).bind_value(nuevo_pass, 'p1').classes('w-full')
                ui.input('Confirmar Contraseña', password=True).bind_value(nuevo_pass, 'p2').classes('w-full')
                
                ui.button('Cambiar Contraseña', on_click=cambiar).classes('w-full mt-6 bg-green-600 text-white')