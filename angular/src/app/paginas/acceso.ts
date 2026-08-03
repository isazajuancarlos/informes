/**
 * Entrada al sistema.
 *
 * El acceso anterior dejaba media pantalla en negro desde el primer commit —el
 * panel de marca no se dibujaba nunca— y ninguna de las 600 pruebas podía verlo,
 * porque el simulador construye los elementos pero no calcula CSS. Aquí el panel
 * es una rejilla de dos columnas que colapsa a una en pantallas estrechas, y no
 * depende de ninguna hoja que pueda no cargar.
 */
import { Component, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Router } from '@angular/router';
import { MatButtonModule } from '@angular/material/button';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';
import { MatProgressBarModule } from '@angular/material/progress-bar';

import { firstValueFrom } from 'rxjs';

import { Api, CuentaDemo } from '../nucleo/api';
import { Sesion } from '../nucleo/sesion';

@Component({
  selector: 'siger-acceso',
  imports: [
    FormsModule,
    MatButtonModule,
    MatFormFieldModule,
    MatIconModule,
    MatInputModule,
    MatProgressBarModule,
  ],
  template: `
    <div class="acceso">
      <section class="marca">
        <h1>SIGER</h1>
        <p class="largo">Sistema Integral de Gestión de Informes</p>
        <ul>
          <li><mat-icon>event_available</mat-icon> Cada obligación, con su fecha nominal y su plazo</li>
          <li><mat-icon>verified_user</mat-icon> Entregas firmadas y auditables</li>
          <li><mat-icon>cloud_off</mat-icon> Funciona sin conexión</li>
        </ul>
      </section>

      <section class="formulario">
        <!--
          Dos pasos, dos formularios, y el primero DESAPARECE al pasar al
          segundo. Dejar el correo y la contraseña en pantalla mientras se pide el
          código invita a volver a enviarlos, y lo que el servidor espera en ese
          momento es otra cosa: la cookie pendiente ya dice quién es.
        -->
        @if (!pideCodigo()) {
          <form (ngSubmit)="entrar()">
            <h2>Entrar</h2>

            <mat-form-field appearance="outline">
              <mat-label>Correo</mat-label>
              <input matInput type="email" name="email" autocomplete="username"
                     [(ngModel)]="email" required autofocus />
              <mat-icon matSuffix>mail</mat-icon>
            </mat-form-field>

            <mat-form-field appearance="outline">
              <mat-label>Contraseña</mat-label>
              <input matInput [type]="verClave() ? 'text' : 'password'" name="clave"
                     autocomplete="current-password" [(ngModel)]="clave" required />
              <button mat-icon-button matSuffix type="button"
                      [attr.aria-label]="verClave() ? 'Ocultar la contraseña' : 'Ver la contraseña'"
                      (click)="verClave.set(!verClave())">
                <mat-icon>{{ verClave() ? 'visibility_off' : 'visibility' }}</mat-icon>
              </button>
            </mat-form-field>

            @if (error()) {
              <p class="error" role="alert">
                <mat-icon>error_outline</mat-icon> {{ error() }}
              </p>
            }

            <button mat-flat-button type="submit" [disabled]="ocupado()">
              {{ ocupado() ? 'Comprobando…' : 'Entrar' }}
            </button>
            @if (ocupado()) { <mat-progress-bar mode="indeterminate" /> }

            <!--
              El cartel de la demostración. Solo aparece si el servidor anuncia
              cuentas, y solo las anuncia si el despliegue las escribió a mano en
              SIGER_DEMO_CUENTAS: en una instalación de cliente esta lista llega
              vacía y aquí no se dibuja nada.
            -->
            @if (demo().length) {
              <aside class="demo">
                <h3><mat-icon>waving_hand</mat-icon> Esto es una demostración</h3>
                <p>
                  Los datos son ficticios y se regeneran cada noche. Entre con
                  cualquiera de estas cuentas —o pulse una y entramos nosotros:
                </p>
                <ul>
                  @for (c of demo(); track c.correo) {
                    <li>
                      <button mat-stroked-button type="button" [disabled]="ocupado()"
                              (click)="entrarComo(c)">
                        Entrar como {{ c.nombre }}
                      </button>
                      <code>{{ c.correo }} · {{ c.clave }}</code>
                    </li>
                  }
                </ul>
              </aside>
            }
          </form>
        } @else {
          <form (ngSubmit)="verificar()">
            <h2>Código de verificación</h2>
            <p class="guia">
              Escriba el código de seis dígitos de su aplicación de autenticación.
              Si no tiene el teléfono a mano, sirve uno de sus códigos de
              recuperación.
            </p>

            <mat-form-field appearance="outline">
              <mat-label>Código</mat-label>
              <!--
                autocomplete="one-time-code" es lo que hace que el teléfono
                ofrezca el código de un vistazo, e inputmode saca el teclado
                numérico. No se pone type="number": un código de recuperación
                lleva letras y un guion.
              -->
              <input matInput name="codigo" autocomplete="one-time-code"
                     inputmode="numeric" [(ngModel)]="codigo" required autofocus />
              <mat-icon matSuffix>pin</mat-icon>
            </mat-form-field>

            @if (error()) {
              <p class="error" role="alert">
                <mat-icon>error_outline</mat-icon> {{ error() }}
              </p>
            }

            <button mat-flat-button type="submit" [disabled]="ocupado()">
              {{ ocupado() ? 'Comprobando…' : 'Verificar' }}
            </button>
            @if (ocupado()) { <mat-progress-bar mode="indeterminate" /> }
            <button mat-button type="button" (click)="volverAlPrincipio()">Cancelar</button>
          </form>
        }
      </section>
    </div>
  `,
  styles: `
    .acceso {
      min-height: 100dvh;
      display: grid;
      grid-template-columns: 1fr 1fr;
    }
    /* Una sola columna en pantallas estrechas: el panel de marca se oculta
       ENTERO en vez de encogerse a una franja ilegible. */
    @media (max-width: 900px) {
      .acceso { grid-template-columns: 1fr; }
      .marca { display: none; }
    }
    .marca {
      display: flex; flex-direction: column; justify-content: center; gap: 1rem;
      padding: 3rem;
      color: var(--mat-sys-on-primary-container);
      background: var(--mat-sys-primary-container);
    }
    .marca h1 { font-size: 3.5rem; margin: 0; letter-spacing: .08em; }
    .marca .largo { margin: 0; opacity: .85; font-size: 1.1rem; }
    .marca ul { list-style: none; padding: 0; margin: 2rem 0 0; display: grid; gap: .9rem; }
    .marca li { display: flex; align-items: center; gap: .6rem; }
    .formulario { display: grid; place-items: center; padding: 2rem; }
    form { display: flex; flex-direction: column; gap: .5rem; width: min(24rem, 100%); }
    h2 { margin: 0 0 1rem; }
    .guia { margin: 0 0 1rem; color: var(--mat-sys-on-surface-variant); font-size: .9rem; }
    .error {
      display: flex; align-items: center; gap: .4rem; margin: .2rem 0 .6rem;
      color: var(--mat-sys-error);
    }
    button[type='submit'] { height: 3rem; }
    .demo {
      margin-top: 1.5rem; padding: 1rem 1.25rem; border-radius: 12px;
      background: var(--mat-sys-surface-container-high);
      border: 1px solid var(--mat-sys-outline-variant);
    }
    .demo h3 {
      display: flex; align-items: center; gap: 0.5rem;
      margin: 0 0 0.5rem; font-size: 1rem;
    }
    .demo p { margin: 0 0 0.75rem; font-size: 0.875rem; opacity: 0.85; }
    .demo ul { list-style: none; margin: 0; padding: 0; display: grid; gap: 0.5rem; }
    .demo li { display: flex; align-items: center; gap: 0.75rem; flex-wrap: wrap; }
    /* El correo y la contraseña se enseñan además del botón: quien quiera
       teclearlos —para ver que no hay truco, o para probar desde el teléfono—
       tiene que poder leerlos. */
    .demo code { font-size: 0.8125rem; opacity: 0.75; }
  `,
})
export class Acceso {
  private sesion = inject(Sesion);
  private router = inject(Router);
  private api = inject(Api);

  email = '';
  clave = '';
  codigo = '';
  verClave = signal(false);
  ocupado = signal(false);
  error = signal('');

  /** El segundo paso está pedido: hay cookie pendiente y no hay sesión. */
  readonly pideCodigo = signal(false);

  /**
   * Las cuentas que este servidor anuncia. Vacío en toda instalación de cliente.
   *
   * Se pregunta SIEMPRE y sin sesión, porque la respuesta de una instalación
   * normal es una lista vacía y no un error: distinguir «no hay demostración» de
   * «esta versión no tiene ese extremo» obligaría a dibujar dos casos que se ven
   * igual.
   */
  readonly demo = signal<CuentaDemo[]>([]);

  constructor() {
    // Si falla, se queda vacío y la pantalla es la de siempre: el cartel es un
    // extra de la demostración, no puede impedir entrar a nadie (#71).
    firstValueFrom(this.api.cuentasDeDemostracion())
      .then((c) => this.demo.set(c))
      .catch(() => this.demo.set([]));
  }

  /**
   * Entrar con una de las cuentas anunciadas, de un clic.
   *
   * Rellena el formulario ADEMÁS de enviarlo: si el acceso falla —la base
   * resembrada a medias, por ejemplo—, el visitante se queda con los campos
   * puestos y el mensaje de error, en vez de con una pantalla vacía que no dice
   * qué se intentó.
   */
  async entrarComo(c: CuentaDemo) {
    this.email = c.correo;
    this.clave = c.clave;
    await this.entrar();
  }

  async entrar() {
    if (this.ocupado()) return;
    this.error.set('');
    this.ocupado.set(true);
    try {
      if (await this.sesion.entrar(this.email, this.clave)) {
        // La contraseña era correcta y no abre nada. La contraseña se borra de
        // memoria aquí: a partir de este punto no hace falta para nada, y lo que
        // no se guarda no se filtra.
        this.clave = '';
        this.pideCodigo.set(true);
        return;
      }
      await this.router.navigate(['/entregas']);
    } catch (e) {
      // El servidor responde lo mismo para «no existe» y «clave incorrecta»: la
      // diferencia le diría a quien prueba qué correos son reales.
      this.error.set((e as Error).message);
    } finally {
      this.ocupado.set(false);
    }
  }

  async verificar() {
    if (this.ocupado()) return;
    this.error.set('');
    this.ocupado.set(true);
    try {
      await this.sesion.completarSegundoFactor(this.codigo.trim());
      await this.router.navigate(['/entregas']);
    } catch (e) {
      // Un código gastado, uno equivocado y un token pendiente caducado dan el
      // mismo mensaje. El único que se distingue es el 503 de «no puedo
      // comprobarlo», que es una avería del servidor y no culpa de nadie.
      this.error.set((e as Error).message);
      this.codigo = '';
    } finally {
      this.ocupado.set(false);
    }
  }

  /**
   * Volver a empezar. No hay nada que deshacer en el servidor: la cookie
   * pendiente caduca sola en cinco minutos y no sirve para nada más.
   */
  volverAlPrincipio() {
    this.codigo = '';
    this.error.set('');
    this.pideCodigo.set(false);
  }
}
