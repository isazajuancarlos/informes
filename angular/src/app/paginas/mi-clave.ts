/**
 * Cambiar la propia contraseña. Cualquier rol.
 *
 * Es la pantalla que cierra el caso de uso de la revocación selectiva (#194):
 * «acabo de cambiar la contraseña, echa a quien entrara con la vieja». Hasta que
 * existió, `token_version` solo se podía subir desde «cerrar sesión en todos los
 * dispositivos», y cambiar la contraseña no era posible en absoluto.
 *
 * El servidor reemite la cookie, así que al terminar se sigue dentro AQUÍ y caen
 * todas las demás sesiones de esta persona. Se dice en pantalla, porque si no el
 * efecto se descubre en el otro dispositivo y parece un fallo.
 */
import { Component, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Router } from '@angular/router';
import { MatButtonModule } from '@angular/material/button';
import { MatCardModule } from '@angular/material/card';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';
import { MatProgressBarModule } from '@angular/material/progress-bar';
import { firstValueFrom } from 'rxjs';

import { Api } from '../nucleo/api';

@Component({
  selector: 'siger-mi-clave',
  imports: [
    FormsModule,
    MatButtonModule,
    MatCardModule,
    MatFormFieldModule,
    MatIconModule,
    MatInputModule,
    MatProgressBarModule,
  ],
  template: `
    <mat-card class="tarjeta">
      <h2>Cambiar mi contraseña</h2>
      <p class="guia">
        Al cambiarla se cerrarán las sesiones que tenga abiertas en
        <strong>otros dispositivos</strong>. En este seguirá dentro.
      </p>

      @if (cargando()) { <mat-progress-bar mode="indeterminate" /> }
      @if (error()) {
        <p class="error" role="alert"><mat-icon>error_outline</mat-icon> {{ error() }}</p>
      }
      @if (hecho()) {
        <p class="bien" role="status"><mat-icon>check_circle</mat-icon> {{ hecho() }}</p>
      }

      <mat-form-field>
        <mat-label>Contraseña actual</mat-label>
        <input matInput type="password" [(ngModel)]="actual" autocomplete="current-password" />
      </mat-form-field>

      <mat-form-field>
        <mat-label>Contraseña nueva</mat-label>
        <input matInput type="password" [(ngModel)]="nueva" autocomplete="new-password" />
        <mat-hint>Mínimo 8 caracteres.</mat-hint>
      </mat-form-field>

      <mat-form-field>
        <mat-label>Repetir la nueva</mat-label>
        <input matInput type="password" [(ngModel)]="repetida" autocomplete="new-password" />
      </mat-form-field>

      <div class="acciones">
        <button mat-button (click)="volver()">Volver</button>
        <button mat-flat-button (click)="cambiar()" [disabled]="cargando()">Cambiar</button>
      </div>
    </mat-card>
  `,
  styles: [
    `
      .tarjeta { max-width: 26rem; margin: 1rem auto; padding: 1.5rem; display: flex; flex-direction: column; gap: 0.75rem; }
      .guia { color: var(--mat-sys-on-surface-variant); font-size: 0.9rem; margin: 0 0 0.5rem; }
      .error, .bien { display: flex; align-items: center; gap: 0.4rem; margin: 0; font-size: 0.9rem; }
      .error { color: var(--mat-sys-error); }
      .bien { color: var(--mat-sys-primary); }
      .acciones { display: flex; justify-content: flex-end; gap: 0.5rem; }
    `,
  ],
})
export class MiClave {
  private api = inject(Api);
  private router = inject(Router);

  actual = '';
  nueva = '';
  repetida = '';

  readonly cargando = signal(false);
  readonly error = signal('');
  readonly hecho = signal('');

  volver() {
    void this.router.navigate(['/entregas']);
  }

  async cambiar() {
    this.error.set('');
    this.hecho.set('');
    // La repetición se comprueba AQUÍ y no en el servidor: es un error de
    // tecleo, no una regla del sistema. Mandarlo al servidor haría que la
    // segunda casilla pareciera algo que él conoce, y no la conoce.
    if (this.nueva !== this.repetida) {
      this.error.set('Las dos contraseñas nuevas no coinciden.');
      return;
    }
    this.cargando.set(true);
    try {
      await firstValueFrom(this.api.cambiarMiClave(this.actual, this.nueva));
      this.actual = '';
      this.nueva = '';
      this.repetida = '';
      this.hecho.set('Contraseña cambiada. Las sesiones de otros dispositivos han quedado cerradas.');
    } catch (e) {
      this.error.set((e as Error).message);
    } finally {
      this.cargando.set(false);
    }
  }
}
