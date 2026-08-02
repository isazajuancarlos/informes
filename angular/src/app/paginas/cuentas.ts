/**
 * Administración de cuentas: alta, rol, activación y restablecimiento de clave.
 *
 * Es la pantalla que faltaba para que el producto no exija abrir el SQLite del
 * cliente con `sqlite3` para dar de alta a un empleado.
 *
 * **Esta pantalla no autoriza nada.** Se enseña solo a quien el servidor dice que
 * es `Admin`, pero eso es comodidad de navegación: quien edite su rol en memoria
 * verá el formulario y recibirá un 403 en cuanto pulse. La decisión vive en
 * `cuentas::solo_admin`, en un solo sitio.
 */
import { Component, computed, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { MatButtonModule } from '@angular/material/button';
import { MatCardModule } from '@angular/material/card';
import { MatCheckboxModule } from '@angular/material/checkbox';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';
import { MatProgressBarModule } from '@angular/material/progress-bar';
import { MatSelectModule } from '@angular/material/select';
import { MatTableModule } from '@angular/material/table';
import { firstValueFrom } from 'rxjs';

import { Api, Cuenta, OpcionParte } from '../nucleo/api';
import { Sesion } from '../nucleo/sesion';

const ROLES = [
  { valor: 'ADMIN', etiqueta: 'Administrador' },
  { valor: 'OPERADOR', etiqueta: 'Operador' },
  { valor: 'SUPERVISOR', etiqueta: 'Supervisor' },
  { valor: 'TERCERO', etiqueta: 'Tercero (parte externa)' },
];

/** Lo que el formulario tiene entre manos. `id` nulo = alta. */
interface Edicion {
  id: number | null;
  nombre: string;
  email: string;
  clave: string;
  rol: string;
  stakeholder_id: number | null;
}

@Component({
  selector: 'siger-cuentas',
  imports: [
    FormsModule,
    MatButtonModule,
    MatCardModule,
    MatCheckboxModule,
    MatFormFieldModule,
    MatIconModule,
    MatInputModule,
    MatProgressBarModule,
    MatSelectModule,
    MatTableModule,
  ],
  template: `
    <header class="cabecera">
      <h2>Cuentas</h2>
      <button mat-flat-button (click)="nueva()"><mat-icon>person_add</mat-icon> Nueva cuenta</button>
    </header>

    @if (cargando()) { <mat-progress-bar mode="indeterminate" /> }
    @if (error()) {
      <mat-card class="aviso" role="alert"><mat-icon>error_outline</mat-icon> {{ error() }}</mat-card>
    }
    @if (hecho()) {
      <mat-card class="aviso bien" role="status"><mat-icon>check_circle</mat-icon> {{ hecho() }}</mat-card>
    }

    @if (edicion(); as ed) {
      <mat-card class="editor">
        <h3>{{ ed.id === null ? 'Nueva cuenta' : 'Editar «' + ed.nombre + '»' }}</h3>
        <div class="campos">
          <mat-form-field>
            <mat-label>Nombre</mat-label>
            <input matInput [(ngModel)]="ed.nombre" />
          </mat-form-field>

          <mat-form-field>
            <mat-label>Correo</mat-label>
            <!-- El correo es la clave de acceso: cambiarlo cambiaría con qué se
                 entra, así que en edición no se toca. Para eso se da de baja y se
                 crea otra, que deja rastro de las dos cosas. -->
            <input matInput [(ngModel)]="ed.email" [disabled]="ed.id !== null" />
            @if (ed.id !== null) { <mat-hint>El correo no se cambia: es con lo que se entra.</mat-hint> }
          </mat-form-field>

          @if (ed.id === null) {
            <mat-form-field>
              <mat-label>Contraseña inicial</mat-label>
              <input matInput type="password" [(ngModel)]="ed.clave" autocomplete="new-password" />
              <mat-hint>Mínimo 8 caracteres.</mat-hint>
            </mat-form-field>
          }

          <mat-form-field>
            <mat-label>Papel</mat-label>
            <mat-select [(ngModel)]="ed.rol">
              @for (r of roles; track r.valor) {
                <mat-option [value]="r.valor">{{ r.etiqueta }}</mat-option>
              }
            </mat-select>
          </mat-form-field>

          <!-- Solo un TERCERO lleva entidad: es el único rol para el que el
               servidor la mira al calcular el alcance. En los demás el servidor
               la rechaza, así que aquí ni se ofrece. -->
          @if (ed.rol === 'TERCERO') {
            <mat-form-field>
              <mat-label>Entidad externa</mat-label>
              <mat-select [(ngModel)]="ed.stakeholder_id">
                <mat-option [value]="null">Sin entidad (no verá nada)</mat-option>
                @for (s of entidades(); track s.id) {
                  <mat-option [value]="s.id">{{ s.nombre }}</mat-option>
                }
              </mat-select>
            </mat-form-field>
          }

          @if (ed.id !== null) {
            <mat-checkbox [(ngModel)]="activa">Cuenta activa</mat-checkbox>
          }
        </div>
        <div class="acciones">
          <button mat-button (click)="cancelar()">Cancelar</button>
          <button mat-flat-button (click)="guardar()" [disabled]="cargando()">Guardar</button>
        </div>
      </mat-card>
    }

    <table mat-table [dataSource]="cuentas()" class="tabla">
      <ng-container matColumnDef="nombre">
        <th mat-header-cell *matHeaderCellDef>Nombre</th>
        <td mat-cell *matCellDef="let c">{{ c.nombre }}</td>
      </ng-container>
      <ng-container matColumnDef="email">
        <th mat-header-cell *matHeaderCellDef>Correo</th>
        <td mat-cell *matCellDef="let c">{{ c.email }}</td>
      </ng-container>
      <ng-container matColumnDef="rol">
        <th mat-header-cell *matHeaderCellDef>Papel</th>
        <td mat-cell *matCellDef="let c">{{ etiquetaRol(c.rol) }}</td>
      </ng-container>
      <ng-container matColumnDef="estado">
        <th mat-header-cell *matHeaderCellDef>Estado</th>
        <td mat-cell *matCellDef="let c">
          @if (c.activo) { <span class="activa">Activa</span> } @else { <span class="baja">Desactivada</span> }
        </td>
      </ng-container>
      <ng-container matColumnDef="acciones">
        <th mat-header-cell *matHeaderCellDef></th>
        <td mat-cell *matCellDef="let c">
          <button mat-icon-button (click)="editar(c)" aria-label="Editar"><mat-icon>edit</mat-icon></button>
          <!-- Restablecer la PROPIA está deshabilitado: el servidor la rechaza
               con un 400 que manda a «cambiar mi contraseña», donde sí se pide la
               actual. Deshabilitarlo aquí evita ofrecer un botón que no funciona. -->
          <button mat-icon-button (click)="restablecer(c)"
                  [disabled]="c.id === yo()?.id"
                  title="Restablecer contraseña" aria-label="Restablecer contraseña">
            <mat-icon>lock_reset</mat-icon>
          </button>
          <!-- El rescate, y SOLO en las filas donde significa algo: quien no
               tiene segundo factor no tiene nada que retirar, y el propio se
               quita desde la pantalla que pide la contraseña. -->
          @if (c.segundo_factor && c.id !== yo()?.id) {
            <button mat-icon-button (click)="retirarSegundoFactor(c)"
                    title="Quitar la verificación en dos pasos"
                    aria-label="Quitar la verificación en dos pasos">
              <mat-icon>phonelink_erase</mat-icon>
            </button>
          }
        </td>
      </ng-container>
      <tr mat-header-row *matHeaderRowDef="columnas"></tr>
      <tr mat-row *matRowDef="let fila; columns: columnas" [class.fila-baja]="!fila.activo"></tr>
    </table>
  `,
  styles: [
    `
      .cabecera { display: flex; align-items: center; justify-content: space-between; margin-bottom: 1rem; }
      .aviso { display: flex; align-items: center; gap: 0.5rem; margin-bottom: 1rem; color: var(--mat-sys-error); }
      .aviso.bien { color: var(--mat-sys-primary); }
      .editor { margin-bottom: 1.25rem; padding: 1rem; }
      .campos { display: flex; flex-wrap: wrap; gap: 1rem; align-items: center; }
      .campos mat-form-field { min-width: 15rem; }
      .acciones { display: flex; justify-content: flex-end; gap: 0.5rem; margin-top: 0.75rem; }
      .tabla { width: 100%; }
      .activa { color: var(--mat-sys-primary); }
      .baja { color: var(--mat-sys-on-surface-variant); }
      .fila-baja { opacity: 0.6; }
    `,
  ],
})
export class Cuentas {
  private api = inject(Api);
  private sesion = inject(Sesion);

  readonly roles = ROLES;
  readonly columnas = ['nombre', 'email', 'rol', 'estado', 'acciones'];

  readonly cuentas = signal<Cuenta[]>([]);
  readonly entidades = signal<OpcionParte[]>([]);
  readonly edicion = signal<Edicion | null>(null);
  readonly cargando = signal(false);
  readonly error = signal('');
  readonly hecho = signal('');
  readonly yo = computed(() => this.sesion.actor());

  /** `activo` va aparte del resto de la edición: el checkbox lo escribe directo. */
  activa = true;

  constructor() {
    void this.recargar();
  }

  etiquetaRol(rol: string): string {
    return ROLES.find((r) => r.valor === rol)?.etiqueta ?? rol;
  }

  async recargar() {
    this.cargando.set(true);
    this.error.set('');
    try {
      this.cuentas.set(await firstValueFrom(this.api.cuentas()));
      this.entidades.set(await firstValueFrom(this.api.stakeholders()));
    } catch (e) {
      this.error.set((e as Error).message);
    } finally {
      this.cargando.set(false);
    }
  }

  nueva() {
    this.hecho.set('');
    this.activa = true;
    this.edicion.set({ id: null, nombre: '', email: '', clave: '', rol: 'OPERADOR', stakeholder_id: null });
  }

  editar(c: Cuenta) {
    this.hecho.set('');
    this.activa = c.activo;
    this.edicion.set({
      id: c.id,
      nombre: c.nombre,
      email: c.email,
      clave: '',
      rol: c.rol,
      stakeholder_id: c.stakeholder_id,
    });
  }

  cancelar() {
    this.edicion.set(null);
    this.error.set('');
  }

  async guardar() {
    const ed = this.edicion();
    if (!ed) return;
    this.cargando.set(true);
    this.error.set('');
    this.hecho.set('');
    // Si el papel no es TERCERO la entidad no significa nada, y el servidor la
    // rechaza. Se limpia aquí para que cambiar el papel en el formulario no deje
    // colgado un valor que el usuario ya no ve.
    const entidad = ed.rol === 'TERCERO' ? ed.stakeholder_id : null;
    try {
      if (ed.id === null) {
        await firstValueFrom(
          this.api.crearCuenta({
            nombre: ed.nombre,
            email: ed.email,
            clave: ed.clave,
            rol: ed.rol,
            stakeholder_id: entidad,
          }),
        );
        this.hecho.set(`Cuenta creada. Ya puede entrar con «${ed.email.trim().toLowerCase()}».`);
      } else {
        await firstValueFrom(
          this.api.actualizarCuenta(ed.id, {
            nombre: ed.nombre,
            rol: ed.rol,
            activo: this.activa,
            stakeholder_id: entidad,
          }),
        );
        this.hecho.set('Cuenta actualizada.');
      }
      this.edicion.set(null);
      await this.recargar();
    } catch (e) {
      // El formulario se queda abierto con lo escrito: cerrarlo tras un fallo
      // obliga a teclearlo todo otra vez, que es como se pierde un alta.
      this.error.set((e as Error).message);
    } finally {
      this.cargando.set(false);
    }
  }

  /**
   * Restablecer la contraseña de otra persona.
   *
   * Se avisa de que le cierra TODAS sus sesiones antes de hacerlo, porque es un
   * efecto que no se deshace y que quien lo pulsa puede no esperar: si esa
   * persona está a mitad de una entrega, la pierde.
   */
  async restablecer(c: Cuenta) {
    const clave = window.prompt(
      `Nueva contraseña para «${c.nombre}» (mínimo 8 caracteres).\n\n` +
        `Se le cerrarán TODAS las sesiones que tenga abiertas.`,
    );
    if (clave === null) return;
    this.cargando.set(true);
    this.error.set('');
    this.hecho.set('');
    try {
      await firstValueFrom(this.api.restablecerClave(c.id, clave));
      this.hecho.set(`Contraseña de «${c.nombre}» restablecida y sus sesiones cerradas.`);
    } catch (e) {
      this.error.set((e as Error).message);
    } finally {
      this.cargando.set(false);
    }
  }

  /**
   * Quitarle el segundo factor a quien perdió el teléfono y los códigos.
   *
   * Se confirma porque baja la protección de una cuenta ajena y no se deshace
   * solo: para volver a tenerla, esa persona tiene que enrolar otra vez desde su
   * pantalla. Se dice ahí, y no en un aviso aparte, porque es lo que hay que
   * saber ANTES de pulsar.
   */
  async retirarSegundoFactor(c: Cuenta) {
    const seguro = window.confirm(
      `¿Quitarle la verificación en dos pasos a «${c.nombre}»?\n\n` +
        `Volverá a entrar solo con su contraseña, y sus códigos de recuperación ` +
        `dejarán de valer. Para recuperarla tendrá que activarla de nuevo desde su cuenta.`,
    );
    if (!seguro) return;
    this.cargando.set(true);
    this.error.set('');
    this.hecho.set('');
    try {
      await firstValueFrom(this.api.retirarSegundoFactorDe(c.id));
      this.hecho.set(`«${c.nombre}» ya puede entrar con su contraseña; dígale que la active otra vez.`);
      await this.recargar();
    } catch (e) {
      this.error.set((e as Error).message);
    } finally {
      this.cargando.set(false);
    }
  }
}
