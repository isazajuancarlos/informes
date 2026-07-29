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
        </form>
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
    .error {
      display: flex; align-items: center; gap: .4rem; margin: .2rem 0 .6rem;
      color: var(--mat-sys-error);
    }
    button[type='submit'] { height: 3rem; }
  `,
})
export class Acceso {
  private sesion = inject(Sesion);
  private router = inject(Router);

  email = '';
  clave = '';
  verClave = signal(false);
  ocupado = signal(false);
  error = signal('');

  async entrar() {
    if (this.ocupado()) return;
    this.error.set('');
    this.ocupado.set(true);
    try {
      await this.sesion.entrar(this.email, this.clave);
      await this.router.navigate(['/entregas']);
    } catch (e) {
      // El servidor responde lo mismo para «no existe» y «clave incorrecta»: la
      // diferencia le diría a quien prueba qué correos son reales.
      this.error.set((e as Error).message);
    } finally {
      this.ocupado.set(false);
    }
  }
}
