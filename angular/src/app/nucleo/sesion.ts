/**
 * Quién está dentro, según el SERVIDOR.
 *
 * Este servicio guarda al actor para pintar la interfaz —el nombre en la barra,
 * qué entradas del menú se muestran—, y eso es todo lo que hace. **No decide
 * permisos.** La comprobación vive en el servidor, en un solo sitio
 * (`autz::Alcance`), y esta capa no puede ampliarla: si alguien edita en memoria
 * su rol a «Admin», el menú le enseñará opciones cuyas peticiones el servidor
 * seguirá rechazando.
 *
 * Decirlo explícitamente importa porque la tentación contraria —guardar el rol
 * en el cliente y confiar en él— es justo la que convierte un cambio de
 * arquitectura en un problema de seguridad.
 */
import { Injectable, computed, inject, signal } from '@angular/core';
import { Router } from '@angular/router';
import { CanActivateFn } from '@angular/router';
import { firstValueFrom } from 'rxjs';

import { Actor, Api } from './api';

@Injectable({ providedIn: 'root' })
export class Sesion {
  private api = inject(Api);
  private router = inject(Router);

  private readonly _actor = signal<Actor | null>(null);
  private readonly _comprobada = signal(false);

  readonly actor = this._actor.asReadonly();
  readonly comprobada = this._comprobada.asReadonly();
  readonly dentro = computed(() => this._actor() !== null);
  readonly iniciales = computed(() => {
    const n = this._actor()?.nombre ?? '';
    return n
      .split(/\s+/)
      .filter(Boolean)
      .slice(0, 2)
      .map((p) => p[0]?.toUpperCase() ?? '')
      .join('');
  });

  /** Pregunta al servidor si la cookie que el navegador tenga sigue valiendo. */
  async comprobar(): Promise<boolean> {
    try {
      this._actor.set(await firstValueFrom(this.api.yo()));
    } catch {
      this._actor.set(null);
    } finally {
      this._comprobada.set(true);
    }
    return this.dentro();
  }

  async entrar(email: string, clave: string) {
    await firstValueFrom(this.api.entrar(email, clave));
    await this.comprobar();
  }

  async salir() {
    try {
      await firstValueFrom(this.api.salir());
    } finally {
      // Aunque el servidor falle al registrar la salida, aquí se sale: dejar la
      // sesión abierta en pantalla porque falló el registro sería la respuesta
      // exactamente contraria a la que quiere quien pulsa «salir».
      this._actor.set(null);
      this.router.navigate(['/acceso']);
    }
  }
}

/** Guarda de ruta. Es comodidad de navegación, no una barrera. */
export const guardaSesion: CanActivateFn = async () => {
  const sesion = inject(Sesion);
  const router = inject(Router);
  const dentro = sesion.comprobada() ? sesion.dentro() : await sesion.comprobar();
  return dentro ? true : router.createUrlTree(['/acceso']);
};
